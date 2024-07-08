// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

use std::fmt;
use std::collections::HashMap;
use std::vec;
use tracepoint_decode::*;

use crate::header_index::PerfHeaderIndex;

#[derive(Debug)]
struct Buffer {
    data: vec::Vec<u8>,
    pos: usize,
}

/// Status returned by ReadEvent, GetSampleEventInfo, and GetNonSampleEventInfo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PerfDataFileResult {
    /// The operation succeeded.
    Ok,

    /// ReadEvent:
    /// No more events because the end of the file was reached.
    EndOfFile,

    /// ReadEvent:
    /// No more events because the file contains invalid data.
    ///
    /// GetSampleEventInfo or GetNonSampleEventInfo:
    /// Failed to get event info because the event contains invalid data.
    InvalidData,

    /// GetSampleEventInfo or GetNonSampleEventInfo:
    /// The event's ID was not found in the event attr table.
    IdNotFound,

    /// GetSampleEventInfo:
    /// Failed to get event info because the event contains headers that this
    /// decoder cannot parse.
    NotSupported,

    /// GetSampleEventInfo or GetNonSampleEventInfo:
    /// Cannot get sample information because the event's ID was not collected in
    /// the trace (the event's sample_type did not include PERF_SAMPLE_ID or
    /// PERF_SAMPLE_IDENTIFIER).
    NoData,
}

impl fmt::Display for PerfDataFileResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PerfDataFileResult::Ok => f.pad("Ok"),
            PerfDataFileResult::EndOfFile => f.pad("EndOfFile"),
            PerfDataFileResult::InvalidData => f.pad("InvalidData"),
            PerfDataFileResult::IdNotFound => f.pad("IdNotFound"),
            PerfDataFileResult::NotSupported => f.pad("NotSupported"),
            PerfDataFileResult::NoData => f.pad("NoData"),
        }
    }
}

/// The order in which events are returned by ReadEvent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PerfDataFileEventOrder {
    /// Events are returned in the order they appear in the file.
    File,

    /// Events are sorted by timestamp, with ties broken by the order they appear
    /// in the file. Events with no timestamp are treated as having timestamp 0.
    ///
    /// More precisely: The file is split into "rounds" based on FinishedInit event,
    /// FinishedRound event, and EndOfFile. Within each round, events are
    /// stable-sorted by the event's timestamp.
    Time,
}

#[derive(Debug)]
pub struct PerfDataFileReader {
    file_pos: u64,
    file_len: u64,
    data_begin_file_pos: u64,
    data_end_file_pos: u64,
    buffers: vec::Vec<Buffer>,
    headers: [Buffer; PerfHeaderIndex::LastFeature.0 as usize],
    event_desc_list: vec::Vec<PerfEventDesc>,
    event_desc_id_to_index: HashMap<u64, usize>,
}

impl Drop for PerfDataFileReader {
    fn drop(&mut self) {
        todo!();
    }
}
