// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

//! Demonstrates how to use [`tp::PerfDataFileWriter`] to write events to a
//! `perf.data` file. In this example, the events are read from an existing
//! `perf.data` file using [`tp::PerfDataFileReader`] and written to a new
//! `perf.data` file using [`tp::PerfDataFileWriter`].
//!
//! This is not intended to be useful (except perhaps for testing purposes). It is
//! intended to show how `PerfDataFileReader` can be used to take the `perf.data` file
//! apart and how `PerfDataFileWriter` can put it back together.
//!
//! Note that the output file is not expected to be exactly the same as the input:
//!
//! - Output is always a normal-mode file even if input was a pipe-mode file.
//! - Output file may store headers in a different order.
//! - Output file may use more/less padding.
//! - If the input file is semantically inconsistent, the output file may not
//!   precisely match the input (the inconsistent data might be lost). For
//!   example, there are usually two (or more) copies of each attr, one in a
//!   v1 format and another in a v2 format. The rewrite process will typically
//!   ignore the v1 copy of the data if a v2 copy is available, so if the v1 copy
//!   is semantically different from the v2 copy, that detail might be lost during
//!   rewrite.

use std::collections;
use std::env;
use std::fs;
use std::io;
use std::process;
use std::string;
use std::vec;

use tracepoint_decode as td;
use tracepoint_perf as tp;

fn main() -> process::ExitCode {
    let mut any_files_converted = false;
    let args = env::args();

    if args.len() <= 1 {
        eprintln!("\nUsage: rewrite_perf [perf.data] ... (will generate *.rewrite)",);
        return process::ExitCode::FAILURE;
    }

    let mut input = tp::PerfDataFileReader::new();
    let mut output = tp::PerfDataFileWriter::new();

    'next_file: for input_path in args.skip(1) {
        let mut output_path = string::String::new();
        output_path.push_str(&input_path);
        output_path.push_str(".rewrite");

        if let Err(e) = input.open_file(&input_path, tp::PerfDataFileEventOrder::File) {
            write_error_message(&input_path, e, "input.open_file failed, skipping");
            continue;
        }

        if input.byte_reader().byte_swap_needed() {
            // PerfDataFileWriter only supports creating host-endian files, so we can't
            // easily rewrite a byte-swapped input file.
            write_error_message(
                &input_path,
                io::ErrorKind::Unsupported.into(),
                "input file is byte-swapped, skipping",
            );
            continue;
        }

        if let Err(e) = output.create_file(&output_path) {
            write_error_message(&output_path, e, "output.create_file failed, skipping");
            continue;
        }

        let mut sample_ids_used = collections::HashSet::new();
        loop {
            match input.move_next_event() {
                Err(e) => {
                    write_error_message(
                        &input_path,
                        e,
                        "input.move_next_event failed, ignoring remainder of file",
                    );
                    break; // Error, break out of loop.
                }
                Ok(false) => break, // EOF, break out of loop.
                Ok(true) => {}      // Got an event, continue.
            }

            let event = input.current_event();
            match event.header.ty {
                td::PerfEventHeaderType::HeaderAttr => {
                    // Pseudo-event, conflicts with AddEventDesc.
                    // PerfDataFileReader automatically merges data from this event into its own
                    // EventDesc table. We'll use AddEventDesc to generate the output file's
                    // attr headers based on the merged EventDesc table.
                    continue;
                }
                td::PerfEventHeaderType::HeaderEventType => {
                    // Pseudo-event, conflicts with AddEventDesc.
                    // PerfDataFileReader could automatically merge data from this event into its
                    // own EventDesc table, but that is not implemented because this event
                    // type is deprecated. Instead, we'll just ignore this event type.
                    continue;
                }
                td::PerfEventHeaderType::HeaderTracingData => {
                    // Pseudo-event, conflicts with SetTracingData.
                    // PerfDataFileReader automatically merges data from this event into its own
                    // TracingData table. We'll use SetTracingData to generate the output file's
                    // tracing data based on the merged TracingData table.
                    continue;
                }
                td::PerfEventHeaderType::HeaderBuildId | td::PerfEventHeaderType::HeaderFeature => {
                    // Pseudo-events, conflict with SetHeader.
                    // PerfDataFileReader automatically merges data from these events into its own
                    // header table. We'll use SetHeader to generate the output file's headers
                    // based on the merged header table.
                    continue;
                }
                td::PerfEventHeaderType::Sample => {
                    // Sample event, need to populate output file's metadata.
                    match input.get_sample_event_info(&event) {
                        Err(e) => {
                            eprintln!(
                                "{}: warning {} : input.get_sample_event_info failed, deleting output file.",
                                &input_path,
                                e,
                            );
                            output.close_no_finalize();
                            _ = fs::remove_file(output_path);
                            continue 'next_file;
                        }
                        Ok(info) => {
                            let desc = info.event_desc;
                            if let Some(format) = desc.format_arc() {
                                if output.add_tracepoint_event_desc(
                                    desc.ids(),
                                    desc.attr(),
                                    desc.name(),
                                    format,
                                ) {
                                    // We don't need to add_event_desc for the IDs covered by this event_desc.
                                    for id in desc.ids() {
                                        sample_ids_used.insert(*id);
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {
                    // Non-sample event, no special treatment needed.
                }
            };

            if let Err(e) = output.write_event_data(event.data) {
                write_error_message(
                    &output_path,
                    e,
                    "output.write_event_data failed, deleting output file.",
                );
                output.close_no_finalize();
                _ = fs::remove_file(output_path);
                continue 'next_file;
            };
        }

        // Populate the output file's EventDesc table from the input file's table.
        // Some of this was already done by AddTracepointEventDesc.
        // In addition, the input file's table usually has duplicate entries - one entry with
        // names and one entry without names. Therefore, MergeEventDesc will skip ids that are
        // already populated, and we merge all descriptors with names before merging any
        // descriptors that don't have names.

        // Prefer data from descriptors that have names.
        for desc in input.event_desc_list() {
            if !desc.name().is_empty() {
                merge_event_desc(&mut output, &output_path, &mut sample_ids_used, desc);
            }
        }

        // Fill gaps (if any) using descriptors that don't have names.
        for desc in input.event_desc_list() {
            if desc.name().is_empty() {
                merge_event_desc(&mut output, &output_path, &mut sample_ids_used, desc);
            }
        }

        for i in tp::PerfHeaderIndex::FirstFeature.0..tp::PerfHeaderIndex::LastFeature.0 {
            match tp::PerfHeaderIndex(i) {
                tp::PerfHeaderIndex::TracingData | tp::PerfHeaderIndex::EventDesc => {
                    // Let the output file auto-populate these based on AddEventDesc and AddTracingData.
                    continue;
                }
                index => {
                    let header_data = input.header(index);
                    if !header_data.is_empty() {
                        // Copy the input file's merged header into the output file.
                        output.set_header(index, header_data);
                    }
                }
            }
        }

        output.set_tracing_data_required(
            input.tracing_data_long_size(),
            input.tracing_data_page_size(),
        );
        output.set_tracing_data_header_page(input.tracing_data_header_page());
        output.set_tracing_data_header_event(input.tracing_data_header_event());
        output.set_tracing_data_kallsyms(input.tracing_data_kallsyms());
        output.set_tracing_data_printk(input.tracing_data_printk());
        output.set_tracing_data_saved_cmd_line(input.tracing_data_saved_cmd_line());
        output.set_tracing_data_clear_ftraces();
        for index in 0..input.tracing_data_ftrace_count() {
            output.set_tracing_data_add_ftrace(input.tracing_data_ftrace(index));
        }

        if let Err(e) = output.finalize_and_close() {
            write_error_message(&output_path, e, "output.finalize_and_close failed");
            _ = fs::remove_file(output_path);
            continue;
        }

        println!("{} -> {}", input_path, output_path);

        any_files_converted = true;
    }

    if any_files_converted {
        process::ExitCode::SUCCESS
    } else {
        process::ExitCode::FAILURE
    }
}

fn merge_event_desc(
    output: &mut tp::PerfDataFileWriter,
    output_path: &str,
    sample_ids_used: &mut collections::HashSet<u64>,
    desc: &td::PerfEventDesc,
) {
    let ids = desc.ids();
    let mut sample_ids_buffer = vec::Vec::with_capacity(ids.len());
    for id in ids {
        if sample_ids_used.insert(*id) {
            sample_ids_buffer.push(*id);
        }
    }

    if !sample_ids_buffer.is_empty()
        && !output.add_event_desc(&sample_ids_buffer, desc.attr(), desc.name())
    {
        write_warning_message(
            output_path,
            io::ErrorKind::InvalidData.into(),
            "output.add_event_desc failed, metadata incomplete",
        );
    }
}

fn write_error_message(filename: &str, error: io::Error, context: &str) {
    eprintln!(
        "{}: error {} : {} ({}).",
        filename,
        error.kind(),
        context,
        error
    );
}

fn write_warning_message(filename: &str, error: io::Error, context: &str) {
    eprintln!(
        "{}: warning {} : {} ({}).",
        filename,
        error.kind(),
        context,
        error
    );
}
