// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

//! Demonstrates how to use [`tp::PerfDataFileReader`] to decode events from a
//! `perf.data` file to text.

use std::env;
use std::process;
use std::vec;

use tracepoint_decode as td;
use tracepoint_perf as tp;

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

    // A reader can be used for multiple files.
    let mut reader = tp::PerfDataFileReader::new();

    // An enumerator context can be used for multiple events.
    let mut enumerator_ctx = td::EventHeaderEnumeratorContext::new();

    // Buffer for working with JSON output.
    let mut json_buf = String::new();

    for filename in &filenames {
        println!("******* OpenFile: {}", filename);

        // The events can be processed in File order (the order they were written to the file)
        // or Time order (sorted by timestamp). For human-readable output, you usually want
        // Time order.
        if let Err(e) = reader.open_file(filename, tp::PerfDataFileEventOrder::Time) {
            eprintln!("Error {} open_file {}", e, filename);
            result = process::ExitCode::FAILURE;
            continue;
        }

        loop {
            match reader.move_next_event() {
                Err(e) => {
                    eprintln!("Error {} read_event {}", e, filename);
                    result = process::ExitCode::FAILURE;
                    break; // Error, break out of loop.
                }
                Ok(false) => break, // EOF, break out of loop.
                Ok(true) => {}      // Got an event, continue.
            }

            let event = reader.current_event();
            if event.header.ty != td::PerfEventHeaderType::Sample {
                // Non-sample event, typically contains information about the system or
                // information about the trace itself.

                // Event info (timestamp, cpu, pid, etc.) may be available.
                let nonsample_event_info = reader.get_non_sample_event_info(&event);
                if let Err(e) = &nonsample_event_info {
                    // Don't warn for IdNotFound errors.
                    if *e != tp::PerfDataFileError::IdNotFound {
                        println!("  get_non_sample_event_info error:  {}", e);
                    }
                }

                println!("NonSample: {}", event.header.ty);
                println!("  size = {}", event.header.size);

                if let Ok(nonsample_event_info) = nonsample_event_info {
                    // Event info was found. Include it in the output.
                    println!(
                        "  info = {{ {} }}",
                        nonsample_event_info.json_meta_display()
                    );
                }
            } else {
                // Sample event, e.g. tracepoint event.

                // Event info (timestamp, cpu, pid, etc.) may be available.
                let sample_event_info = match reader.get_sample_event_info(&event) {
                    Ok(sample_event_info) => sample_event_info,
                    Err(e) => {
                        println!("  get_sample_event_info error:  {}", e);
                        println!("  size = {}", event.header.size);
                        continue; // Usually can't make use of the event without the metadata.
                    }
                };

                println!("Sample: {}", sample_event_info.name());
                println!("  size = {}", event.header.size);

                if let Ok(mut enumerator) = enumerator_ctx.enumerate(&sample_event_info) {
                    // Decode using EventHeader metadata.

                    // event_info has a bunch of information about the event.
                    let eh_event_info = enumerator.event_info();

                    // Add the EventHeader-specific info.
                    println!(
                        "  info = {{ {} }}",
                        &eh_event_info.json_meta_display(Some(&sample_event_info))
                    );

                    // Transition past the initial BeforeFirstItem state. Otherwise, the
                    // first `write_json_item_and_move_next_sibling` would consume the entire event.
                    enumerator.move_next();

                    // This will loop once for each top-level item in the event.
                    while enumerator.state() >= td::EventHeaderEnumeratorState::BeforeFirstItem {
                        let item_info = enumerator.item_info(); // Information about the item.

                        // item_info.value has lots of properties and methods for accessing its data in different
                        // formats, but they only work for simple values -- scalar, array element, or array of
                        // fixed-size elements. For complex values such as structs or arrays of variable-size
                        // elements, you need to use the enumerator to access the sub-items.

                        // In this example, we use the enumerator to convert the current item to a JSON-formatted string.
                        // In the case of a simple item, it will be the same as `item_info.value().write_json_scalar_to()`.
                        // In the case of a complex item, it will recursively format the item and its sub-items.
                        json_buf.clear();
                        _ = enumerator.write_json_item_and_move_next_sibling(
                            &mut json_buf, // fmt::Write works here, but io::Write doesn't. Use a String as a buffer.
                            false,         // Don't put a ',' before the item.
                            td::PerfConvertOptions::Default
                                .and_not(td::PerfConvertOptions::RootName), // Don't include a JSON "ItemName": prefix.
                        );
                        println!("  {} = {}", item_info.name_and_tag_display(), json_buf);
                    }

                    if enumerator.state() == td::EventHeaderEnumeratorState::Error {
                        // Unexpected: Error decoding event.
                        println!("  MoveNext error: {}", enumerator.last_error());
                    }
                } else if let Some(event_format) = sample_event_info.format() {
                    // Decode using TraceFS format metadata.
                    println!("  info = {{ {} }}", sample_event_info.json_meta_display());

                    // Typically the "common" fields are not interesting, so skip them.
                    let skip_fields = event_format.common_field_count();
                    for field_format in event_format.fields().iter().skip(skip_fields) {
                        let field_value = field_format.get_field_value(&sample_event_info);

                        // field_value has lots of properties and methods for accessing its data in different
                        // formats. TraceFS fields are always scalars or arrays of fixed-size elements, so
                        // the following will work to get the data as a string value.
                        println!("  {} = {}", field_format.name(), field_value.display());
                    }
                } else {
                    // Unexpected: Did not find TraceFS format metadata for this event.
                    println!("  info = {{ {} }}", sample_event_info.json_meta_display());
                    println!("  no format");
                }
            }
        }
    }

    result
}

fn usage() -> process::ExitCode {
    eprintln!("Usage: decode_perf <filename1.data> [<filename2.data> ...]");
    process::ExitCode::FAILURE
}
