// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

use crate::PerfByteReader;
use core::time;

const BILLION32: u32 = 1_000_000_000;
const BILLION64: u64 = 1_000_000_000;

/// Semantics equivalent to `struct timespec` from `time.h`.
/// Time = 1970 + seconds + nanoseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PerfTimeSpec {
    seconds: i64,
    nanoseconds: u32,
}

impl PerfTimeSpec {
    /// Returns the Unix epoch, 1970-01-01 00:00:00.
    pub const UNIX_EPOCH: PerfTimeSpec = PerfTimeSpec {
        seconds: 0,
        nanoseconds: 0,
    };

    /// Returns the maximum representable value (year 292,277,026,596).
    pub const MAX: PerfTimeSpec = PerfTimeSpec {
        seconds: i64::MAX,
        nanoseconds: BILLION32 - 1,
    };

    /// Returns the minimum representable value (year -292,277,022,656, or BC 292,277,022,657).
    pub const MIN: PerfTimeSpec = PerfTimeSpec {
        seconds: i64::MIN,
        nanoseconds: 0,
    };

    /// Initializes a new instance of the PerfTimeSpec struct with the specified
    /// seconds and nanoseconds, relative to 1970.
    ///
    /// Normalizes nanoseconds to the range 0..999,999,999, i.e. if the nanoseconds
    /// parameter exceeds 999,999,999 then an appropriate number of seconds (1..4)
    /// will be added to seconds. (Note that this may cause seconds to overflow,
    /// which is not checked.)
    pub const fn new(seconds: i64, nanoseconds: u32) -> PerfTimeSpec {
        let mut this = PerfTimeSpec {
            seconds,
            nanoseconds,
        };

        while this.nanoseconds >= BILLION32 {
            this.seconds += 1; // May overflow.
            this.nanoseconds -= BILLION32;
        }

        return this;
    }

    /// Initializes a new instance of the PerfTimeSpec struct with the specified
    /// duration, which is treated as the amount of time after the Unix epoch (1970).
    /// For example, `from_duration_after_epoch(Duration::from_days(10))`  would
    /// return a PerfTimeSpec representing midnight of 1970-01-11.
    ///
    /// Requires that the duration is less than or equal to i64::MAX seconds.
    pub const fn from_duration_after_epoch(duration: time::Duration) -> PerfTimeSpec {
        let seconds = duration.as_secs();
        assert!(
            seconds <= i64::MAX as u64,
            "Duration too large for PerfTimeSpec"
        );
        return PerfTimeSpec::new(seconds as i64, duration.subsec_nanos());
    }

    /// Returns the number of whole seconds since the Unix epoch (1970-01-01 00:00:00).
    pub fn seconds(&self) -> i64 {
        return self.seconds;
    }

    /// Returns the number of nanoseconds since the last whole second, in the range 0..999,999,999.
    pub fn nanoseconds(&self) -> u32 {
        return self.nanoseconds;
    }

    /// Returns this time as a `time::Duration` after the Unix epoch (1970).
    /// Returns `None` if this time is before the Unix epoch (1970).
    pub fn as_duration_after_epoch(&self) -> Option<time::Duration> {
        if self.seconds < 0 {
            return None;
        } else {
            return Some(time::Duration::new(self.seconds as u64, self.nanoseconds));
        }
    }

    /// Returns a new PerfTimeSpec that is the sum of this + nanoseconds.
    pub fn add_nanoseconds(&self, nanoseconds: u64) -> PerfTimeSpec {
        let sec = (nanoseconds / BILLION64) as i64;
        let nsec = (nanoseconds % BILLION64) as u32;

        // The nsec parameter is in the range (0..+2Billion). The PerfTimeSpec constructor
        // will normalize this.
        return PerfTimeSpec::new(self.seconds + sec, self.nanoseconds + nsec);
    }
}

/// Information about a perf event collection session.
#[derive(Clone, Copy, Debug)]
pub struct PerfSessionInfo {
    clock_offset_seconds: i64,
    clock_offset_nanoseconds: u32,
    clock_id: u32,
    clock_offset_known: bool,
    byte_reader: PerfByteReader,
}

impl PerfSessionInfo {
    /// Constructs a new PerfSessionInfo instance.
    /// Instances of this class are normally created and managed by the session
    /// manager or file reader.
    pub const fn new(byte_reader: PerfByteReader) -> PerfSessionInfo {
        PerfSessionInfo {
            clock_offset_seconds: 0,
            clock_offset_nanoseconds: 0,
            clock_id: 0,
            clock_offset_known: false,
            byte_reader,
        }
    }

    /// Returns true if the the session's event data is formatted in big-endian
    /// byte order. (Use `byte_reader` to do byte-swapping as appropriate.)
    pub const fn source_big_endian(&self) -> bool {
        self.byte_reader.source_big_endian()
    }

    /// Returns a PerfByteReader configured for the byte order of the events
    /// in this session, i.e. `PerfByteReader(this.source_big_endian())`.
    pub const fn byte_reader(&self) -> PerfByteReader {
        self.byte_reader
    }

    /// Returns true if session clock offset is known.
    pub const fn clock_offset_known(&self) -> bool {
        self.clock_offset_known
    }

    /// Returns the `CLOCK_REALTIME` value that corresponds to an event timestamp of 0
    /// for this session. Returns 1970 if the session timestamp offset is unknown.
    pub const fn clock_offset(&self) -> PerfTimeSpec {
        PerfTimeSpec::new(self.clock_offset_seconds, self.clock_offset_nanoseconds)
    }

    /// Returns the clockid of the session timestamp, e.g. `CLOCK_MONOTONIC`.
    /// Returns `u32::MAX` if the session timestamp clockid is unknown.
    pub const fn clock_id(&self) -> u32 {
        self.clock_id
    }

    /// From `HEADER_CLOCKID`. If unknown, use `set_clock_id(u32::MAX)`.
    pub fn set_clock_id(&mut self, clock_id: u32) {
        self.clock_id = clock_id;
    }

    /// From HEADER_CLOCK_DATA. If unknown, use SetClockData(u32::MAX, 0, 0).
    pub fn set_clock_data(&mut self, clock_id: u32, wall_clock_ns: u64, clock_id_time_ns: u64) {
        if clock_id == u32::MAX {
            // Offset is unspecified.

            self.clock_offset_seconds = 0;
            self.clock_offset_nanoseconds = 0;
            self.clock_id = clock_id;
            self.clock_offset_known = false;
        } else if clock_id_time_ns <= wall_clock_ns {
            // Offset is positive.

            // wallClockNS = clockidTimeNS + offsetNS
            // offsetNS = wallClockNS - clockidTimeNS
            let offset_ns = wall_clock_ns - clock_id_time_ns;

            // offsetNS = sec * Billion + nsec

            // sec = offsetNS / Billion
            self.clock_offset_seconds = (offset_ns / BILLION64) as i64;

            // nsec = offsetNS % Billion
            self.clock_offset_nanoseconds = (offset_ns % BILLION64) as u32;

            self.clock_id = clock_id;
            self.clock_offset_known = true;
        } else {
            // Offset is negative.

            // wallClockNS = clockidTimeNS + offsetNS
            // offsetNS = wallClockNS - clockidTimeNS
            // -negOffsetNS = wallClockNS - clockidTimeNS
            // negOffsetNS = clockidTimeNS - wallClockNS
            let neg_offset_ns = clock_id_time_ns - wall_clock_ns;

            // negOffsetNS = (negOffsetNS / Billion) * Billion + (negOffsetNS % Billion)
            // negOffsetNS = (negOffsetNS / Billion) * Billion + (negOffsetNS % Billion) - Billion + Billion
            // negOffsetNS = (negOffsetNS / Billion + 1) * Billion + (negOffsetNS % Billion) - Billion

            // negOffsetNS = negSec * Billion + negNsec
            // negSec = negOffsetNS / Billion + 1
            // negNsec = (negOffsetNS % Billion) - Billion

            // sec = -(negOffsetNS / Billion + 1)
            self.clock_offset_seconds = -((neg_offset_ns / BILLION64) as i64) - 1;

            // nsec = -((negOffsetNS % Billion) - Billion)
            self.clock_offset_nanoseconds = BILLION32 - (neg_offset_ns % BILLION64) as u32;

            // Fix up case where nsec is too large.
            if self.clock_offset_nanoseconds == BILLION32 {
                self.clock_offset_seconds += 1;
                self.clock_offset_nanoseconds -= BILLION32;
            }

            self.clock_id = clock_id;
            self.clock_offset_known = true;
        }
    }

    /// Gets offset values `(wall_clock_ns, clockid_time_ns)` suitable for use in
    /// `HEADER_CLOCK_DATA`.
    ///
    /// Note: The returned NS values may be normalized relative to the values provided
    /// to SetClockData, but the difference between them will be the same as the
    /// difference between the values provided to SetClockData.
    pub const fn get_clock_data(&self) -> (u64, u64) {
        if self.clock_offset_seconds >= 0 {
            return (
                self.clock_offset_seconds as u64 * BILLION64 + self.clock_offset_nanoseconds as u64,
                0,
            );
        } else {
            return (
                0,
                (-self.clock_offset_seconds) as u64 * BILLION64
                    - self.clock_offset_nanoseconds as u64,
            );
        }
    }

    /// Converts time from session timestamp to real-time (time since 1970):
    /// `time_to_time_spec = clock_offset() + time`.
    ///
    /// If session clock offset is unknown, assumes 1970.
    pub const fn time_to_time_spec(&self, time: u64) -> PerfTimeSpec {
        let mut sec = (time / BILLION64) as i64;
        let mut nsec = (time % BILLION64) as u32;
        sec += self.clock_offset_seconds;
        nsec += self.clock_offset_nanoseconds;
        if nsec >= BILLION32 {
            sec += 1;
            nsec -= BILLION32;
        }
        return PerfTimeSpec::new(sec, nsec);
    }
}
