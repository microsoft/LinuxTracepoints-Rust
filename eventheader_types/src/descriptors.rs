// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

use crate::enums::ExtensionKind;
use crate::enums::HeaderFlags;
use crate::enums::Level;
use crate::enums::Opcode;

/// Characteristics of an eventheader event: severity level, id, etc.
///
/// Each EventHeader event starts with an instance of the `EventHeader` structure.
/// It contains core information recorded for every event to help with event
/// identification, filtering, and decoding.
///
/// If eventheader.flags has the [`HeaderFlags::Extension`] bit set then the
/// eventheader is followed by one or more [`EventHeaderExtension`] blocks.
/// Otherwise the eventheader is followed by the event payload data.
///
/// If [`EventHeaderExtension::kind`] has the chain flag set then the
/// EventHeaderExtension block is followed immediately (no alignment/padding) by
/// another extension block. Otherwise it is followed immediately (no
/// alignment/padding) by the event payload data.
///
/// If there is a `Metadata` extension then it contains the event name, field names,
/// and field types needed to decode the payload data. Otherwise, the payload
/// decoding system is defined externally, i.e. you will use the provider name to
/// find the appropriate decoding manifest, then use the event's id+version to
/// find the decoding information within the manifest, then use that decoding
/// information to decode the event payload data.
///
/// For a particular event definition (i.e. for a particular event name, or for a
/// particular nonzero event id+version), the information in the eventheader (and
/// in the `Metadata` extension, if present) should be constant. For example, instead
/// of having a single event with a runtime-variable level, you should have a
/// distinct event definition (with distinct event name and/or distinct event id)
/// for each level.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EventHeader {
    /// Indicates whether the event uses 32-bit or 64-bit pointers, whether the
    /// event uses little-endian or big-endian byte order, and whether the
    /// event contains any header extension blocks. When generating events,
    /// this should be set to either Default or DefaultWithExtension.
    pub flags: HeaderFlags,

    /// Set version to 0 unless the event has a manually-assigned stable id.
    /// If the event does have a manually-assigned stable id, start the version
    /// at 0, then increment the version for each breaking change to the event
    /// (e.g. for changes to the field names, types, or semantics).
    pub version: u8,

    /// Set id to 0 unless the event has a manually-assigned stable id.
    pub id: u16,

    /// Provider-defined 16-bit value.
    pub tag: u16,

    /// Special semantics for event: 0=informational, 1=activity-start, 2=activity-stop.
    pub opcode: Opcode,

    /// Event severity level: 1=critical, 2=error, 3=warning, 4=info, 5=verbose.
    /// If unsure, use 5 (verbose).
    pub level: Level,
}

impl EventHeader {
    /// Creates a new header for an informational event.
    ///
    /// level: critical, error, warning, info, verbose; if unsure use verbose.
    ///
    /// has_extension: true if the event has one or more header extension blocks.
    pub const fn new(level: Level, has_extension: bool) -> EventHeader {
        return EventHeader {
            flags: if has_extension {
                HeaderFlags::DefaultWithExtension
            } else {
                HeaderFlags::Default
            },
            version: 0,
            id: 0,
            tag: 0,
            opcode: Opcode::Info,
            level,
        };
    }

    /// Creates a new descriptor from values.
    pub const fn from_parts(
        flags: HeaderFlags,
        version: u8,
        id: u16,
        tag: u16,
        opcode: Opcode,
        level: Level,
    ) -> EventHeader {
        return EventHeader {
            flags,
            version,
            id,
            tag,
            opcode,
            level,
        };
    }
}

/// Characteristics of an eventheader extension block.
///
/// Extension block is an EventHeaderExtension followed by `size` bytes of data.
/// Extension block is tightly-packed (no padding bytes, no alignment).
///
/// If [`EventHeader::flags`] has the Extension bit set then the EventHeader is
/// followed by one or more EventHeaderExtension blocks. Otherwise the EventHeader
/// is followed by the event payload data.
///
/// If [`EventHeaderExtension::kind`] has the chain flag set then the
/// EventHeaderExtension block is followed immediately (no alignment/padding) by
/// another extension block. Otherwise it is followed immediately (no
/// alignment/padding) by the event payload data.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EventHeaderExtension {
    /// Size (in bytes) of the data block following this header.
    pub size: u16,

    /// Type of the data block following this header.
    pub kind: ExtensionKind,
}

impl EventHeaderExtension {
    /// Creates a new header for an extension block. Sets size to 0.
    pub fn new(kind: ExtensionKind) -> Self {
        return Self { size: 0, kind };
    }

    /// Creates a new header from values.
    pub fn from_parts(size: u16, kind: ExtensionKind, chain: bool) -> Self {
        return Self {
            size,
            kind: if chain {
                ExtensionKind::from_int(kind.as_int() | ExtensionKind::ChainFlag)
            } else {
                kind
            },
        };
    }
}
