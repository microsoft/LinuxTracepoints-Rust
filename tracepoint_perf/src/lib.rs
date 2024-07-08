// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#![warn(missing_docs)]
#![allow(clippy::needless_return)]

//! perf.data file decoding

pub use header_index::PerfHeaderIndex;
pub use file_reader::PerfDataFileResult;
pub use file_reader::PerfDataFileEventOrder;

mod file_reader;
mod file_writer;
mod header_index;
