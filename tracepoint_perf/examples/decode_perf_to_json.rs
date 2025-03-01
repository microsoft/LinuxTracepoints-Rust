// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

//! Demonstrates how to use [`tp::PerfDataFileReader`] to decode events from a
//! `perf.data` file to JSON.

use core::fmt;
use std::env;
use std::fs;
use std::io;
use std::io::IsTerminal;
use std::process;

use tracepoint_decode as td;
use tracepoint_perf as tp;

const USAGE_COMMON: &str = "
Usage: decode_perf_to_json [options...] PerfDataFiles...
";

/// Usage error: stderr += USAGE_COMMON + USAGE_SHORT.
const USAGE_SHORT: &str = "
Usage: decode_perf_to_json [options...] PerfDataFiles...
";

/// -h or --help: stdout += USAGE_COMMON + USAGE_LONG.
const USAGE_LONG: &str = "
Converts perf.data files to JSON.

Options:

-o, --output <file> Set the output filename. The default is stdout.

-h, --help          Show this help message and exit.
";

fn main() -> process::ExitCode {
    let mut input_names = Vec::new();
    let mut output_name = String::new();
    let mut show_help = false;
    let mut usage_error = false;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if !arg.starts_with('-') {
            input_names.push(arg);
        } else if let Some(flag) = arg.strip_prefix("--") {
            match flag {
                "output" => {
                    if let Some(arg) = env::args().next() {
                        output_name = arg;
                    } else {
                        eprintln!("error: missing filename for flag --output.");
                        usage_error = true;
                    }
                }
                "help" => {
                    show_help = true;
                }
                _ => {
                    eprintln!("error: invalid flag \"--{}\".", flag);
                    usage_error = true;
                }
            }
        } else {
            let flags = &arg[1..];
            for flag in flags.chars() {
                match flag {
                    'o' => {
                        if let Some(arg) = args.next() {
                            output_name = arg;
                        } else {
                            eprintln!("error: missing filename for flag -o.");
                            usage_error = true;
                        }
                    }
                    'h' => {
                        show_help = true;
                    }
                    _ => {
                        eprintln!("error: invalid flag -{}.", flag);
                        usage_error = true;
                    }
                }
            }
        }
    }

    if show_help {
        println!("{}{}", USAGE_COMMON, USAGE_LONG);
        return process::ExitCode::SUCCESS;
    }

    if usage_error {
        eprintln!("{}{}", USAGE_COMMON, USAGE_SHORT);
        return process::ExitCode::FAILURE;
    }

    if input_names.is_empty() {
        eprintln!("error: no input files specified.");
        return process::ExitCode::FAILURE;
    }

    let is_tty;
    let mut output: Box<dyn io::Write> = if output_name.is_empty() {
        let stdout = io::stdout();
        is_tty = stdout.is_terminal();
        Box::new(stdout)
    } else {
        match fs::File::create(&output_name) {
            Ok(output) => {
                is_tty = output.is_terminal();
                Box::new(output)
            }
            Err(e) => {
                eprintln!(
                    "error: failed to open output file \"{}\": {}",
                    output_name, e
                );
                return process::ExitCode::FAILURE;
            }
        }
    };

    match write_json(&mut output, &input_names, is_tty) {
        Err(e) => {
            eprintln!("error: {}", e);
            process::ExitCode::FAILURE
        }
        Ok(exit_code) => exit_code,
    }
}

fn write_json(
    json_out: &mut Box<dyn io::Write>,
    input_names: &[String],
    is_tty: bool,
) -> io::Result<process::ExitCode> {
    let mut exit_code = process::ExitCode::SUCCESS;
    let mut comma;
    let mut json_buf = String::new();

    // JSON: {
    // Start of JSON.
    // Include a BOM if the output is not a TTY.
    writeln!(json_out, "{}", if is_tty { "{" } else { "\u{FEFF}{" })?;
    comma = false;

    // A reader can be reused for multiple files.
    let mut reader = tp::PerfDataFileReader::new();

    // An enumerator context can be used for multiple events.
    let mut enumerator_ctx = td::EventHeaderEnumeratorContext::new();

    for input_name in input_names {
        if comma {
            writeln!(json_out, ",")?;
        }

        // JSON: "input_name": [
        // Start of an input file.
        writeln!(
            json_out,
            " \"{}\": [",
            td::display::JsonEscapeDisplay::new(input_name)
        )?;
        comma = false;

        // The events can be processed in File order (the order they were written to the file)
        // or Time order (sorted by timestamp). For human-readable output, you usually want
        // Time order.
        if let Err(e) = reader.open_file(input_name, tp::PerfDataFileEventOrder::Time) {
            eprintln!("error: open_file(\"{}\") failed: {}", input_name, e);
            exit_code = process::ExitCode::FAILURE;
        } else {
            loop {
                match reader.move_next_event() {
                    Err(e) => {
                        eprintln!("error: read_event(\"{}\") failed: {}", input_name, e);
                        exit_code = process::ExitCode::FAILURE;
                        break; // Error, break out of loop.
                    }
                    Ok(false) => break, // EOF, break out of loop.
                    Ok(true) => {}      // Got an event, continue.
                }

                if comma {
                    writeln!(json_out, ",")?;
                }

                // JSON: {
                // Start of an event.
                write!(json_out, "  {{")?;

                let event = reader.current_event();
                if event.header.ty != td::PerfEventHeaderType::Sample {
                    // Non-sample event, typically contains information about the system or
                    // information about the trace itself.

                    // Event will be formatted as something like:
                    // { "NonSample": "Mmap", "size": 48, "meta": {...} }

                    // Event info (timestamp, cpu, pid, etc.) may be available.
                    let nonsample_event_info = reader.get_non_sample_event_info(&event);
                    if let Err(e) = &nonsample_event_info {
                        // Don't warn for IdNotFound errors.
                        if *e != tp::PerfDataFileError::IdNotFound {
                            eprintln!(
                                "warning: get_non_sample_event_info(\"{}\") failed: {}",
                                input_name, e
                            );
                        }
                    }

                    // JSON: "NonSample": "Type", "size": Size
                    // Note that nonsample_event_info.name() may be available, but it's usually not very informative.
                    write!(
                        json_out,
                        " \"NonSample\": \"{}\", \"size\": {}",
                        event.header.ty, event.header.size
                    )?;

                    if let Ok(nonsample_event_info) = nonsample_event_info {
                        // JSON: , "meta": {...}
                        write_json_meta(json_out, &nonsample_event_info.json_meta_display())?;
                    }
                } else {
                    // Sample event, e.g. tracepoint event.

                    // Event will be formatted as something like:
                    // { "n": "provider:name", "Field1": Value1, ..., "meta": {...} }

                    // Event info (timestamp, cpu, pid, etc.) should be available.
                    match reader.get_sample_event_info(&event) {
                        Err(e) => {
                            // Unexpected: Error getting event info.
                            eprintln!(
                                "warning: get_sample_event_info(\"{}\") failed: {}",
                                input_name, e
                            );

                            // JSON: "n": null, "get_sample_event_info": "Error", "size": Size
                            write!(
                                json_out,
                                " \"n\": null, \"get_sample_event_info\": \"{}\", \"size\":{}",
                                e, event.header.size
                            )?;
                        }
                        Ok(sample_event_info) => {
                            // Found event info (attributes). Include data from it in the output.

                            if let Ok(mut enumerator) = enumerator_ctx.enumerate(&sample_event_info)
                            {
                                // Decode using EventHeader metadata.

                                // event_info has a bunch of information about the event.
                                let eh_event_info = enumerator.event_info();

                                // JSON: "n": "Name"
                                write_json_n(json_out, &eh_event_info.json_identity_display())?;

                                // Current position is "before first item". The next sibling of the current
                                // position is "after last item". So write_json_item_and_move_next_sibling will
                                // write all items (if any) in the event.
                                json_buf.clear();
                                _ = enumerator.write_json_item_and_move_next_sibling(
                                    &mut json_buf, // fmt::Write works here, but io::Write doesn't. Use a String as a buffer.
                                    true,          // Include a ',' before the first item (if any).
                                    td::PerfConvertOptions::Default,
                                );

                                // JSON: , "Field1": Value1, "Field2": Value2
                                write!(json_out, "{}", json_buf)?;

                                if enumerator.state() == td::EventHeaderEnumeratorState::Error {
                                    // Unexpected: Error decoding event.
                                    eprintln!(
                                        "warning: move_next failed: {}",
                                        enumerator.last_error()
                                    );
                                }

                                // JSON: , "meta": {...}
                                write_json_meta(
                                    json_out,
                                    &eh_event_info.json_meta_display(Some(&sample_event_info)),
                                )?;
                            } else if let Some(event_format) = sample_event_info.format() {
                                // Decode using TraceFS format metadata.

                                // JSON: "n": "Name"
                                write_json_n(json_out, &sample_event_info.json_name_display())?;

                                // Typically the "common" fields are not interesting, so skip them.
                                let skip_fields = event_format.common_field_count();
                                for field_format in event_format.fields().iter().skip(skip_fields) {
                                    let field_value =
                                        field_format.get_field_value(&sample_event_info);

                                    // JSON: , "FieldName": Value
                                    write!(
                                        json_out,
                                        ", \"{}\": {}",
                                        td::display::JsonEscapeDisplay::new(field_format.name()),
                                        field_value.json_display(),
                                    )?;
                                }

                                // JSON: , "meta": {...}
                                write_json_meta(json_out, &sample_event_info.json_meta_display())?;
                            } else {
                                // Unexpected: Did not find TraceFS format metadata for this event.
                                eprintln!(
                                    "warning: no format found for event \"{}\"",
                                    sample_event_info.name(),
                                );

                                // JSON: "n": "Name"
                                write_json_n(json_out, &sample_event_info.json_name_display())?;

                                // JSON: , "meta": {...}
                                write_json_meta(json_out, &sample_event_info.json_meta_display())?;
                            }
                        }
                    }
                }

                // JSON: }
                // End of an event.
                write!(json_out, " }}")?;
                comma = true;
            }
        }

        // JSON: ]
        // End of an input file.
        write!(json_out, " ]")?;
        comma = true;
    }

    // JSON: }
    // End of JSON.
    writeln!(json_out, " }}")?;
    Ok(exit_code)
}

/// JSON: "n":"Name"
fn write_json_n<D: fmt::Display>(json_out: &mut dyn io::Write, name_display: &D) -> io::Result<()> {
    write!(json_out, " \"n\": \"{}\"", name_display)
}

/// JSON: , "meta": { ... }
fn write_json_meta<D: fmt::Display>(
    json_out: &mut dyn io::Write,
    meta_display: &D,
) -> io::Result<()> {
    write!(json_out, ", \"meta\": {{ {} }}", meta_display)
}
