// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

use crate::*;
use core::ops;

/// Represents the header and raw data of a perf event.
///
/// - If this is a sample event (i.e. if `header.header_type == PerfEventHeaderType::Sample`),
///   you will usually need to get additional information about the event (timestamp,
///   cpu, decoding information, etc.) by calling `reader.get_sample_event_info(&bytes)`.
///
/// - If this is a non-sample event (i.e. if `header.header_type != PerfEventHeaderType::Sample`),
///   you may be able to get additional information about the event (timestamp, cpu, etc.)
///   by calling `reader.get_non_sample_event_info(&bytes)`. However, this is not always
///   necessary, e.g. in many cases the `header_type` alone is all you need, and in other
///   cases, the payload is in a known location within the event content. In addition, many
///   non-sample events do not support this additional information, e.g. if
///   `header_type >= UserTypeStart` or if the event appears before the `FinishedInit`
///   event has been processed.
#[derive(Clone, Copy, Debug, Default)]
pub struct PerfEventBytes<'dat> {
    /// The header of the event, in host-endian byte order.
    ///
    /// The header comes from `data[0..8]`, but has been byte-swapped if appropriate (i.e.
    /// if the event byte order does not match the host byte order).
    pub header: PerfEventHeader,

    /// The bytes of the event, including header and content, in event byte order.
    ///
    /// If this is a `Sample` event (i.e. if `header.header_type == PerfEventHeaderType::Sample`)
    /// then `data` will contain:
    ///
    /// - The 8 byte header (same data as `header`, but in event byte order).
    /// - A sequence of fields, one field for each bit set in the event's `sample_type`.
    ///   These fields include items like `id`, `time`, `ip`, `cpu`, `pid`, `tid`, and raw
    ///   data.
    ///   - The raw data field contains the content of the event, which includes both
    ///     "common" fields (the same for all `Sample` events on the current system) and
    ///     "user" fields (different for different tracepoints). The raw data should be
    ///     decoded uses the event's `format` (if available).
    ///
    /// You will normally use `reader.get_sample_event_info(&bytes)` to help parse the
    /// fields from the data of the `Sample` event.
    ///
    /// If this is a non-sample event (i.e. if `header.header_type != PerfEventHeaderType::Sample`)
    /// then `data` will contain:
    ///
    /// - The 8 byte header (same data as `header`, but in event byte order).
    /// - The content of the event (if any), which is in a format determined by the event's
    ///   `header_type`.
    /// - The event may contain additional fields after the content, one field for each bit
    ///   set in the event's `sample_type`. These fields include items like `id`, `time`,
    ///   `cpu`, pid, tid.
    ///
    /// If additional fields are present, you will normally use
    /// `reader.get_non_sample_event_info(&bytes)` to parse them. Note that these addtional
    /// fields are not always present. In particular, they are generally not present for
    /// events with `header_type >= UserTypeStart` or for events that appear before the
    /// `FinishedInit` event has been processed.
    pub data: &'dat [u8],
}

impl<'dat> PerfEventBytes<'dat> {
    /// Constructs a new PerfEventBytes instance.
    pub const fn new(header: PerfEventHeader, data: &'dat [u8]) -> PerfEventBytes<'dat> {
        debug_assert!(data.len() >= PerfEventHeader::SIZE_OF);
        return PerfEventBytes { header, data };
    }
}

/// Information about a non-sample event, typically returned by
/// `reader.get_non_sample_event_info(&bytes)`.
#[derive(Clone, Debug)]
pub struct PerfNonSampleEventInfo<'a> {
    /// The bytes of the event, including header and content, in event byte order.
    ///
    /// The data consists of the 8-byte header followed by the content, both in event byte order.
    /// The format of the content depends on `header_type`.
    ///
    /// Valid always.
    pub data: &'a [u8],

    /// Information about the session that collected the event, e.g. clock id and
    /// clock offset.
    ///
    /// Valid always.
    pub session_info: &'a PerfSessionInfo,

    /// Information about the event (shared by all events with the same `id`).
    ///
    /// Valid always.
    pub event_desc: &'a PerfEventDesc,

    /// Valid if `sample_type()` contains `Identifier` or `Id`.
    pub id: u64,

    /// Valid if `sample_type()` contains `Cpu`.
    pub cpu: u32,

    /// Valid if `sample_type()` contains `Cpu`.
    pub cpu_reserved: u32,

    /// Valid if `sample_type()` contains `StreamId`.
    pub stream_id: u64,

    /// Use SessionInfo.TimeToTimeSpec() to convert to a TimeSpec.
    ///
    /// Valid if `sample_type()` contains `Time`.
    pub time: u64,

    /// Valid if `sample_type()` contains `Tid`.
    pub pid: u32,

    /// Valid if `sample_type()` contains `Tid`.
    pub tid: u32,
}

impl<'a> PerfNonSampleEventInfo<'a> {
    /// Constructs a new PerfNonSampleEventInfo instance.
    /// Requires that `data` is at least 8 bytes long (must start with the [`PerfEventHeader`]).
    pub const fn new(
        data: &'a [u8],
        session_info: &'a PerfSessionInfo,
        event_desc: &'a PerfEventDesc,
    ) -> Self {
        debug_assert!(data.len() >= PerfEventHeader::SIZE_OF);
        return Self {
            data,
            session_info,
            event_desc,
            id: 0,
            cpu: 0,
            cpu_reserved: 0,
            stream_id: 0,
            time: 0,
            pid: 0,
            tid: 0,
        };
    }

    /// Returns true if the the session's event data is formatted in big-endian
    /// byte order. (Use `byte_reader()` to do byte-swapping as appropriate.)
    pub const fn source_big_endian(&self) -> bool {
        self.session_info.source_big_endian()
    }

    /// Returns a [`PerfByteReader`] configured for the byte order of the events
    /// in this session, i.e. `PerfByteReader::new(source_big_endian())`.
    pub const fn byte_reader(&self) -> PerfByteReader {
        self.session_info.byte_reader()
    }

    /// Returns the header of the event, in host-endian byte order.
    /// (Reads the header from `data[0..8]` and byte-swaps as appropriate based on
    /// the session's byte order.)
    pub fn header(&self) -> PerfEventHeader {
        let array = self.data[..8].try_into().unwrap();
        return PerfEventHeader::from_bytes(&array, self.session_info.byte_reader());
    }

    /// Returns flags indicating which data was present in the event.
    pub const fn sample_type(&self) -> PerfEventAttrSampleType {
        self.event_desc.attr().sample_type
    }

    /// Gets the event's name, e.g. "sched:sched_switch".
    /// - If name is available from `PERF_HEADER_EVENT_DESC`, return it.
    /// - Otherwise, return empty string.
    pub fn name(&self) -> &str {
        self.event_desc.name()
    }

    /// Gets the event's `time` as a [`PerfTimeSpec`], using offset information from `session_info`.
    pub const fn time_spec(&self) -> PerfTimeSpec {
        self.session_info.time_to_time_spec(self.time)
    }
}

/// Information about a sample event, typically returned by
/// `reader.get_sample_event_info(&bytes)`.
///
/// If the `format()` property is non-empty, you can use it to access event
/// information, including the event's fields.
#[derive(Clone, Debug)]
pub struct PerfSampleEventInfo<'a> {
    /// The bytes of the event, including header and content, in event byte order.
    ///
    /// The data consists of the 8-byte header followed by the content, both in event byte order.
    /// The format of the content depends on `header_type`.
    ///
    /// Valid always.
    pub data: &'a [u8],

    /// Information about the session that collected the event, e.g. clock id and
    /// clock offset.
    ///
    /// Valid always.
    pub session_info: &'a PerfSessionInfo,

    /// Information about the event (shared by all events with the same `id`).
    ///
    /// Valid always.
    pub event_desc: &'a PerfEventDesc,

    /// Valid if `sample_type()` contains `Identifier` or `Id`.
    pub id: u64,

    /// Valid if `sample_type()` contains `IP`.
    pub ip: u64,

    /// Valid if `sample_type()` contains `Tid`.
    pub pid: u32,

    /// Valid if `sample_type()` contains `Tid`.
    pub tid: u32,

    /// Use SessionInfo.TimeToTimeSpec() to convert to a TimeSpec.
    ///
    /// Valid if `sample_type()` contains `Time`.
    pub time: u64,

    /// Valid if `sample_type()` contains `Addr`.
    pub addr: u64,

    /// Valid if `sample_type()` contains `StreamId`.
    pub stream_id: u64,

    /// Valid if `sample_type()` contains `Cpu`.
    pub cpu: u32,

    /// Valid if `sample_type()` contains `Cpu`.
    pub cpu_reserved: u32,

    /// Valid if `sample_type()` contains `Period`.
    pub period: u64,

    /// Read format data.
    ///
    /// Valid if `sample_type()` contains `Read`.
    pub read_range: ops::Range<u16>,

    /// Callchain data.
    ///
    /// Valid if `sample_type()` contains `Callchain`.
    pub callchain_range: ops::Range<u16>,

    /// Raw event data.
    ///
    /// Valid if `sample_type()` contains `Raw`.
    pub raw_range: ops::Range<u16>,
}

impl<'a> PerfSampleEventInfo<'a> {
    /// Constructs a new PerfSampleEventInfo instance.
    /// Requires that `data` is at least 8 bytes long (must start with the [`PerfEventHeader`]).
    pub const fn new(
        data: &'a [u8],
        session_info: &'a PerfSessionInfo,
        event_desc: &'a PerfEventDesc,
    ) -> Self {
        debug_assert!(data.len() >= PerfEventHeader::SIZE_OF);
        return Self {
            data,
            session_info,
            event_desc,
            id: 0,
            ip: 0,
            pid: 0,
            tid: 0,
            time: 0,
            addr: 0,
            stream_id: 0,
            cpu: 0,
            cpu_reserved: 0,
            period: 0,
            read_range: 0..0,
            callchain_range: 0..0,
            raw_range: 0..0,
        };
    }

    /// Returns true if the the session's event data is formatted in big-endian
    /// byte order. (Use `byte_reader()` to do byte-swapping as appropriate.)
    pub const fn source_big_endian(&self) -> bool {
        self.session_info.source_big_endian()
    }

    /// Returns a [`PerfByteReader`] configured for the byte order of the events
    /// in this session, i.e. `PerfByteReader::new(source_big_endian())`.
    pub const fn byte_reader(&self) -> PerfByteReader {
        self.session_info.byte_reader()
    }

    /// Returns the header of the event, in host-endian byte order.
    /// (Reads the header from `data[0..8]` and byte-swaps as appropriate based on
    /// the session's byte order.)
    pub fn header(&self) -> PerfEventHeader {
        let array = self.data[..8].try_into().unwrap();
        return PerfEventHeader::from_bytes(&array, self.session_info.byte_reader());
    }

    /// Returns flags indicating which data was present in the event.
    pub const fn sample_type(&self) -> PerfEventAttrSampleType {
        self.event_desc.attr().sample_type
    }

    /// Gets the event's name, e.g. "sched:sched_switch".
    /// - If name is available from `PERF_HEADER_EVENT_DESC`, return it.
    /// - Otherwise, if name is available from format, return it.
    /// - Otherwise, return empty string.
    pub fn name(&self) -> &str {
        self.event_desc.name()
    }

    /// Gets the event's `time` as a [`PerfTimeSpec`], using offset information from `session_info`.
    pub const fn time_spec(&self) -> PerfTimeSpec {
        self.session_info.time_to_time_spec(self.time)
    }

    /// Event's format, or an empty format if no format data available.
    pub fn format(&self) -> &PerfEventFormat {
        self.event_desc.format()
    }

    /// Gets the read format data from the event in event-endian byte order.
    ///
    /// Valid if `sample_type()` contains `Read`.
    pub fn read_format(&self) -> &'a [u8] {
        &self.data[self.read_range.start as usize..self.read_range.end as usize]
    }

    /// Gets the callchain data from the event in event-endian byte order.
    ///
    /// Valid if `sample_type()` contains `Callchain`.
    pub fn callchain(&self) -> &'a [u8] {
        &self.data[self.callchain_range.start as usize..self.callchain_range.end as usize]
    }

    /// Gets the raw data from the event in event-endian byte order.
    ///
    /// Valid if `sample_type()` contains `Raw`.
    pub fn raw_data(&self) -> &'a [u8] {
        &self.data[self.raw_range.start as usize..self.raw_range.end as usize]
    }

    /// Gets the user data from the event in event-endian byte order.
    /// The user data is the raw data after the common fields.
    ///
    /// Valid if `sample_type()` contains `Raw` and format is available.
    pub fn user_data(&self) -> &'a [u8] {
        let format = self.format();
        let raw_len = self.raw_range.end - self.raw_range.start;
        let user_offset = format.common_fields_size();
        if format.is_empty() || user_offset > raw_len {
            return &[];
        } else {
            return &self.data[(self.raw_range.start + user_offset) as usize
                ..(self.raw_range.end - user_offset) as usize];
        }
    }
}
