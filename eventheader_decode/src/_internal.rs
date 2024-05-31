// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#![doc(hidden)]
//! Internal implementation details for eventheader_decode and tests.
//! Contents subject to change without notice.

use core::fmt;
use core::fmt::Write;
use core::str;

use crate::EventHeaderItemInfo;
use crate::PerfConvertOptions;

#[inline]
fn to_hex_char_uppercase(nibble: u8) -> u8 {
    return match nibble & 0xF {
        0..=9 => b'0' + nibble,
        10..=15 => b'A' + nibble - 10,
        _ => unreachable!(),
    };
}

#[inline]
fn write_json_escaped_char<W: fmt::Write>(buffer: &mut W, value: char) -> Result<(), fmt::Error> {
    match value {
        '"' => buffer.write_str("\\\""),
        '\\' => buffer.write_str("\\\\"),
        '\x08' => buffer.write_str("\\b"),
        '\x0C' => buffer.write_str("\\f"),
        '\x0A' => buffer.write_str("\\n"),
        '\x0D' => buffer.write_str("\\r"),
        '\x09' => buffer.write_str("\\t"),
        '\0'..='\x1F' => unsafe {
            let c8 = value as u8;
            buffer.write_str(str::from_utf8_unchecked(&[
                b'\\',
                b'u',
                b'0',
                b'0',
                to_hex_char_uppercase(c8 / 16),
                to_hex_char_uppercase(c8),
            ]))
        },
        '\x20'..='\x7F' => buffer.write_str(unsafe { str::from_utf8_unchecked(&[value as u8]) }),
        _ => buffer.write_char(value),
    }
}

fn write_json_escaped<W: fmt::Write>(buffer: &mut W, value: &str) -> Result<(), fmt::Error> {
    for c in value.chars() {
        write_json_escaped_char(buffer, c)?;
    }
    return Ok(());
}

struct JsonEscapeWriter<'a, W: fmt::Write> {
    buffer: &'a mut W,
}

impl<'a, W: fmt::Write> JsonEscapeWriter<'a, W> {
    pub fn new(buffer: &'a mut W) -> JsonEscapeWriter<'a, W> {
        JsonEscapeWriter { buffer }
    }
}

impl<'a, W: fmt::Write> fmt::Write for JsonEscapeWriter<'a, W> {
    fn write_str(&mut self, value: &str) -> Result<(), fmt::Error> {
        return write_json_escaped(self.buffer, value);
    }
}

pub struct JsonWriter<'a, W: fmt::Write> {
    buffer: &'a mut W,
    comma: bool,
    current_space: bool,
    want_space: bool,
    want_field_tag: bool,
}

impl<'a, W: fmt::Write> JsonWriter<'a, W> {
    pub fn new(buffer: &'a mut W, options: PerfConvertOptions, comma: bool) -> JsonWriter<'a, W> {
        JsonWriter {
            buffer,
            comma,
            current_space: comma && options.has(PerfConvertOptions::Space),
            want_space: options.has(PerfConvertOptions::Space),
            want_field_tag: options.has(PerfConvertOptions::FieldTag),
        }
    }

    /// True if a comma should be written before the next value.
    pub fn comma(&self) -> bool {
        return self.comma;
    }

    /// For use before a value.
    /// Writes: comma?-newline-indent? i.e. `,\n  `
    pub fn write_newline_before_value(&mut self, indent: usize) -> Result<(), fmt::Error> {
        if self.comma {
            self.buffer.write_str(",\n")?;
        } else {
            self.buffer.write_str("\n")?;
        }
        if self.want_space {
            for _ in 0..indent {
                self.buffer.write_str("  ")?;
            }
        }
        self.comma = false;
        self.current_space = false;
        return Ok(());
    }

    /// Writes: `, "escaped-name":`
    pub fn write_property_name(&mut self, name: &str) -> Result<(), fmt::Error> {
        self.write_raw_comma_space()?;
        self.comma = false;

        self.buffer.write_str("\"")?;
        write_json_escaped(self.buffer, name)?;
        return self.buffer.write_str("\":");
    }

    /// Writes: `, "escaped-name;tag=0xTAG":`
    pub fn write_property_name_from_item_info(
        &mut self,
        item_info: &EventHeaderItemInfo,
    ) -> Result<(), fmt::Error> {
        self.write_raw_comma_space()?;
        self.comma = false;

        self.buffer.write_str("\"")?;

        let mut escape_writer = JsonEscapeWriter::new(self.buffer);
        item_info.name_chars().write_to(&mut escape_writer)?;

        if self.want_field_tag {
            let tag = item_info.metadata().field_tag();
            if tag != 0 {
                escape_writer.buffer.write_str(";tag=0x")?;
                escape_writer.write_fmt(format_args!("{:X}", tag))?;
            }
        }

        return self.buffer.write_str("\":");
    }

    /// Writes: `, "name":`
    pub fn write_property_name_json_safe(
        &mut self,
        json_safe_name: &str,
    ) -> Result<(), fmt::Error> {
        self.write_raw_comma_space()?;
        self.comma = false;

        self.buffer.write_str("\"")?;
        self.buffer.write_str(json_safe_name)?;
        return self.buffer.write_str("\":");
    }

    /// Writes: `, {`
    pub fn write_start_object(&mut self) -> Result<(), fmt::Error> {
        self.write_raw_comma_space()?;
        self.comma = false;

        return self.buffer.write_str("{");
    }

    /// Writes: ` }`
    pub fn write_end_object(&mut self) -> Result<(), fmt::Error> {
        self.comma = true;
        if self.current_space {
            self.buffer.write_str(" ")?;
        }
        return self.buffer.write_str("}");
    }

    /// Writes: `, [`
    pub fn write_start_array(&mut self) -> Result<(), fmt::Error> {
        self.write_raw_comma_space()?;
        self.comma = false;
        return self.buffer.write_str("[");
    }

    /// Writes: ` ]`
    pub fn write_end_array(&mut self) -> Result<(), fmt::Error> {
        self.comma = true;
        if self.current_space {
            self.buffer.write_str(" ")?;
        }
        return self.buffer.write_str("]");
    }

    /// Writes: `, "escaped-value"`
    pub fn write_string_value(&mut self, value: &str) -> Result<(), fmt::Error> {
        self.write_raw_comma_space()?;
        self.comma = true;

        self.buffer.write_str("\"")?;
        write_json_escaped(self.buffer, value)?;
        return self.buffer.write_str("\"");
    }

    /// Writes: `, "value"`
    pub fn write_string_value_json_safe(
        &mut self,
        json_safe_value: &str,
    ) -> Result<(), fmt::Error> {
        self.write_raw_comma_space()?;
        self.comma = true;

        self.buffer.write_str("\"")?;
        self.buffer.write_str(json_safe_value)?;
        return self.buffer.write_str("\"");
    }

    /// Writes: `, "escaped-fmt_args"`
    pub fn write_string_value_fmt(&mut self, fmt_args: fmt::Arguments) -> Result<(), fmt::Error> {
        self.write_raw_comma_space()?;
        self.comma = true;

        self.buffer.write_str("\"")?;
        JsonEscapeWriter::new(&mut self.buffer).write_fmt(fmt_args)?;
        return self.buffer.write_str("\"");
    }

    /// Writes: `, "fmt_args"`
    pub fn write_string_value_fmt_json_safe(
        &mut self,
        json_safe_fmt_args: fmt::Arguments,
    ) -> Result<(), fmt::Error> {
        self.write_raw_comma_space()?;
        self.comma = true;

        self.buffer.write_str("\"")?;
        self.buffer.write_fmt(json_safe_fmt_args)?;
        return self.buffer.write_str("\"");
    }

    /// Writes: `, value`
    pub fn write_value_json_safe(&mut self, json_safe_value: &str) -> Result<(), fmt::Error> {
        self.write_raw_comma_space()?;
        self.comma = true;

        return self.buffer.write_str(json_safe_value);
    }

    /// Writes: `, fmt_args`
    pub fn write_value_fmt_json_safe(
        &mut self,
        json_safe_fmt_args: fmt::Arguments,
    ) -> Result<(), fmt::Error> {
        self.write_raw_comma_space()?;
        self.comma = true;

        return self.buffer.write_fmt(json_safe_fmt_args);
    }

    /// Writes: `, `, does not update any state.
    fn write_raw_comma_space(&mut self) -> Result<(), fmt::Error> {
        if self.current_space {
            self.current_space = self.want_space;
            if self.comma {
                return self.buffer.write_str(", ");
            } else {
                return self.buffer.write_str(" ");
            }
        } else {
            self.current_space = self.want_space;
            if self.comma {
                return self.buffer.write_str(",");
            } else {
                return Ok(());
            }
        }
    }
}
