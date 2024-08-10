// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#![warn(missing_docs)]
#![allow(clippy::needless_return)]

//! perf.data file decoding

pub use file_reader::PerfDataFileError;
pub use file_reader::PerfDataFileEventOrder;
pub use file_reader::PerfDataFileReader;
pub use file_writer::PerfDataFileWriter;
pub use header_index::PerfHeaderIndex;

mod file_abi;
mod file_reader;
mod file_writer;
mod header_index;
mod input_file;
mod output_file;
