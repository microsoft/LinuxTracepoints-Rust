// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

extern crate alloc;

use core::fmt;

use alloc::string;
use alloc::vec;

use crate::*;
use perf_field_format::ascii_to_u32;
use perf_field_format::consume_string;
use perf_field_format::is_space_or_tab;

/// This macro is used in certain edge cases that I don't expect to happen in normal
/// `format` files. The code treats these as errors. The macro provides an easy way
/// to make an instrumented build that reports these cases.
///
/// At present, does nothing.
macro_rules! debug_eprintln {
    ($($arg:tt)*) => {};
}

/// Values for the DecodingStyle property of PerfEventFormat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PerfEventDecodingStyle {
    /// Event should be decoded using tracefs "format" file.
    TraceEventFormat,

    /// Event contains embedded "EventHeader" metadata and should be decoded using
    /// [`EventHeaderEnumerator`]. (TraceEvent decoding information is present, but the
    /// first TraceEvent-format field is named "eventheader_flags".)
    EventHeader,
}

/// Event information parsed from a tracefs "format" file.
#[derive(Debug)]
pub struct PerfEventFormat {
    system_name: string::String,
    name: string::String,
    print_fmt: string::String,
    fields: vec::Vec<PerfFieldFormat>,
    id: u32,
    common_field_count: u16,
    common_fields_size: u16,
    decoding_style: PerfEventDecodingStyle,
}

impl PerfEventFormat {
    /// Parses an event's "format" file and sets the fields of this object based
    /// on the results.
    ///
    /// - `long_is_64_bits`:
    ///   Indicates the size to use for "long" fields in this event.
    ///   true if sizeof(long) == 8, false if sizeof(long) == 4.
    ///
    /// - `system_name`:
    ///   The name of the system. For example, the system_name for "user_events:my_event"
    ///   would be "user_events".
    ///
    /// - `format_file_contents`:
    ///   The contents of the "format" file. This is typically obtained from tracefs,
    ///   e.g. the format_file_contents for "user_events:my_event" will usually be the
    ///   contents of "/sys/kernel/tracing/events/user_events/my_event/format".
    ///
    /// If "ID:" is a valid unsigned and and "name:" is not empty, returns
    /// a usable value. Otherwise, returns an `EMPTY` value.
    pub fn parse(
        long_is_64_bits: bool,
        system_name: &str,
        format_file_contents: &str,
    ) -> Option<Self> {
        let mut name = "";
        let mut print_fmt = "";
        let mut fields = vec::Vec::new();
        let mut id = None;
        let mut common_field_count = 0u16;

        let format_bytes = format_file_contents.as_bytes();

        // Search for lines like "NAME: VALUE..."
        let mut pos = 0;
        'NextLine: while pos < format_bytes.len() {
            // Skip any newlines.
            while is_eol_char(format_bytes[pos]) {
                pos += 1;
                if pos >= format_bytes.len() {
                    break 'NextLine;
                }
            }

            // Skip spaces.
            while is_space_or_tab(format_bytes[pos]) {
                debug_eprintln!("Space before propname in event");
                pos += 1; // Unexpected.
                if pos >= format_bytes.len() {
                    break 'NextLine;
                }
            }

            // "NAME:"
            let prop_name_pos = pos;
            while format_bytes[pos] != b':' {
                if is_eol_char(format_bytes[pos]) {
                    debug_eprintln!("EOL before ':' in format");
                    continue 'NextLine; // Unexpected.
                }

                pos += 1;

                if pos >= format_bytes.len() {
                    debug_eprintln!("EOF before ':' in format");
                    break 'NextLine; // Unexpected.
                }
            }

            let prop_name = &format_bytes[prop_name_pos..pos];
            pos += 1; // Skip ':'

            // Skip spaces.
            while pos < format_bytes.len() && is_space_or_tab(format_bytes[pos]) {
                pos += 1;
            }

            let prop_value_pos = pos;

            // "VALUE..."
            while pos < format_bytes.len() && !is_eol_char(format_bytes[pos]) {
                let consumed = format_bytes[pos];
                pos += 1;

                if consumed == b'"' {
                    pos = consume_string(pos, format_bytes, b'"');
                }
            }

            // Did we find something we can use?
            if prop_name == b"name" {
                name = &format_file_contents[prop_value_pos..pos];
            } else if prop_name == b"ID" && pos < format_bytes.len() {
                id = ascii_to_u32(&format_bytes[prop_value_pos..pos]);
            } else if prop_name == b"print fmt" {
                print_fmt = &format_file_contents[prop_value_pos..pos];
            } else if prop_name == b"format" {
                let mut common = true;
                fields.clear();

                // Search for lines like: " field:TYPE NAME; offset:N; size:N; signed:N;"
                while pos < format_bytes.len() {
                    debug_assert!(
                        is_eol_char(format_bytes[pos]),
                        "Loop should only repeat at EOL"
                    );

                    if format_bytes.len() - pos >= 2
                        && format_bytes[pos] == b'\r'
                        && format_bytes[pos + 1] == b'\n'
                    {
                        pos += 2; // Skip CRLF.
                    } else {
                        pos += 1; // Skip CR or LF.
                    }

                    let line_start_pos = pos;
                    while pos < format_bytes.len() && !is_eol_char(format_bytes[pos]) {
                        pos += 1;
                    }

                    if line_start_pos == pos {
                        // Blank line.
                        if common {
                            // First blank line means we're done with common fields.
                            common = false;
                            continue;
                        } else {
                            // Second blank line means we're done with format.
                            break;
                        }
                    }

                    let field = PerfFieldFormat::parse(
                        long_is_64_bits,
                        &format_file_contents[line_start_pos..pos],
                    );
                    if let Some(field) = field {
                        fields.push(field);
                        if common {
                            common_field_count += 1;
                        }
                    } else {
                        debug_eprintln!("Field parse failure");
                    }
                }
            }
        }

        match id {
            Some(id) if !name.is_empty() => {
                let common_fields_size = if common_field_count == 0 {
                    0
                } else {
                    let last_common_field = &fields[common_field_count as usize - 1];
                    last_common_field.offset() + last_common_field.size()
                };

                let decoding_style = if fields.len() > common_field_count as usize
                    && fields[common_field_count as usize].name() == "eventheader_flags"
                {
                    PerfEventDecodingStyle::EventHeader
                } else {
                    PerfEventDecodingStyle::TraceEventFormat
                };

                return Some(Self {
                    system_name: string::String::from(system_name),
                    name: string::String::from(name),
                    print_fmt: string::String::from(print_fmt),
                    fields,
                    id,
                    common_field_count,
                    common_fields_size,
                    decoding_style,
                });
            }
            _ => {
                return None;
            }
        }
    }

    /// Returns the value of the `system_name` parameter provided to the constructor,
    /// e.g. `"user_events"`.
    pub fn system_name(&self) -> &str {
        &self.system_name
    }

    /// Returns the value of the "name:" property, e.g. `"my_event"`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the value of the "print fmt:" property.
    pub fn print_fmt(&self) -> &str {
        &self.print_fmt
    }

    /// Returns the fields from the "format:" property.
    pub fn fields(&self) -> &[PerfFieldFormat] {
        &self.fields
    }

    /// Returns the value of the "ID:" property. Note that this value gets
    /// matched against the "common_type" field of an event, not the id field
    /// of perf_event_attr or PerfSampleEventInfo.
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Returns the number of "common_*" fields at the start of the event.
    /// User fields start at this index. At present, there are 4 common fields:
    /// common_type, common_flags, common_preempt_count, common_pid.
    pub fn common_field_count(&self) -> usize {
        self.common_field_count as usize
    }

    /// Returns the offset of the end of the last "common_*" field.
    /// This is the offset of the first user field.
    pub fn common_fields_size(&self) -> u16 {
        self.common_fields_size
    }

    /// Returns the detected event decoding system - `None`, `TraceEventFormat` or `EventHeader`.
    pub fn decoding_style(&self) -> PerfEventDecodingStyle {
        self.decoding_style
    }

    /// Writes a string representation of this format to the provided string.
    /// The string representation is in the format of a tracefs "format" file.
    pub fn write_to<W: fmt::Write>(&self, s: &mut W) -> fmt::Result {
        writeln!(s, "name: {}", self.name())?;
        writeln!(s, "ID: {}", self.id())?;
        s.write_str("format:\n")?;

        let common_field_count = self.common_field_count();
        for (i, field) in self.fields().iter().enumerate() {
            write!(
                s,
                "\tfield:{};\toffset:{};\tsize:{};",
                field.field(),
                field.offset(),
                field.size(),
            )?;
            if let Some(signed) = field.signed() {
                writeln!(s, "\tsigned:{};", signed as u8)?;
            } else {
                s.write_str("\n")?;
            }

            if i + 1 == common_field_count {
                s.write_str("\n")?;
            }
        }

        return writeln!(s, "\nprint fmt: {}", self.print_fmt());
    }
}

fn is_eol_char(c: u8) -> bool {
    c == b'\r' || c == b'\n'
}
