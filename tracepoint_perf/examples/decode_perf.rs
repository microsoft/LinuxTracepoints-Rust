// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

//! Demonstrates how to use [`tp::PerfDataFileReader`] to decode events from a
//! `perf.data` file.

use std::env;
use std::process;
use std::vec;

use tracepoint_decode as td;
use tracepoint_perf as tp;

fn main() -> process::ExitCode {
    let mut result = process::ExitCode::SUCCESS;

    let mut filenames = vec::Vec::new();

    //filenames.push("E:\\repos\\LinuxTracepoints-Net\\DecodeTest\\input\\perf.data".to_string());
    //filenames.push("E:\\repos\\LinuxTracepoints-Net\\DecodeTest\\input\\pipe.data".to_string());

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

    // A reader can be reused for multiple files.
    let mut reader = tp::PerfDataFileReader::new();

    'next_file: for filename in &filenames {
        println!("Processing: {}", filename);

        // The events can be processed in File order (the order they were written to the file)
        // or Time order (sorted by timestamp). For human-readable output, you usually want
        // Time order.
        if let Err(e) = reader.open_file(filename, tp::PerfDataFileEventOrder::File) {
            eprintln!("Error {} open_file {}", e, filename);
            result = process::ExitCode::FAILURE;
            continue 'next_file;
        }

        loop {
            match reader.move_next_event() {
                Err(e) => {
                    eprintln!("Error {} read_event {}", e, filename);
                    result = process::ExitCode::FAILURE;
                    continue 'next_file;
                }
                Ok(false) => break, // EOF
                Ok(true) => {
                    let event_bytes = reader.current_event();
                    if event_bytes.header.header_type == td::PerfEventHeaderType::Sample {
                        match reader.get_sample_event_info(&event_bytes) {
                            Err(e) => {
                                eprintln!("  Warning {} get_sample_event_info", e);
                            }
                            Ok(info) => {
                                println!("  Sample event: {}", info.name());
                            }
                        }
                    } else if event_bytes.header.header_type < td::PerfEventHeaderType::UserTypeStart {
                        match reader.get_non_sample_event_info(&event_bytes) {
                            Err(e) => {
                                eprintln!("  Warning {} get_non_sample_event_info", e);
                            }
                            Ok(info) => {
                                println!("  Non-sample event: {}", info.name());
                            }
                        }
                    } else {
                        println!("  Non-sample core event: {}", event_bytes.header.header_type);
                    }
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
