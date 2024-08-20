// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

//! `fmt::Display` helpers for using  tracepoint_decode types with format
//! macros like [`write!`] and [`format_args!`].

use core::fmt;
use core::fmt::Write;

use eventheader_types::*;

use crate::charconv;
use crate::enumerator;
use crate::filters;
use crate::filters::Filter;
use crate::perf_abi;
use crate::perf_event_data;
use crate::perf_event_desc;
use crate::perf_item;
use crate::perf_session;
use crate::writers;

use crate::PerfConvertOptions;
use crate::PerfMetaOptions;

/// Display implementation that JSON-escapes the provided input string.
/// This escapes control chars, quotes, and backslashes. For example,
/// the string `Hello, "world"!` would be displayed as `Hello, \"world\"!`.
pub struct JsonEscapeDisplay<'str> {
    value: &'str str,
}

impl<'str> JsonEscapeDisplay<'str> {
    /// Creates a new formatter for the specified string.
    pub fn new(value: &'str str) -> Self {
        return Self { value };
    }

    /// Writes the JSON-escaped value to the specified writer.
    pub fn write_to<W: fmt::Write + ?Sized>(&self, writer: &mut W) -> fmt::Result {
        let mut dest = filters::WriteFilter::new(writer);
        return filters::JsonEscapeFilter::new(&mut dest).write_str(self.value);
    }
}

impl<'str> fmt::Display for JsonEscapeDisplay<'str> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> fmt::Result {
        return self.write_to(f);
    }
}

/// Display implementation for string data is expected to be UTF-8. For example,
/// this may be used for the name of an EventHeader event or field.
///
/// Tries to interpret the value as UTF-8, but falls back to Latin1 if the value
/// contains non-UTF-8 sequences. This allows the value to be displayed with
/// best-effort fidelity even if the event is incorrectly-authored or corrupt.
///
/// Instances of this type are returned by methods such as
/// [`crate::EventHeaderEventInfo::name_display`] and
/// [`crate::EventHeaderItemInfo::name_display`].
#[derive(Clone, Copy, Debug)]
pub struct Utf8WithLatin1FallbackDisplay<'dat> {
    utf8_bytes: &'dat [u8],
}

impl<'dat> Utf8WithLatin1FallbackDisplay<'dat> {
    /// Creates a new formatter for the specified string data.
    ///
    /// The `utf8_bytes` value is expected to be UTF-8, but if it is not, the bytes
    /// that are not valid UTF-8 will be interpreted as Latin-1.
    pub fn new(utf8_bytes: &'dat [u8]) -> Self {
        return Self { utf8_bytes };
    }

    /// Writes the value to the specified writer.
    pub fn write_to<W: fmt::Write + ?Sized>(&self, writer: &mut W) -> fmt::Result {
        let mut dest = filters::WriteFilter::new(writer);
        return charconv::write_utf8_with_latin1_fallback_to(self.utf8_bytes, &mut dest);
    }
}

impl<'dat> fmt::Display for Utf8WithLatin1FallbackDisplay<'dat> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> fmt::Result {
        return self.write_to(f);
    }
}

/// Display implementation for the name and tag of an EventHeader field.
///
/// If the field tag is 0, writes just the field name.
/// Otherwise, writes the field name plus a suffix like ";tag=0x1234".
///
/// Tries to interpret the name as UTF-8, but falls back to Latin1 if the name
/// contains non-UTF-8 sequences. This allows the value to be displayed with
/// best-effort fidelity even if the event is incorrectly-authored or corrupt.
///
/// Instances of this type are returned by the
/// [`crate::EventHeaderItemInfo::name_and_tag_display`] method.
#[derive(Clone, Copy, Debug)]
pub struct FieldNameAndTagDisplay<'dat> {
    name_utf8_bytes: &'dat [u8],
    tag: u16,
}

impl<'dat> FieldNameAndTagDisplay<'dat> {
    /// Creates a new formatter for the specified field name and field tag.
    ///
    /// The `name_utf8_bytes` value is expected to be UTF-8, but if it is not, the bytes
    /// that are not valid UTF-8 will be interpreted as Latin-1.
    pub fn new(name_utf8_bytes: &'dat [u8], tag: u16) -> Self {
        return Self {
            name_utf8_bytes,
            tag,
        };
    }

    /// If the field tag is 0, writes just the field name.
    /// Otherwise, writes the field name plus a suffix like ";tag=0x1234".
    pub fn write_to<W: fmt::Write + ?Sized>(&self, writer: &mut W) -> fmt::Result {
        let mut dest = filters::WriteFilter::new(writer);
        charconv::write_utf8_with_latin1_fallback_to(self.name_utf8_bytes, &mut dest)?;
        if self.tag != 0 {
            return write!(dest, ";tag=0x{:X}", self.tag);
        }
        return Ok(());
    }
}

impl<'dat> fmt::Display for FieldNameAndTagDisplay<'dat> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> fmt::Result {
        return self.write_to(f);
    }
}

/// Display implementation for the identity of an EventHeader event, i.e.
/// "ProviderName:EventName".
///
/// Instances of this type are returned by the
/// [`crate::EventHeaderEventInfo::identity_display`] method.
#[derive(Clone, Copy, Debug)]
pub struct EventHeaderIdentityDisplay<'nam, 'dat> {
    provider_name: &'nam str,
    event_name_utf8_bytes: &'dat [u8],
}

impl<'nam, 'dat> EventHeaderIdentityDisplay<'nam, 'dat> {
    /// Creates a new formatter for the specified provider name and event name.
    ///
    /// The `event_name_utf8_bytes` value is expected to be UTF-8, but if it is not,
    /// the bytes that are not valid UTF-8 will be interpreted as Latin-1.
    pub fn new(provider_name: &'nam str, event_name_utf8_bytes: &'dat [u8]) -> Self {
        return Self {
            provider_name,
            event_name_utf8_bytes,
        };
    }

    /// Writes the event identity, i.e. "ProviderName:EventName"
    pub fn write_to<W: fmt::Write + ?Sized>(&self, writer: &mut W) -> fmt::Result {
        let mut dest = filters::WriteFilter::new(writer);
        dest.write_str(self.provider_name)?;
        dest.write_ascii(b':')?;
        return charconv::write_utf8_with_latin1_fallback_to(self.event_name_utf8_bytes, &mut dest);
    }
}

impl<'nam, 'dat> fmt::Display for EventHeaderIdentityDisplay<'nam, 'dat> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> fmt::Result {
        return self.write_to(f);
    }
}

/// Display implementation for the JSON-escaped identity of an EventHeader event,
/// i.e. "ProviderName:EventName".
///
/// Instances of this type are returned by the
/// [`crate::EventHeaderEventInfo::json_identity_display`] method.
#[derive(Clone, Copy, Debug)]
pub struct EventHeaderJsonIdentityDisplay<'nam, 'dat> {
    provider_name: &'nam str,
    event_name_utf8_bytes: &'dat [u8],
}

impl<'nam, 'dat> EventHeaderJsonIdentityDisplay<'nam, 'dat> {
    /// Creates a new formatter for the specified provider name and event name.
    ///
    /// The `event_name_utf8_bytes` value is expected to be UTF-8, but if it is not,
    /// the bytes that are not valid UTF-8 will be interpreted as Latin-1.
    pub fn new(provider_name: &'nam str, event_name_utf8_bytes: &'dat [u8]) -> Self {
        return Self {
            provider_name,
            event_name_utf8_bytes,
        };
    }

    /// Writes the event identity, i.e. "ProviderName:EventName"
    pub fn write_to<W: fmt::Write + ?Sized>(&self, writer: &mut W) -> fmt::Result {
        let mut dest_raw = filters::WriteFilter::new(writer);
        let mut dest = filters::JsonEscapeFilter::new(&mut dest_raw);
        dest.write_str(self.provider_name)?;
        dest.write_ascii(b':')?;
        return charconv::write_utf8_with_latin1_fallback_to(self.event_name_utf8_bytes, &mut dest);
    }
}

impl<'nam, 'dat> fmt::Display for EventHeaderJsonIdentityDisplay<'nam, 'dat> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> fmt::Result {
        return self.write_to(f);
    }
}

/// Text formatter for the value of a [`crate::PerfItemValue`].
/// This formats the value using `value.write_to()`.
pub struct PerfItemValueDisplay<'dat> {
    value: &'dat perf_item::PerfItemValue<'dat>,
    convert_options: PerfConvertOptions,
}

impl<'dat> PerfItemValueDisplay<'dat> {
    /// Creates a new formatter for the specified value.
    pub fn new(value: &'dat perf_item::PerfItemValue<'dat>) -> Self {
        return Self {
            value,
            convert_options: PerfConvertOptions::Default,
        };
    }

    /// Configures the conversion options. The default value is [`PerfConvertOptions::Default`].
    pub fn convert_options(&mut self, value: PerfConvertOptions) -> &mut Self {
        self.convert_options = value;
        return self;
    }

    /// Writes the value to the specified writer.
    pub fn write_to<W: fmt::Write + ?Sized>(&self, writer: &mut W) -> fmt::Result {
        self.value.write_to(writer, self.convert_options)
    }
}

impl<'dat> fmt::Display for PerfItemValueDisplay<'dat> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> fmt::Result {
        self.value.write_to(f, self.convert_options)
    }
}

/// JSON formatter for the value of a [`crate::PerfItemValue`].
/// This formats the value using `value.write_json_to()`.
pub struct PerfItemValueJsonDisplay<'dat> {
    value: &'dat perf_item::PerfItemValue<'dat>,
    convert_options: PerfConvertOptions,
}

impl<'dat> PerfItemValueJsonDisplay<'dat> {
    /// Creates a new formatter for the specified value.
    pub fn new(value: &'dat perf_item::PerfItemValue<'dat>) -> Self {
        return Self {
            value,
            convert_options: PerfConvertOptions::Default,
        };
    }

    /// Configures the conversion options. The default value is [`PerfConvertOptions::Default`].
    pub fn convert_options(&mut self, value: PerfConvertOptions) -> &mut Self {
        self.convert_options = value;
        return self;
    }

    /// Writes the value to the specified writer.
    pub fn write_to<W: fmt::Write + ?Sized>(&self, writer: &mut W) -> fmt::Result {
        self.value.write_json_to(writer, self.convert_options)
    }
}

impl<'dat> fmt::Display for PerfItemValueJsonDisplay<'dat> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> fmt::Result {
        self.value.write_json_to(f, self.convert_options)
    }
}

/// Formatter for the "meta" suffix of an EventHeader event, i.e. `"level": 5, "keyword": 3`.
///
/// Instances of this type are returned by the
/// [`crate::EventHeaderEventInfo::json_meta_display`] method.
#[derive(Debug)]
pub struct EventHeaderJsonMetaDisplay<'inf> {
    eh_event_info: &'inf enumerator::EventHeaderEventInfo<'inf, 'inf>,
    sample_event_info: Option<&'inf perf_event_data::PerfSampleEventInfo<'inf>>,
    add_comma_before_first_item: bool,
    meta_options: PerfMetaOptions,
    convert_options: PerfConvertOptions,
}

impl<'inf> EventHeaderJsonMetaDisplay<'inf> {
    pub(crate) fn new(
        eh_event_info: &'inf enumerator::EventHeaderEventInfo<'inf, 'inf>,
        sample_event_info: Option<&'inf perf_event_data::PerfSampleEventInfo<'inf>>,
    ) -> Self {
        return Self {
            eh_event_info,
            sample_event_info,
            add_comma_before_first_item: false,
            meta_options: PerfMetaOptions::Default,
            convert_options: PerfConvertOptions::Default,
        };
    }

    /// Configures whether a comma will be written before the first item, e.g.
    /// `, "level": 5` (true) instead of `"level": 5` (false). The default value is false.
    ///
    /// Note that if no items are written, no comma is written regardless of this setting.
    pub fn add_comma_before_first_item(&mut self, value: bool) -> &mut Self {
        self.add_comma_before_first_item = value;
        return self;
    }

    /// Configures the items that will be included in the suffix.
    /// The default value is [`PerfMetaOptions::Default`].
    pub fn meta_options(&mut self, value: PerfMetaOptions) -> &mut Self {
        self.meta_options = value;
        return self;
    }

    /// Configures the conversion options. The default value is [`PerfConvertOptions::Default`].
    pub fn convert_options(&mut self, value: PerfConvertOptions) -> &mut Self {
        self.convert_options = value;
        return self;
    }

    /// Writes event metadata as a comma-separated list of 0 or more
    /// JSON name-value pairs, e.g. `"level": 5, "keyword": 3` (including the quotation marks).
    /// Returns true if any items were written, false if nothing was written.
    pub fn write_to<W: fmt::Write + ?Sized>(&self, w: &mut W) -> Result<bool, fmt::Error> {
        let mut any_written = if let Some(sample_event_info) = self.sample_event_info {
            sample_event_info
                .json_meta_display()
                .add_comma_before_first_item(self.add_comma_before_first_item)
                .meta_options(
                    self.meta_options
                        .and_not(PerfMetaOptions::Provider.or(PerfMetaOptions::Event)),
                )
                .convert_options(self.convert_options)
                .write_to(w)?
        } else {
            false
        };

        let mut json = writers::JsonWriter::new(
            w,
            self.convert_options,
            any_written || self.add_comma_before_first_item,
        );

        let tracepoint_name = self.eh_event_info.tracepoint_name();
        let provider_name_end = if self
            .meta_options
            .has_flag(PerfMetaOptions::Provider.or(PerfMetaOptions::Options))
        {
            // Unwrap: Shouldn't be possible to get an EventHeaderEventInfo with an invalid tracepoint name.
            tracepoint_name.rfind('_').unwrap()
        } else {
            0
        };

        if self.meta_options.has_flag(PerfMetaOptions::Provider) {
            any_written = true;
            json.write_property_name_json_safe("provider")?;
            json.write_value_quoted(|w| {
                w.write_str_with_json_escape(&tracepoint_name[..provider_name_end])
            })?;
        }

        if self.meta_options.has_flag(PerfMetaOptions::Event) {
            any_written = true;
            json.write_property_name_json_safe("event")?;
            json.write_value_quoted(|w| {
                w.write_utf8_with_json_escape(self.eh_event_info.name_bytes())
            })?;
        }

        if self.meta_options.has_flag(PerfMetaOptions::Id) && self.eh_event_info.header().id != 0 {
            any_written = true;
            json.write_property_name_json_safe("id")?;
            json.write_value(|w| w.write_display_with_no_filter(self.eh_event_info.header().id))?;
        }

        if self.meta_options.has_flag(PerfMetaOptions::Version)
            && self.eh_event_info.header().version != 0
        {
            any_written = true;
            json.write_property_name_json_safe("version")?;
            json.write_value(|w| {
                w.write_display_with_no_filter(self.eh_event_info.header().version)
            })?;
        }

        if self.meta_options.has_flag(PerfMetaOptions::Level)
            && self.eh_event_info.header().level != Level::Invalid
        {
            any_written = true;
            json.write_property_name_json_safe("level")?;
            json.write_value(|w| {
                w.write_display_with_no_filter(self.eh_event_info.header().level.as_int())
            })?;
        }

        if self.meta_options.has_flag(PerfMetaOptions::Keyword) && self.eh_event_info.keyword() != 0
        {
            any_written = true;
            json.write_property_name_json_safe("keyword")?;
            json.write_value(|w| w.write_json_hex64(self.eh_event_info.keyword()))?;
        }

        if self.meta_options.has_flag(PerfMetaOptions::Opcode)
            && self.eh_event_info.header().opcode != Opcode::Info
        {
            any_written = true;
            json.write_property_name_json_safe("opcode")?;
            json.write_value(|w| {
                w.write_display_with_no_filter(self.eh_event_info.header().opcode.as_int())
            })?;
        }

        if self.meta_options.has_flag(PerfMetaOptions::Tag) && self.eh_event_info.header().tag != 0
        {
            any_written = true;
            json.write_property_name_json_safe("tag")?;
            json.write_value(|w| w.write_json_hex32(self.eh_event_info.header().tag as u32))?;
        }

        if self.meta_options.has_flag(PerfMetaOptions::Activity)
            && self.eh_event_info.activity_id_len() >= 16
        {
            any_written = true;
            json.write_property_name_json_safe("activity")?;
            let start = self.eh_event_info.activity_id_start() as usize;
            json.write_value_quoted(|w| {
                w.write_uuid(
                    &self.eh_event_info.event_data()[start..start + 16]
                        .try_into()
                        .unwrap(),
                )
            })?;
        }

        if self.meta_options.has_flag(PerfMetaOptions::RelatedActivity)
            && self.eh_event_info.activity_id_len() >= 32
        {
            any_written = true;
            json.write_property_name_json_safe("relatedActivity")?;
            let start = self.eh_event_info.activity_id_start() as usize + 16;
            json.write_value_quoted(|w| {
                w.write_uuid(
                    &self.eh_event_info.event_data()[start..start + 16]
                        .try_into()
                        .unwrap(),
                )
            })?;
        }

        if self.meta_options.has_flag(PerfMetaOptions::Options) {
            let name_bytes = tracepoint_name.as_bytes();
            let mut pos = provider_name_end;
            while pos < name_bytes.len() {
                let ch = name_bytes[pos];
                if ch.is_ascii_uppercase() && ch != b'L' && ch != b'K' {
                    any_written = true;
                    json.write_property_name_json_safe("options")?;
                    json.write_value_quoted(|w| {
                        w.write_str_with_no_filter(&tracepoint_name[pos..])
                    })?;
                    break;
                }
                pos += 1;
            }
        }

        if self.meta_options.has_flag(PerfMetaOptions::Flags) {
            any_written = true;
            json.write_property_name_json_safe("flags")?;
            json.write_value(|w| {
                w.write_json_hex32(self.eh_event_info.header().flags.as_int() as u32)
            })?;
        }

        return Ok(any_written);
    }
}

impl<'inf> fmt::Display for EventHeaderJsonMetaDisplay<'inf> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)?;
        return Ok(());
    }
}

/// Formatter for the "meta" suffix of an event, i.e. `"time": "...", "cpu": 3`.
///
/// Instances of this type are returned by the
/// [`crate::PerfNonSampleEventInfo::json_meta_display`] and
/// [`crate::PerfSampleEventInfo::json_meta_display`] methods.
pub struct EventInfoJsonMetaDisplay<'a> {
    session_info: &'a perf_session::PerfSessionInfo,
    event_desc: &'a perf_event_desc::PerfEventDesc,
    time: u64,
    cpu: u32,
    pid: u32,
    tid: u32,
    add_comma_before_first_item: bool,
    meta_options: PerfMetaOptions,
    convert_options: PerfConvertOptions,
}

impl<'a> EventInfoJsonMetaDisplay<'a> {
    pub(crate) const fn new(
        session_info: &'a perf_session::PerfSessionInfo,
        event_desc: &'a perf_event_desc::PerfEventDesc,
        time: u64,
        cpu: u32,
        pid: u32,
        tid: u32,
    ) -> Self {
        return Self {
            session_info,
            event_desc,
            time,
            cpu,
            pid,
            tid,
            add_comma_before_first_item: false,
            meta_options: PerfMetaOptions::Default,
            convert_options: PerfConvertOptions::Default,
        };
    }

    /// Configures whether a comma will be written before the first item, e.g.
    /// `, "cpu": 3` (true) instead of `"cpu": 3` (false). The default value is false.
    ///
    /// Note that if no items are written, no comma is written regardless of this setting.
    pub fn add_comma_before_first_item(&mut self, value: bool) -> &mut Self {
        self.add_comma_before_first_item = value;
        return self;
    }

    /// Configures the items that will be included in the suffix.
    /// The default value is [`PerfMetaOptions::Default`].
    pub fn meta_options(&mut self, value: PerfMetaOptions) -> &mut Self {
        self.meta_options = value;
        return self;
    }

    /// Configures the conversion options. The default value is [`PerfConvertOptions::Default`].
    pub fn convert_options(&mut self, value: PerfConvertOptions) -> &mut Self {
        self.convert_options = value;
        return self;
    }

    /// Writes event metadata as a comma-separated list of 0 or more
    /// JSON name-value pairs, e.g. `"level": 5, "keyword": 3` (including the quotation marks).
    /// Returns true if any items were written, false if nothing was written.
    pub fn write_to<W: fmt::Write + ?Sized>(&self, w: &mut W) -> Result<bool, fmt::Error> {
        let mut json =
            writers::JsonWriter::new(w, self.convert_options, self.add_comma_before_first_item);
        let mut any_written = false;
        let sample_type = self.event_desc.attr().sample_type;

        if sample_type.has_flag(perf_abi::PerfEventAttrSampleType::Time)
            && self.meta_options.has_flag(PerfMetaOptions::Time)
        {
            any_written = true;
            json.write_property_name_json_safe("time")?;
            if self.session_info.clock_offset_known() {
                let time_spec = self.session_info.time_to_time_spec(self.time);
                let dt = writers::date_time::DateTime::new(time_spec.seconds());
                if dt.valid() {
                    json.write_value_quoted(|w| {
                        w.write_fmt_with_no_filter(format_args!(
                            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:07}Z",
                            dt.year(),
                            dt.month_of_year(),
                            dt.day_of_month(),
                            dt.hour(),
                            dt.minute(),
                            dt.second(),
                            time_spec.nanoseconds() / 100,
                        ))
                    })?;
                } else {
                    json.write_value_quoted(|w| {
                        w.write_fmt_with_no_filter(format_args!(
                            "TIME({}.{:09})",
                            time_spec.seconds(),
                            time_spec.nanoseconds()
                        ))
                    })?;
                }
            } else {
                json.write_value(|w| w.write_float64(self.time as f64 / 1000000000.0))?;
            }
        }

        if sample_type.has_flag(perf_abi::PerfEventAttrSampleType::Cpu)
            && self.meta_options.has_flag(PerfMetaOptions::Cpu)
        {
            any_written = true;
            json.write_property_name_json_safe("cpu")?;
            json.write_value(|w| w.write_display_with_no_filter(self.cpu))?;
        }

        if sample_type.has_flag(perf_abi::PerfEventAttrSampleType::Tid) {
            if self.meta_options.has_flag(PerfMetaOptions::Pid) {
                any_written = true;
                json.write_property_name_json_safe("pid")?;
                json.write_value(|w| w.write_display_with_no_filter(self.pid))?;
            }

            if self.meta_options.has_flag(PerfMetaOptions::Tid)
                && (self.pid != self.tid || !self.meta_options.has_flag(PerfMetaOptions::Pid))
            {
                any_written = true;
                json.write_property_name_json_safe("tid")?;
                json.write_value(|w| w.write_display_with_no_filter(self.tid))?;
            }
        }

        if self
            .meta_options
            .has_flag(PerfMetaOptions::Provider.or(PerfMetaOptions::Event))
        {
            let provider;
            let event;
            let desc_name = self.event_desc.name();
            match self.event_desc.format() {
                Some(format) if desc_name.is_empty() => {
                    provider = format.system_name();
                    event = format.name();
                }
                _ => {
                    let mut parts = desc_name.split(':');
                    provider = parts.next().unwrap_or("");
                    event = parts.next().unwrap_or("");
                }
            }

            if self.meta_options.has_flag(PerfMetaOptions::Provider) && !provider.is_empty() {
                any_written = true;
                json.write_property_name_json_safe("provider")?;
                json.write_value_quoted(|w| w.write_str_with_json_escape(provider))?;
            }

            if self.meta_options.has_flag(PerfMetaOptions::Event) && !event.is_empty() {
                any_written = true;
                json.write_property_name_json_safe("event")?;
                json.write_value_quoted(|w| w.write_str_with_json_escape(event))?;
            }
        }

        return Ok(any_written);
    }
}

impl<'a> fmt::Display for EventInfoJsonMetaDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)?;
        return Ok(());
    }
}
