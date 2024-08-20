// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

//! Demonstrates how to use [`td::EventHeaderEnumerator`] to decode events.
//!
//! `EventHeaderEnumerator` requires the event's tracepoint name and the event's
//! user data as input. In practice, you would get this information from trace buffers
//! or from a trace log file. For purposes of this sample, we read this information from
//! a `.dat` file, e.g. from `tracepoint_decode/test_data/EventHeaderInterceptorLE64.dat`.
//!
//! The `.dat` file consists of a sequence of events. Each event is a little-endian
//! 32-bit integer (size of the event record in bytes; size includes the 4-byte size),
//! followed by a null-terminated string (the tracepoint name), followed by the event
//! user data (the remainder of the record).

use std::env;
use std::fs;
use std::process;
use std::str;
use std::vec;

use tracepoint_decode as td;
use tracepoint_decode::EventHeaderEnumeratorContext;

fn main() -> process::ExitCode {
    let mut result = process::ExitCode::SUCCESS;

    let mut filenames = vec::Vec::new();
    for arg in env::args().skip(1) {
        if arg.starts_with('-') {
            eprintln!("Unknown option: {}", arg);
            return usage();
        } else {
            filenames.push(arg);
        }
    }

    if filenames.is_empty() {
        eprintln!("No input files specified.");
        return usage();
    }

    for filename in &filenames {
        println!("Processing: {}", filename);
        match fs::read(filename) {
            Err(e) => {
                eprintln!("Error {} reading from {}", e, filename);
                result = process::ExitCode::FAILURE;
            }
            Ok(file_bytes) => {
                if !print_events_from_dat_file(filename, &file_bytes) {
                    result = process::ExitCode::FAILURE;
                }
            }
        }
    }

    result
}

fn usage() -> process::ExitCode {
    eprintln!("Usage: decode_dat <filename1.dat> [<filename2.dat> ...]");
    process::ExitCode::FAILURE
}

fn print_events_from_dat_file(filename: &str, file_bytes: &[u8]) -> bool {
    let mut parser = DatFileParser {
        file_bytes,
        filename,
        file_pos: 0,
        any_errors: false,
    };

    // `EventHeaderEnumeratorContext` contains the state of an enumeration.
    // You can use the same context to enumerate multiple events.
    // The context is not particularly expensive to create, but reusing the
    // context for multiple events saves a bit of CPU overhead per event.
    let mut enumerator_context = td::EventHeaderEnumeratorContext::new();

    let mut value_json = String::new();
    while let Some((tracepoint_name, event_data)) = parser.next() {
        // Begin enumeration by calling `enumerator_context.enumerate()` with the tracepoint
        // name (e.g. "MyEvent_L1K1") and the event data (the "user" data of the event, starting
        // immediately after the common fields).
        match enumerator_context.enumerate_with_name_and_data(
            tracepoint_name,
            event_data,
            EventHeaderEnumeratorContext::MOVE_NEXT_LIMIT_DEFAULT,
        ) {
            Err(e) => {
                // Header of the event was invalid.
                eprintln!(
                    "Error {} enumerating {} in {}",
                    e, tracepoint_name, filename,
                );
                parser.any_errors = true;
            }
            Ok(mut enumerator) => {
                // Header of the event was valid. You can now use the enumerator to get information
                // about the event and about the event fields. You can also use the enumerator to
                // format the fields, the event info, or the entire event as a string.

                // event_info includes event attributes: name, data, activity_id, related_activity_id,
                // level, keyword, opcode, etc. It also has helpers for formatting these attributes as
                // strings, such as `identity_display()`, `json_meta_display()`, `name_display()`.
                let event_info = enumerator.event_info();
                println!(
                    "- {} {{ {} }}",
                    event_info.identity_display(), // "ProviderName:EventName"
                    event_info.json_meta_display(None), // Various event attributes, formatted as JSON.
                );

                // The enumerator starts at position before-first-item. If there are no fields,
                // move_next will move to after-last-item and return false.
                enumerator.move_next();

                // Loop as long as we're on a valid item.
                // (Alternative: loop forever, break out if move_next returns false.)
                while enumerator.state().can_item_info() {
                    // There's an item. It might be a simple field, an array-begin, an array-end, a
                    // struct-begin, or a struct-end.
                    let item_info = enumerator.item_info();
                    let item_value = item_info.value();

                    match enumerator.state() {
                        td::EventHeaderEnumeratorState::Value => {
                            // This is a scalar field.
                            println!(
                                "  - {} = {}",
                                item_info.name_display(),
                                item_value.display()
                            );

                            // You could also use methods on the `item_value` to do things like:
                            // - Determine the value type, e.g. u32, uuid, utf-8 string.
                            // - Cast the value to u32, Guid, string, etc.

                            // Move to the next item.
                            enumerator.move_next();
                        }
                        td::EventHeaderEnumeratorState::ArrayBegin
                        | td::EventHeaderEnumeratorState::StructBegin => {
                            // This is a complex field (array or struct).
                            // Current state is ArrayBegin or StructBegin.

                            // If we wanted to get to the simple fields, we would call move_next to enter
                            // the array or struct and enumerate the items within the array or struct.

                            // In the special case of an array of simple (fixed-length) fields (e.g. u32, Guid,
                            // float, but not string or binary blob), we could access the elements of the array
                            // using methods on the value object.

                            // For this example, we'll instead use the enumerator to format the entire field as
                            // a single JSON string. This also consumes the field, moving the enumerator to the
                            // next sibling (i.e. after the corresponding ArrayEnd or StructEnd).
                            const CONVERT_OPTIONS: td::PerfConvertOptions =
                                td::PerfConvertOptions::Default
                                    .and_not(td::PerfConvertOptions::RootName);
                            enumerator
                                .write_json_item_and_move_next_sibling(
                                    &mut value_json,
                                    false,
                                    CONVERT_OPTIONS,
                                )
                                .unwrap();
                            println!("  + {} = {}", item_info.name_display(), &value_json,);
                            value_json.clear();
                        }
                        _ => {
                            // This is an unexpected state because:
                            // - while loop exits for Error or AfterLastItem.
                            // - We skipped past the BeforeFirstItem state.
                            // - We handled Value, ArrayBegin, and StructBegin.
                            // - ArrayBegin/StructBegin case consumes up through the corresponding ArrayEnd/StructEnd.
                            eprintln!("  - Unexpected state: {:?}", enumerator.state());
                            enumerator.move_next();
                        }
                    }
                }
            }
        }
    }

    !parser.any_errors
}

// Parser for the .dat file format.
struct DatFileParser<'a, 'b> {
    file_bytes: &'a [u8],
    filename: &'b str,
    file_pos: usize,
    any_errors: bool,
}

impl<'a, 'b> DatFileParser<'a, 'b> {
    fn next(&mut self) -> Option<(&str, &'a [u8])> {
        'retry: loop {
            let event_begin_pos = self.file_pos;
            let mut pos = event_begin_pos;
            let remaining_size = self.file_bytes.len() - pos;
            if remaining_size <= 4 {
                if remaining_size != 0 {
                    eprintln!("Early EOF at pos {} in {}", event_begin_pos, self.filename);
                    self.any_errors = true;
                }
                return None;
            }

            let event_size =
                u32::from_le_bytes(self.file_bytes[pos..pos + 4].try_into().unwrap()) as usize;
            pos += 4;
            if event_size <= 4 || event_size > remaining_size {
                eprintln!(
                    "Invalid size {} at pos {} in {}",
                    event_size, event_begin_pos, self.filename
                );
                self.any_errors = true;
                return None;
            }

            let event_end_pos = event_begin_pos + event_size;
            self.file_pos = event_end_pos;

            let tracepoint_name_start = pos;
            while self.file_bytes[pos] != 0 {
                pos += 1;
                if pos == event_end_pos {
                    eprintln!(
                        "Unterminated string at pos {} in {}",
                        tracepoint_name_start, self.filename
                    );
                    self.any_errors = true;
                    continue 'retry;
                }
            }
            let tracepoint_name_bytes = &self.file_bytes[tracepoint_name_start..pos];
            pos += 1; // Skip the nul.

            let tracepoint_name = match str::from_utf8(tracepoint_name_bytes) {
                Err(e) => {
                    eprintln!(
                        "Invalid UTF-8 at pos {} in {}: {}",
                        tracepoint_name_start, self.filename, e
                    );
                    self.any_errors = true;
                    continue 'retry;
                }
                Ok(s) => s,
            };

            return Some((tracepoint_name, &self.file_bytes[pos..event_end_pos]));
        }
    }
}
