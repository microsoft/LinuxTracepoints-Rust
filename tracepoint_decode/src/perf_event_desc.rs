// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

extern crate alloc;

use alloc::boxed;
use alloc::string;
use alloc::sync;

use crate::*;

/// Information about the event (shared by all events with the same Id).
#[derive(Clone, Debug)]
pub struct PerfEventDesc {
    attr: PerfEventAttr,
    name: string::String,
    format: Option<sync::Arc<PerfEventFormat>>,
    ids: boxed::Box<[u64]>,
}

impl PerfEventDesc {
    /// Creates a new PerfEventDesc.
    ///
    /// `PerfEventDesc` objects are usually created by the session or file reader.
    ///
    /// - `attr`: Event's `perf_event_attr`, or an attr with size = 0 if event's info is not available.
    /// - `name`: Event's name, e.g. "sched:sched_switch", typically from a `PERF_HEADER_EVENT_DESC` header.
    ///   If not available and format is provided, name will be constructed from the format.
    /// - `format`: Event's format, or `None` if no format is available. (Non-sample events generally do not
    ///   have a format. Sample events should have a format.)
    /// - `ids`: The sample_ids that share this descriptor.
    pub fn new(
        attr: PerfEventAttr,
        name: string::String,
        format: Option<&sync::Arc<PerfEventFormat>>,
        ids: boxed::Box<[u64]>,
    ) -> PerfEventDesc {
        let mut this = PerfEventDesc {
            attr,
            name,
            format: format.cloned(),
            ids,
        };

        // If name is empty and format is present, construct name from format.
        this.update_name();
        return this;
    }

    /// Event's perf_event_attr, or an attr with size = 0 if event's info
    /// is not available.
    pub const fn attr(&self) -> &PerfEventAttr {
        &self.attr
    }

    /// Gets the event's name, e.g. "sched:sched_switch".
    /// - If name is available from `PERF_HEADER_EVENT_DESC`, return it.
    /// - Otherwise, if name is available from format, return it.
    /// - Otherwise, return empty string.
    pub fn name(&self) -> &str {
        return &self.name;
    }

    /// Event's format, or None if no format is available.
    pub fn format(&self) -> Option<&PerfEventFormat> {
        return self.format.as_deref();
    }

    /// Event's format, or `None` if no format is available.
    pub fn format_arc(&self) -> Option<&sync::Arc<PerfEventFormat>> {
        return self.format.as_ref();
    }

    /// The sample_ids that share this descriptor.
    pub const fn ids(&self) -> &[u64] {
        &self.ids
    }

    /// Advanced: updates the descriptor once the format becomes available.
    pub fn set_format(&mut self, format: &sync::Arc<PerfEventFormat>) {
        self.format = Some(format.clone());
        self.update_name();
    }

    fn update_name(&mut self) {
        if self.name.is_empty() {
            if let Some(format) = &self.format {
                self.name.push_str(format.system_name());
                self.name.push(':');
                self.name.push_str(format.name());
            }
        }
    }
}

impl Default for PerfEventDesc {
    fn default() -> PerfEventDesc {
        PerfEventDesc {
            attr: PerfEventAttr::default(),
            name: string::String::new(),
            format: None,
            ids: boxed::Box::new([]),
        }
    }
}
