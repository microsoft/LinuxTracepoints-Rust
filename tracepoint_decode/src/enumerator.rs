// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

use core::fmt;
use core::fmt::Write;
use core::mem;
use core::str;

use eventheader_types::*;

use crate::charconv;
use crate::filters;
use crate::filters::Filter;
use crate::writers;
use crate::PerfByteReader;
use crate::PerfConvertOptions;
use crate::PerfItemMetadata;
use crate::PerfItemValue;
use crate::PerfMetaOptions;

#[derive(Clone, Copy, Debug)]
enum SubState {
    Error,
    AfterLastItem,
    BeforeFirstItem,
    ValueMetadata,
    ValueScalar,
    ValueSimpleArrayElement,
    ValueComplexArrayElement,
    ArrayBegin,
    ArrayEnd,
    StructBegin,
    StructEnd,
}

// Returns (val, end_pos).
fn lowercase_hex_to_int(str: &[u8], start_pos: usize) -> (u64, usize) {
    let mut val: u64 = 0;
    let mut pos = start_pos;
    while pos < str.len() {
        let nibble;
        let ch = str[pos];
        if ch.is_ascii_digit() {
            nibble = ch - b'0';
        } else if (b'a'..=b'f').contains(&ch) {
            nibble = ch - b'a' + 10;
        } else {
            break;
        }

        val = (val << 4) + (nibble as u64);
        pos += 1;
    }

    return (val, pos);
}

#[derive(Clone, Copy, Debug)]
struct StackEntry {
    /// event_data[next_offset] starts next field's name.
    pub next_offset: u32,

    /// event_data[name_offset] starts current field's name.
    pub name_offset: u32,

    // event_data[name_offset + name_len + 1] starts current field's type.
    pub name_len: u16,

    pub array_index: u16,

    pub array_count: u16,

    /// Number of next_property() calls before popping stack.
    pub remaining_field_count: u8,

    pub _unused: u8,
}

impl StackEntry {
    pub const ZERO: StackEntry = StackEntry {
        next_offset: 0,
        name_offset: 0,
        name_len: 0,
        array_index: 0,
        array_count: 0,
        remaining_field_count: 0,
        _unused: 0,
    };
}

#[derive(Clone, Copy, Debug)]
struct FieldType {
    pub encoding: FieldEncoding,
    pub format: FieldFormat,
    pub tag: u16,
}

/// Formatter for the name of an EventHeader event or field. Tries to interpret the
/// name as UTF-8, but falls back to Latin1 if the name contains non-UTF-8 sequences.
#[derive(Clone, Copy, Debug)]
pub struct NameDisplay<'dat> {
    name: &'dat [u8],
}

impl<'dat> fmt::Display for NameDisplay<'dat> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> fmt::Result {
        return self.write_to(f);
    }
}

impl<'dat> NameDisplay<'dat> {
    /// Writes the name to the specified writer.
    pub fn write_to<W: fmt::Write + ?Sized>(&self, writer: &mut W) -> fmt::Result {
        let mut dest = filters::WriteFilter::new(writer);
        return charconv::write_utf8_with_latin1_fallback_to(self.name, &mut dest);
    }
}

/// Formatter for the name and tag of an EventHeader field.
/// If the field tag is 0, writes just the field name.
/// Otherwise, writes the field name plus a suffix like ";tag=0x1234".
#[derive(Clone, Copy, Debug)]
pub struct NameAndTagDisplay<'dat> {
    name: &'dat [u8],
    tag: u16,
}

impl<'dat> fmt::Display for NameAndTagDisplay<'dat> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> fmt::Result {
        return self.write_to(f);
    }
}

impl<'dat> NameAndTagDisplay<'dat> {
    /// If the field tag is 0, writes just the field name.
    /// Otherwise, writes the field name plus a suffix like ";tag=0x1234".
    pub fn write_to<W: fmt::Write + ?Sized>(&self, writer: &mut W) -> fmt::Result {
        let mut dest = filters::WriteFilter::new(writer);
        charconv::write_utf8_with_latin1_fallback_to(self.name, &mut dest)?;
        if self.tag != 0 {
            return write!(dest, ";tag=0x{:X}", self.tag);
        }
        return Ok(());
    }
}

/// Formatter for the identity of an EventHeader event, i.e. "ProviderName:EventName".
#[derive(Clone, Copy, Debug)]
pub struct IdentityDisplay<'nam, 'dat> {
    provider_name: &'nam str,
    name: &'dat [u8],
}

impl<'nam, 'dat> fmt::Display for IdentityDisplay<'nam, 'dat> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> fmt::Result {
        return self.write_to(f);
    }
}

impl<'nam, 'dat> IdentityDisplay<'nam, 'dat> {
    /// Writes the event identity, i.e. "ProviderName:EventName"
    pub fn write_to<W: fmt::Write + ?Sized>(&self, writer: &mut W) -> fmt::Result {
        let mut dest = filters::WriteFilter::new(writer);
        dest.write_str(self.provider_name)?;
        dest.write_ascii(b':')?;
        return charconv::write_utf8_with_latin1_fallback_to(self.name, &mut dest);
    }
}

/// Formatter for the "meta" suffix of an EventHeader event, i.e. `"level": 5, "keyword": 3`.
#[derive(Debug)]
pub struct JsonMetaDisplay<'inf> {
    event_info: &'inf EventHeaderEventInfo<'inf, 'inf>,
    add_comma_before_first_item: bool,
    meta_options: PerfMetaOptions,
    convert_options: PerfConvertOptions,
}

impl<'inf> fmt::Display for JsonMetaDisplay<'inf> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)?;
        return Ok(());
    }
}

impl<'inf> JsonMetaDisplay<'inf> {
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
    /// Retruns true if any items were written, false if nothing was written.
    pub fn write_to<W: fmt::Write + ?Sized>(&self, w: &mut W) -> Result<bool, fmt::Error> {
        let mut json =
            writers::JsonWriter::new(w, self.convert_options, self.add_comma_before_first_item);
        let mut any_written = false;

        let tracepoint_name = self.event_info.tracepoint_name;
        let provider_name_end = if self
            .meta_options
            .has(PerfMetaOptions::Provider.or(PerfMetaOptions::Options))
        {
            // Unwrap: Shouldn't be possible to get an EventHeaderEventInfo with an invalid tracepoint name.
            tracepoint_name.rfind('_').unwrap()
        } else {
            0
        };

        if self.meta_options.has(PerfMetaOptions::Provider) {
            any_written = true;
            json.write_property_name_json_safe("provider")?;
            json.write_value_quoted(|w| {
                w.write_str_with_json_escape(&tracepoint_name[..provider_name_end])
            })?;
        }

        if self.meta_options.has(PerfMetaOptions::Event) {
            any_written = true;
            json.write_property_name_json_safe("event")?;
            json.write_value_quoted(|w| {
                w.write_utf8_with_json_escape(self.event_info.name_bytes())
            })?;
        }

        if self.meta_options.has(PerfMetaOptions::Id) && self.event_info.header.id != 0 {
            any_written = true;
            json.write_property_name_json_safe("id")?;
            json.write_value(|w| w.write_display_with_no_filter(self.event_info.header.id))?;
        }

        if self.meta_options.has(PerfMetaOptions::Version) && self.event_info.header.version != 0 {
            any_written = true;
            json.write_property_name_json_safe("version")?;
            json.write_value(|w| w.write_display_with_no_filter(self.event_info.header.version))?;
        }

        if self.meta_options.has(PerfMetaOptions::Level)
            && self.event_info.header.level != Level::Invalid
        {
            any_written = true;
            json.write_property_name_json_safe("level")?;
            json.write_value(|w| {
                w.write_display_with_no_filter(self.event_info.header.level.as_int())
            })?;
        }

        if self.meta_options.has(PerfMetaOptions::Keyword) && self.event_info.keyword != 0 {
            any_written = true;
            json.write_property_name_json_safe("keyword")?;
            json.write_value(|w| w.write_json_hex64(self.event_info.keyword))?;
        }

        if self.meta_options.has(PerfMetaOptions::Opcode)
            && self.event_info.header.opcode != Opcode::Info
        {
            any_written = true;
            json.write_property_name_json_safe("opcode")?;
            json.write_value(|w| {
                w.write_display_with_no_filter(self.event_info.header.opcode.as_int())
            })?;
        }

        if self.meta_options.has(PerfMetaOptions::Tag) && self.event_info.header.tag != 0 {
            any_written = true;
            json.write_property_name_json_safe("tag")?;
            json.write_value(|w| w.write_json_hex32(self.event_info.header.tag as u32))?;
        }

        if self.meta_options.has(PerfMetaOptions::Activity) && self.event_info.activity_id_len >= 16
        {
            any_written = true;
            json.write_property_name_json_safe("activity")?;
            let start = self.event_info.activity_id_start as usize;
            json.write_value_quoted(|w| {
                w.write_uuid(
                    &self.event_info.event_data[start..start + 16]
                        .try_into()
                        .unwrap(),
                )
            })?;
        }

        if self.meta_options.has(PerfMetaOptions::RelatedActivity)
            && self.event_info.activity_id_len >= 32
        {
            any_written = true;
            json.write_property_name_json_safe("relatedActivity")?;
            let start = self.event_info.activity_id_start as usize + 16;
            json.write_value_quoted(|w| {
                w.write_uuid(
                    &self.event_info.event_data[start..start + 16]
                        .try_into()
                        .unwrap(),
                )
            })?;
        }

        if self.meta_options.has(PerfMetaOptions::Options) {
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

        if self.meta_options.has(PerfMetaOptions::Flags) {
            any_written = true;
            json.write_property_name_json_safe("flags")?;
            json.write_value(|w| w.write_json_hex32(self.event_info.header.flags.as_int() as u32))?;
        }

        return Ok(any_written);
    }
}

/// Values for the `last_error()` property of [`EventHeaderEnumerator`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventHeaderEnumeratorError {
    /// No error.
    Success,

    /// Event is smaller than 8 bytes or larger than 2GB,
    /// or tracepoint_name is longer than 255 characters.
    InvalidParameter,

    /// Event does not follow the EventHeader naming/layout rules,
    /// has unrecognized flags, or has unrecognized types.
    NotSupported,

    /// Resource usage limit (`move_next_limit`) reached.
    ImplementationLimit,

    /// Event has an out-of-range value.
    InvalidData,

    /// Event has more than 8 levels of nested structs.
    StackOverflow,
}

impl fmt::Display for EventHeaderEnumeratorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            EventHeaderEnumeratorError::Success => "Success",
            EventHeaderEnumeratorError::InvalidParameter => "InvalidParameter",
            EventHeaderEnumeratorError::NotSupported => "NotSupported",
            EventHeaderEnumeratorError::ImplementationLimit => "ImplementationLimit",
            EventHeaderEnumeratorError::InvalidData => "InvalidData",
            EventHeaderEnumeratorError::StackOverflow => "StackOverflow",
        };
        return f.pad(text);
    }
}

/// Values for the State property of [`EventHeaderEnumerator`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EventHeaderEnumeratorState {
    /// After an error has been returned by `move_next`.
    /// `move_next()` and `item_info()` are invalid operations for this state.
    Error,

    /// Positioned after the last item in the event.
    /// `move_next()` and `item_info()` are invalid operations for this state.
    AfterLastItem,

    // move_next() is an invalid operation for all states above this line.
    // move_next() is a valid operation for all states below this line.
    /// Positioned before the first item in the event.
    /// `item_info()` is an invalid operation for this state.
    BeforeFirstItem,

    // item_info() is an invalid operation for all states above this line.
    // item_info() is a valid operation for all states below this line.
    /// Positioned at an item with data (a field or an array element).
    Value,

    /// Positioned before the first item in an array.
    ArrayBegin,

    /// Positioned after the last item in an array.
    ArrayEnd,

    /// Positioned before the first item in a struct.
    StructBegin,

    /// Positioned after the last item in a struct.
    StructEnd,
}

impl fmt::Display for EventHeaderEnumeratorState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            EventHeaderEnumeratorState::Error => "Error",
            EventHeaderEnumeratorState::AfterLastItem => "AfterLastItem",
            EventHeaderEnumeratorState::BeforeFirstItem => "BeforeFirstItem",
            EventHeaderEnumeratorState::Value => "Value",
            EventHeaderEnumeratorState::ArrayBegin => "ArrayBegin",
            EventHeaderEnumeratorState::ArrayEnd => "ArrayEnd",
            EventHeaderEnumeratorState::StructBegin => "StructBegin",
            EventHeaderEnumeratorState::StructEnd => "StructEnd",
        };
        return f.pad(text);
    }
}

impl EventHeaderEnumeratorState {
    /// Returns true if `move_next()` is a valid operation for this state,
    /// i.e. returns `self >= BeforeFirstItem`. This is false for the
    /// `None`, `Error`, and `AfterLastItem` states.
    pub const fn can_move_next(self) -> bool {
        return self as u8 >= EventHeaderEnumeratorState::BeforeFirstItem as u8;
    }

    /// Returns true if `item_info()` is a valid operation for this state,
    /// i.e. returns `self >= Value`. This is false for the
    /// `None`, `Error`, `AfterLastItem`, and `BeforeFirstItem` states.
    pub const fn can_item_info(self) -> bool {
        return self as u8 >= EventHeaderEnumeratorState::Value as u8;
    }
}

/// Event attributes returned by the `event_info()` method of [`EventHeaderEnumerator`].
#[derive(Clone, Copy, Debug)]
pub struct EventHeaderEventInfo<'nam, 'dat> {
    tracepoint_name: &'nam str,
    event_data: &'dat [u8],
    name_start: u32,
    name_len: u32,
    activity_id_start: u32,
    activity_id_len: u8,
    header: EventHeader,
    keyword: u64,
}

impl<'nam, 'dat> EventHeaderEventInfo<'nam, 'dat> {
    /// Returns the `tracepoint_name` that was passed to
    /// `context.enumerate(tracepoint_name, event_data)`, e.g. "ProviderName_L1K2".
    pub fn tracepoint_name(&self) -> &'nam str {
        return self.tracepoint_name;
    }

    /// Returns the `event_data` that was passed to
    /// `context.enumerate(tracepoint_name, event_data)`.
    pub fn event_data(&self) -> &'dat [u8] {
        return self.event_data;
    }

    /// Returns a formatter for the event's identity, i.e. "ProviderName:EventName".
    pub fn identity_display(&self) -> IdentityDisplay<'nam, 'dat> {
        return IdentityDisplay {
            provider_name: self.provider_name(),
            name: self.name_bytes(),
        };
    }

    /// Returns a formatter for the event's "meta" suffix.
    ///
    /// The returned formatter writes event metadata as a comma-separated list of 0 or more
    /// JSON name-value pairs, e.g. `"level": 5, "keyword": 3` (including the quotation marks).
    ///
    /// The included items default to [`PerfMetaOptions::Default`], but can be customized with
    /// the `meta_options()` property.
    ///
    /// One name-value pair is appended for each metadata item that is both requested
    /// by `meta_options` and has a meaningful value available in the event. For example,
    /// the "id" metadata item is only appended if the event has a non-zero `Id` value,
    /// even if the `meta_options` property includes [`PerfMetaOptions::Id`].
    ///
    /// The following metadata items are supported:
    ///
    /// - `"provider": "MyProviderName"` (off by default)
    /// - `"event": "MyEventName"` (off by default)
    /// - `"id": 123` (omitted if zero)
    /// - `"version": 1` (omitted if zero)
    /// - `"level": 5` (omitted if zero)
    /// - `"keyword": "0x1"` (omitted if zero)
    /// - `"opcode": 1` (omitted if zero)
    /// - `"tag": "0x123"` (omitted if zero)
    /// - `"activity": "12345678-1234-1234-1234-1234567890AB"` (omitted if not present)
    /// - `"relatedActivity": "12345678-1234-1234-1234-1234567890AB"` (omitted if not present)
    /// - `"options": "Gmygroup"` (omitted if not present, off by default)
    /// - `"flags": "0x7"` (omitted if zero, off by default)
    pub fn json_meta_display(&self) -> JsonMetaDisplay {
        return JsonMetaDisplay {
            event_info: self,
            add_comma_before_first_item: false,
            meta_options: PerfMetaOptions::Default,
            convert_options: PerfConvertOptions::Default,
        };
    }

    /// Returns the offset into `event_data` where the event name starts.
    pub fn name_start(&self) -> u32 {
        return self.name_start;
    }

    /// Returns the length of the event name in bytes.
    pub fn name_len(&self) -> u32 {
        return self.name_len;
    }

    /// Returns the event's name as a byte slice. In a well-formed event, this will be valid UTF-8.
    /// To handle cases where the name is not valid UTF-8, use `name_display()` instead.
    pub fn name_bytes(&self) -> &'dat [u8] {
        let start = self.name_start as usize;
        let end = start + self.name_len as usize;
        return &self.event_data[start..end];
    }

    /// Returns a formatter for the the event's name. The formatter tries to interpret
    /// the field name as UTF-8, but falls back to Latin1 for any invalid UTF-8 sequences.
    pub fn name_display(&self) -> NameDisplay<'dat> {
        let start = self.name_start as usize;
        let end = start + self.name_len as usize;
        return NameDisplay {
            name: &self.event_data[start..end],
        };
    }

    /// Returns the offset into `event_data` where the activity ID section starts.
    pub fn activity_id_start(&self) -> u32 {
        return self.activity_id_start;
    }

    /// Returns the length of the activity ID section in bytes. This will be 0
    /// (no activity ID), 16 (activity ID but no related ID), or 32 (activity ID
    /// followed by related ID.
    pub fn activity_id_len(&self) -> u8 {
        return self.activity_id_len;
    }

    /// Returns the event header (contains level, opcode, tag, id, version).
    pub fn header(&self) -> EventHeader {
        return self.header;
    }

    /// Returns the event keyword (category bits, extracted from `tracepoint_name`).
    pub fn keyword(&self) -> u64 {
        return self.keyword;
    }

    /// Returns the provider name (extracted from `tracepoint_name`).
    pub fn provider_name(&self) -> &'nam str {
        let result = if let Some(underscore_pos) = self.tracepoint_name.rfind('_') {
            &self.tracepoint_name[..underscore_pos]
        } else {
            self.tracepoint_name
        };
        return result;
    }

    /// Returns the provider options (extracted from `tracepoint_name`), e.g. "" or "Gmygroup".
    pub fn options(&self) -> &'nam str {
        if let Some(underscore_pos) = self.tracepoint_name.rfind('_') {
            // Skip "L...K..." by looking for the next uppercase letter other than L or K.
            let bytes = self.tracepoint_name.as_bytes();
            let mut pos = underscore_pos + 1;
            loop {
                if pos >= bytes.len() {
                    break;
                }

                let ch = bytes[pos];
                if ch.is_ascii_uppercase() && ch != b'L' && ch != b'K' {
                    return &self.tracepoint_name[pos..];
                }
                pos += 1;
            }
        }

        return "";
    }

    /// Returns the activity ID section as an slice.
    /// - If no activity ID: returns an empty slice.
    /// - If activity ID but no related ID: returns a 16-byte slice.
    /// - If activity ID and related ID: returns a 32-byte slice (activity ID followed by related ID).
    pub fn activity_id_section(&self) -> &'dat [u8] {
        let start = self.activity_id_start as usize;
        let end = start + self.activity_id_len as usize;
        return &self.event_data[start..end];
    }

    /// Returns the activity ID, or None if there is no activity ID.
    pub fn activity_id(&self) -> Option<&'dat [u8; 16]> {
        let result = if self.activity_id_len < 16 {
            None
        } else {
            let start = self.activity_id_start as usize;
            Some(self.event_data[start..start + 16].try_into().unwrap())
        };

        return result;
    }

    /// Returns the related activity ID, or None if there is no related activity ID.
    pub fn related_activity_id(&self) -> Option<&'dat [u8; 16]> {
        let result = if self.activity_id_len < 32 {
            None
        } else {
            let start = self.activity_id_start as usize + 16;
            Some(self.event_data[start..start + 16].try_into().unwrap())
        };
        return result;
    }
}

/// Provides access to the name and value of an EventHeader event item. An item is a
/// field of the event or an element of an array field of the event. This struct is
/// returned by the `item_info()` method of [`EventHeaderEnumerator`].
#[derive(Clone, Copy, Debug)]
pub struct EventHeaderItemInfo<'dat> {
    event_data: &'dat [u8],
    name_start: u32,
    name_len: u32,
    value: PerfItemValue<'dat>,
}

impl<'dat> EventHeaderItemInfo<'dat> {
    fn new(context: &EventHeaderEnumeratorContext, event_data: &'dat [u8]) -> Self {
        debug_assert!(context.state.can_item_info());
        let data_pos = context.data_pos_cooked as usize;
        return Self {
            event_data,
            name_start: context.stack_top.name_offset,
            name_len: context.stack_top.name_len as u32,
            value: PerfItemValue::new(
                &event_data[data_pos..data_pos + context.item_size_cooked as usize],
                context.item_metadata_impl(),
            ),
        };
    }

    /// Returns the `event_data` that was passed to
    /// `context.enumerate(tracepoint_name, event_data)`.
    pub fn event_data(&self) -> &'dat [u8] {
        return self.event_data;
    }

    /// Returns the offset into `event_data` where the field name starts.
    pub fn name_start(&self) -> u32 {
        return self.name_start;
    }

    /// Returns the length of the field name in bytes.
    pub fn name_len(&self) -> u32 {
        return self.name_len;
    }

    /// Returns the field's name as a byte slice. In a well-formed event, this will be valid UTF-8.
    /// To handle cases where the name is not valid UTF-8, use `name_display()` instead.
    pub fn name_bytes(&self) -> &'dat [u8] {
        let start = self.name_start as usize;
        let end = start + self.name_len as usize;
        return &self.event_data[start..end];
    }

    /// Returns a formatter for the the field's name. The formatter tries to interpret
    /// the field name as UTF-8, but falls back to Latin1 for any invalid UTF-8 sequences.
    pub fn name_display(&self) -> NameDisplay<'dat> {
        let start = self.name_start as usize;
        let end = start + self.name_len as usize;
        return NameDisplay {
            name: &self.event_data[start..end],
        };
    }

    /// Returns a formatter for the field's name and tag.
    /// If the field tag is 0, this is the field name.
    /// If the field tag is nonzero, this is the field name plus a suffix like ";tag=0x1234".
    pub fn name_and_tag_display(&self) -> NameAndTagDisplay<'dat> {
        let start = self.name_start as usize;
        let end = start + self.name_len as usize;
        return NameAndTagDisplay {
            name: &self.event_data[start..end],
            tag: self.metadata().field_tag(),
        };
    }

    /// Returns the field value.
    pub fn value(&self) -> &PerfItemValue<'dat> {
        return &self.value;
    }

    /// Returns the field's metadata (e.g. type information).
    pub fn metadata(&self) -> PerfItemMetadata {
        return self.value.metadata();
    }
}

/// Helper for getting information from an EventHeader event, e.g. the event name, event
/// attributes, and event fields (field name, type, and value). Enumerate an event as follows:
/// - Create an [`EventHeaderEnumeratorContext`] context. For optimal performance, reuse the
///   context for many events instead of constructing a new context for each event.
/// - Call `context.enumerate(tracepoint_name, event_data)` to get the enumerator for the event.
///   - `tracepoint_name` is the name of the tracepoint, e.g. "ProviderName_L1K2".
///   - `event_data` is the event's user data, starting with an the `eventheader_flags` header
///      (starts immediately after the event's common fields).
/// - Enumerator starts in the `BeforeFirstItem` state.
/// - Use `event_info()` to get the event's name and attributes.
/// - Call `move_next()` to move through the event items.
///   - Check the enumerator state to determine whether the item is a field value, the start/end
///     of an array, the start/end of a struct, or the end of the event (after last item).
///   - Call `item_info()` to get information about the each item.
/// - Reset the enumerator with `reset()` to restart enumeration of the same event.
#[derive(Debug)]
pub struct EventHeaderEnumerator<'ctx, 'nam, 'dat> {
    context: &'ctx mut EventHeaderEnumeratorContext,
    tracepoint_name: &'nam str,
    event_data: &'dat [u8],
}

impl<'ctx, 'nam, 'dat> EventHeaderEnumerator<'ctx, 'nam, 'dat> {
    /// Returns the current state.
    pub fn state(&self) -> EventHeaderEnumeratorState {
        return self.context.state;
    }

    /// Gets status for the most recent call to move_next.
    pub fn last_error(&self) -> EventHeaderEnumeratorError {
        return self.context.last_error;
    }

    /// Gets the remaining event payload, i.e. the event data that has not yet
    /// been decoded. The data position can change each time `move_next()` is called.
    ///
    /// This can be useful after enumeration has completed to to determine
    /// whether the event contains any trailing data (data not described by the
    /// decoding information). Up to 7 bytes of trailing data is normal (padding
    /// between events), but 8 or more bytes of trailing data might indicate some
    /// kind of encoding problem or data corruption.
    pub fn raw_data_position(&self) -> &'dat [u8] {
        return &self.event_data[self.context.data_pos_raw as usize..];
    }

    /// Gets information that applies to the current event, e.g. the event name,
    /// provider name, options, level, keyword, etc.
    pub fn event_info(&self) -> EventHeaderEventInfo<'nam, 'dat> {
        return EventHeaderEventInfo {
            event_data: self.event_data,
            tracepoint_name: self.tracepoint_name,
            name_start: self.context.meta_start,
            name_len: self.context.event_name_len as u32,
            activity_id_start: self.context.activity_id_start,
            activity_id_len: self.context.activity_id_len,
            header: self.context.header,
            keyword: self.context.keyword,
        };
    }

    /// Gets information about the current item, e.g. the item's name,
    /// the item's type (integer, string, float, etc.), data pointer, data size.
    /// The current item changes each time `move_next()` is called.
    ///
    /// **PRECONDITION (debug_assert):** Can be called when `self.state().can_item_info()`,
    /// i.e. after `move_next()` returns true.
    pub fn item_info(&self) -> EventHeaderItemInfo<'dat> {
        debug_assert!(self.context.state.can_item_info());
        let data_pos = self.context.data_pos_cooked as usize;
        return EventHeaderItemInfo {
            event_data: self.event_data,
            name_start: self.context.stack_top.name_offset,
            name_len: self.context.stack_top.name_len as u32,
            value: PerfItemValue::new(
                &self.event_data[data_pos..data_pos + self.context.item_size_cooked as usize],
                self.item_metadata(),
            ),
        };
    }

    /// Gets metadata (type, endian, tag) information of the current item.
    /// This is a subset of the information returned by item_info().
    /// The current item changes each time `move_next()` is called.
    ///
    /// **PRECONDITION (debug_assert):** Can be called when `self.state().can_item_info()`,
    /// i.e. after `move_next()` returns true.
    pub fn item_metadata(&self) -> PerfItemMetadata {
        return self.context.item_metadata_impl();
    }

    /// Positions the enumerator before the first item.
    /// Resets the `move_next` limit to `MOVE_NEXT_LIMIT_DEFAULT`.
    pub fn reset(&mut self) {
        return self
            .context
            .reset_impl(EventHeaderEnumeratorContext::MOVE_NEXT_LIMIT_DEFAULT);
    }

    /// Positions the enumerator before the first item.
    /// Resets the `move_next` limit to the specified value.
    pub fn reset_with_limit(&mut self, move_next_limit: u32) {
        return self.context.reset_impl(move_next_limit);
    }

    /// Moves the enumerator to the next item in the current event, or to the end
    /// of the event if no more items. Returns true if moved to a valid item,
    /// false if no more items or decoding error.
    ///
    /// **PRECONDITION (debug_assert):** Can be called when `self.state().can_move_next()`.
    ///
    /// - Returns true if moved to a valid item.
    /// - Returns false and sets state to AfterLastItem if no more items.
    /// - Returns false and sets state to Error for decoding error.
    ///
    /// Check `last_error()` for details.
    pub fn move_next(&mut self) -> bool {
        return self.context.move_next_impl(self.event_data);
    }

    /// Moves the enumerator to the next sibling of the current item, or to the end
    /// of the event if no more items. Returns true if moved to a valid item, false
    /// if no more items or decoding error.
    ///
    /// - If the current item is ArrayBegin or StructBegin, this efficiently moves
    ///   enumeration to AFTER the corresponding ArrayEnd or StructEnd.
    /// - Otherwise, this is the same as `move_next()`.
    ///
    /// **PRECONDITION (debug_assert):** Can be called when `self.state().can_move_next()`.
    ///
    /// - Returns true if moved to a valid item.
    /// - Returns false and sets state to AfterLastItem if no more items.
    /// - Returns false and sets state to Error for decoding error.
    ///
    /// Check `last_error()` for details.
    pub fn move_next_sibling(&mut self) -> bool {
        return self.context.move_next_sibling_impl(self.event_data);
    }

    /// Advanced scenarios. This method is for extracting type information from an
    /// event without looking at value information. Moves the enumerator to the next
    /// field declaration (not the next field value). Returns true if moved to a valid
    /// item, false if no more items or decoding error.
    ///
    /// **PRECONDITION (debug_assert):** Can be called when `self.state().can_move_next()`.
    ///
    /// - Returns true if moved to a valid item.
    /// - Returns false and sets state to AfterLastItem if no more items.
    /// - Returns false and sets state to Error for decoding error.
    ///
    /// Note that metadata enumeration gives a flat view of arrays and structures.
    /// There are only Value and ArrayBegin items, no ArrayEnd, StructBegin, StructEnd.
    /// A struct shows up as a value with encoding = Struct.
    /// An array shows up as an ArrayBegin with ArrayFlags != 0, and ElementCount is either zero
    /// (indicating a runtime-variable array length) or nonzero (indicating a compile-time
    /// constant array length). An array of struct is a ArrayBegin with Encoding = Struct and
    /// ArrayFlags != 0. ValueBytes will always be empty. ArrayIndex and TypeSize
    /// will always be zero.
    ///
    /// Note that when enumerating metadata for a structure, the enumeration may end before
    /// the expected number of fields are seen. This is a supported scenario and is not an
    /// error in the event. A large field count just means "this structure contains all the
    /// remaining fields in the event".
    ///
    /// Typically called in a loop until it returns false.
    pub fn move_next_metadata(&mut self) -> bool {
        return self.context.move_next_metadata_impl(self.event_data);
    }

    /// Writes a JSON representation of the current item to the provided `writer`,
    /// e.g. for  state [`EventHeaderEnumeratorState::Value`] this might generate
    /// `"MyField": "My Value"` (including the quotation marks), or for state
    /// [`EventHeaderEnumeratorState::ArrayBegin`] this might generate
    /// `"MyField": [ 1, 2, 3 ]`. Consumes the current item and its descendents as if
    /// by a call to `move_next_sibling`.
    ///
    /// Returns true if a comma would be needed before subsequent JSON output, i.e. if
    /// anything was written OR if `add_comma_before_first_item` was true.
    ///
    /// **PRECONDITION (debug_assert):** Can be called when `self.state().can_move_next()`.
    ///
    /// After calling this method, check `self.state()` to determine whether the
    /// enumeration has reached the end of the event or has encountered an error, i.e.
    /// enumeration should stop if `!self.state().can_move_next()`.
    ///
    /// The output and the amount consumed depends on the initial state of the enumerator.
    ///
    /// - [`EventHeaderEnumeratorState::Value`]
    ///
    ///   Appends the current item as a JSON name-value pair like `"MyField": 123` (omits the
    ///   `"MyField":` name if `convert_options` omits [`PerfConvertOptions::RootName`] or if the
    ///   item is an element of an array). Moves enumeration to the next item.
    ///
    /// - [`EventHeaderEnumeratorState::StructBegin`]
    ///
    ///   Appends the current item as a JSON  name-object pair like
    ///   `"MyStruct": { "StructField1": 123, "StructField2": "Hello" }` (omits the `"MyStruct":`
    ///   name if `convert_options` omits [`PerfConvertOptions::RootName`] or if the item is an
    ///   element of an array). Moves enumeration past the end of the item and its descendents,
    ///   i.e. after the matching [`EventHeaderEnumeratorState::StructEnd`].
    ///
    /// - [`EventHeaderEnumeratorState::ArrayBegin`]
    ///
    ///   Appends the current item as a JSON name-array pair like `"MyArray": [ 1, 2, 3 ]` (omits
    ///   the `"MyArray":` name if `convert_options` omits [`PerfConvertOptions::RootName`]). Moves
    ///   enumeration past the end of the item and its descendents, i.e. after the matching
    ///   [`EventHeaderEnumeratorState::ArrayEnd`].
    ///
    /// - [`EventHeaderEnumeratorState::BeforeFirstItem`]
    ///
    ///   Appends all items in the current event as a comma-separated list of name-value pairs, e.g.
    ///   `"MyField": 123, "MyArray": [ 1, 2, 3 ]`. Moves enumeration to
    ///   [`EventHeaderEnumeratorState::AfterLastItem`].
    ///
    /// - [`EventHeaderEnumeratorState::ArrayEnd`], [`EventHeaderEnumeratorState::StructEnd`]
    ///
    ///   Unspecified behavior.
    pub fn write_item_and_move_next_sibling<W: fmt::Write + ?Sized>(
        &mut self,
        writer: &mut W,
        add_comma_before_first_item: bool,
        convert_options: PerfConvertOptions,
    ) -> Result<bool, fmt::Error> {
        return self.context.write_item_and_move_next_sibling_impl(
            self.event_data,
            writer,
            add_comma_before_first_item,
            convert_options,
        );
    }
}

/// Context for enumerating the fields of an EventHeader event. Enumerate an event as follows:
/// - Create an [`EventHeaderEnumeratorContext`] context. For optimal performance, reuse the
///   context for many events instead of constructing a new context for each event.
/// - Call `context.enumerate(tracepoint_name, event_data)` to get the enumerator for the event.
#[derive(Debug)]
pub struct EventHeaderEnumeratorContext {
    // Set by StartEvent:
    header: EventHeader,
    keyword: u64,
    meta_start: u32, // Relative to event_data.
    meta_end: u32,
    activity_id_start: u32, // Relative to event_data.
    activity_id_len: u8,
    byte_reader: PerfByteReader,
    event_name_len: u16, // Name starts at event_data[meta_start].
    data_start: u32,     // Relative to event_data.

    // Vary during enumeration:
    data_pos_raw: u32,
    move_next_remaining: u32,
    stack_top: StackEntry,
    stack_index: u8, // Number of items currently on stack.
    state: EventHeaderEnumeratorState,
    substate: SubState,
    last_error: EventHeaderEnumeratorError,

    element_size: u8,
    field_type: FieldType,
    data_pos_cooked: u32,
    item_size_raw: u32,
    item_size_cooked: u32,

    stack: [StackEntry; EventHeaderEnumeratorContext::STRUCT_NEST_LIMIT as usize],
}

impl EventHeaderEnumeratorContext {
    const READ_FIELD_ERROR: FieldEncoding = FieldEncoding::Invalid;

    /// Default limit on the number of `move_next()` calls that can be made, currently 4096.
    pub const MOVE_NEXT_LIMIT_DEFAULT: u32 = 4096;

    /// Maximum supported levels of struct nesting, currently 8.
    pub const STRUCT_NEST_LIMIT: u8 = 8;

    /// Creates a new context for enumerating the fields of an EventHeader event.
    pub const fn new() -> Self {
        return Self {
            header: EventHeader {
                flags: HeaderFlags::None,
                version: 0,
                id: 0,
                tag: 0,
                opcode: Opcode::Info,
                level: Level::Invalid,
            },
            keyword: 0,
            meta_start: 0,
            meta_end: 0,
            activity_id_start: 0,
            activity_id_len: 0,
            byte_reader: PerfByteReader::new(false),
            event_name_len: 0,
            data_start: 0,
            data_pos_raw: 0,
            move_next_remaining: 0,
            stack_top: StackEntry::ZERO,
            stack_index: 0,
            state: EventHeaderEnumeratorState::Error,
            substate: SubState::Error,
            last_error: EventHeaderEnumeratorError::Success,
            element_size: 0,
            field_type: FieldType {
                encoding: FieldEncoding::Invalid,
                format: FieldFormat::Default,
                tag: 0,
            },
            data_pos_cooked: 0,
            item_size_raw: 0,
            item_size_cooked: 0,
            stack: [StackEntry::ZERO; 8],
        };
    }

    /// Enumerates the fields of an EventHeader event. Returns an enumerator for the event.
    ///
    /// - `tracepoint_name` is the name of the tracepoint, e.g. "ProviderName_L1K2".
    /// - `event_data` is the event's user data, starting with the `eventheader_flags` field
    ///   (i.e. starting immediately after the event's common fields).
    ///
    /// Returns an enumerator for the event, positioned before the first item, with the
    /// move_next limit set to `MOVE_NEXT_LIMIT_DEFAULT`.
    pub fn enumerate<'ctx, 'nam, 'dat>(
        &'ctx mut self,
        tracepoint_name: &'nam str,
        event_data: &'dat [u8],
    ) -> Result<EventHeaderEnumerator<'ctx, 'nam, 'dat>, EventHeaderEnumeratorError> {
        return self.enumerate_with_limit(
            tracepoint_name,
            event_data,
            Self::MOVE_NEXT_LIMIT_DEFAULT,
        );
    }

    /// Enumerates the fields of an EventHeader event. Returns an enumerator for the event.
    ///
    /// - `tracepoint_name` is the name of the tracepoint, e.g. "ProviderName_L1K2".
    /// - `event_data` is the event's user data, starting with the `eventheader_flags` field
    ///   (i.e. starting immediately after the event's common fields).
    /// - `move_next_limit` is the maximum number of `move_next()` calls that can be made.
    ///   This is a safety feature to prevent excessive CPU usage when processing malformed
    ///   events.
    ///
    /// Returns an enumerator for the event, positioned before the first item, with the
    /// move_next limit set to `move_next_limit`.
    pub fn enumerate_with_limit<'ctx, 'nam, 'dat>(
        &'ctx mut self,
        tracepoint_name: &'nam str,
        event_data: &'dat [u8],
        move_next_limit: u32,
    ) -> Result<EventHeaderEnumerator<'ctx, 'nam, 'dat>, EventHeaderEnumeratorError> {
        const EVENT_HEADER_TRACEPOINT_NAME_MAX: usize = 256;

        const KNOWN_FLAGS: u8 = HeaderFlags::Pointer64.as_int()
            | HeaderFlags::LittleEndian.as_int()
            | HeaderFlags::Extension.as_int();

        let mut event_pos = 0;
        let tp_name_bytes = tracepoint_name.as_bytes();

        if event_data.len() < mem::size_of::<EventHeader>()
            || event_data.len() >= 0x80000000
            || tp_name_bytes.len() >= EVENT_HEADER_TRACEPOINT_NAME_MAX
        {
            // Event has no header or tracepoint_name too long.
            return Err(EventHeaderEnumeratorError::InvalidParameter);
        }

        // Get event header and validate it.

        self.header.flags = HeaderFlags::from_int(event_data[event_pos]);
        self.byte_reader =
            PerfByteReader::new(!self.header.flags.has_flag(HeaderFlags::LittleEndian));
        event_pos += 1;
        self.header.version = event_data[event_pos];
        event_pos += 1;
        self.header.id = self.byte_reader.read_u16(&event_data[event_pos..]);
        event_pos += 2;
        self.header.tag = self.byte_reader.read_u16(&event_data[event_pos..]);
        event_pos += 2;
        self.header.opcode = Opcode::from_int(event_data[event_pos]);
        event_pos += 1;
        self.header.level = Level::from_int(event_data[event_pos]);
        event_pos += 1;

        if self.header.flags.as_int() != (self.header.flags.as_int() & KNOWN_FLAGS) {
            // Not a supported event: unsupported flags.
            return Err(EventHeaderEnumeratorError::NotSupported);
        }

        // Validate Tracepoint name (e.g. "ProviderName_L1K2..."), extract keyword.

        let mut attrib_pos = tp_name_bytes.len();
        loop {
            if attrib_pos == 0 {
                // Not a supported event: no Level in name.
                return Err(EventHeaderEnumeratorError::NotSupported);
            }

            if tp_name_bytes[attrib_pos - 1] == b'_' {
                break;
            }

            attrib_pos -= 1;
        }

        if attrib_pos >= tp_name_bytes.len() || tp_name_bytes[attrib_pos] != b'L' {
            // Not a supported event: no Level in name.
            return Err(EventHeaderEnumeratorError::NotSupported);
        }

        let attrib_level;
        (attrib_level, attrib_pos) = lowercase_hex_to_int(tp_name_bytes, attrib_pos + 1);
        if attrib_level != self.header.level.as_int() as u64 {
            // Not a supported event: name's level != header's level.
            return Err(EventHeaderEnumeratorError::NotSupported);
        }

        if attrib_pos >= tp_name_bytes.len() || b'K' != tp_name_bytes[attrib_pos] {
            // Not a supported event: no Keyword in name.
            return Err(EventHeaderEnumeratorError::NotSupported);
        }

        (self.keyword, attrib_pos) = lowercase_hex_to_int(tp_name_bytes, attrib_pos + 1);

        // Validate but ignore any other attributes.

        while attrib_pos < tp_name_bytes.len() {
            let ch = tp_name_bytes[attrib_pos];
            attrib_pos += 1;
            if !ch.is_ascii_uppercase() {
                // Invalid attribute start character.
                return Err(EventHeaderEnumeratorError::NotSupported);
            }

            // Skip attribute value chars.
            while attrib_pos < tp_name_bytes.len() {
                let ch = tp_name_bytes[attrib_pos];
                if !ch.is_ascii_digit() && !ch.is_ascii_lowercase() {
                    break;
                }
                attrib_pos += 1;
            }
        }

        // Parse header extensions.

        self.meta_start = 0;
        self.meta_end = 0;
        self.activity_id_start = 0;
        self.activity_id_len = 0;

        if self.header.flags.has_flag(HeaderFlags::Extension) {
            loop {
                if event_data.len() - event_pos < mem::size_of::<EventHeaderExtension>() {
                    return Err(EventHeaderEnumeratorError::InvalidData);
                }

                let ext_size = self.byte_reader.read_u16(&event_data[event_pos..]);
                event_pos += 2;
                let ext_kind =
                    ExtensionKind::from_int(self.byte_reader.read_u16(&event_data[event_pos..]));
                event_pos += 2;

                if event_data.len() - event_pos < ext_size as usize {
                    return Err(EventHeaderEnumeratorError::InvalidData);
                }

                match ExtensionKind::from_int(ext_kind.as_int() & ExtensionKind::ValueMask) {
                    ExtensionKind::Invalid => {
                        // Invalid extension type.
                        return Err(EventHeaderEnumeratorError::InvalidData);
                    }
                    ExtensionKind::Metadata => {
                        if self.meta_start != 0 {
                            // Multiple Format extensions.
                            return Err(EventHeaderEnumeratorError::InvalidData);
                        }

                        self.meta_start = event_pos as u32;
                        self.meta_end = self.meta_start + ext_size as u32;
                    }
                    ExtensionKind::ActivityId => {
                        if self.activity_id_start != 0 || (ext_size != 16 && ext_size != 32) {
                            // Multiple ActivityId extensions, or bad activity id size.
                            return Err(EventHeaderEnumeratorError::InvalidData);
                        }

                        self.activity_id_start = event_pos as u32;
                        self.activity_id_len = ext_size as u8;
                    }
                    _ => {} // Ignore other extension types.
                }

                event_pos += ext_size as usize;

                if !ext_kind.has_flag(ExtensionKind::from_int(ExtensionKind::ChainFlag)) {
                    break;
                }
            }
        }

        if self.meta_start == 0 {
            // Not a supported event - no metadata extension.
            return Err(EventHeaderEnumeratorError::NotSupported);
        }

        let mut name_pos = self.meta_start as usize;
        let meta_end = self.meta_end as usize;
        loop {
            if name_pos >= meta_end {
                // Event name not nul-terminated.
                return Err(EventHeaderEnumeratorError::InvalidData);
            }

            if event_data[name_pos] == 0 {
                break;
            }

            name_pos += 1;
        }

        self.event_name_len = (name_pos - self.meta_start as usize) as u16;
        self.data_start = event_pos as u32;
        self.reset_impl(move_next_limit);

        return Ok(EventHeaderEnumerator {
            context: self,
            event_data,
            tracepoint_name,
        });
    }

    fn item_metadata_impl(&self) -> PerfItemMetadata {
        debug_assert!(self.state.can_item_info());
        let is_scalar = self.state < EventHeaderEnumeratorState::ArrayBegin
            || self.state > EventHeaderEnumeratorState::ArrayEnd;
        return PerfItemMetadata::new(
            self.byte_reader,
            self.field_type.encoding,
            self.field_type.format,
            is_scalar,
            self.element_size,
            if is_scalar {
                1
            } else {
                self.stack_top.array_count
            },
            self.field_type.tag,
        );
    }

    fn reset_impl(&mut self, move_next_limit: u32) {
        self.data_pos_raw = self.data_start;
        self.move_next_remaining = move_next_limit;
        self.stack_top.next_offset = self.meta_start + self.event_name_len as u32 + 1;
        self.stack_top.remaining_field_count = 255;
        self.stack_index = 0;
        self.set_state(
            EventHeaderEnumeratorState::BeforeFirstItem,
            SubState::BeforeFirstItem,
        );
        self.last_error = EventHeaderEnumeratorError::Success;
    }

    fn move_next_impl(&mut self, event_data: &[u8]) -> bool {
        debug_assert!(self.state.can_move_next());

        if self.move_next_remaining == 0 {
            return self.set_error_state(EventHeaderEnumeratorError::ImplementationLimit);
        }

        self.move_next_remaining -= 1;

        let moved_to_item;
        match self.substate {
            SubState::BeforeFirstItem => {
                debug_assert!(self.state == EventHeaderEnumeratorState::BeforeFirstItem);
                moved_to_item = self.next_property(event_data);
            }
            SubState::ValueScalar => {
                debug_assert!(self.state == EventHeaderEnumeratorState::Value);
                debug_assert!(self.field_type.encoding.without_flags() != FieldEncoding::Struct);
                debug_assert!(!self.field_type.encoding.is_array());
                debug_assert!(event_data.len() as u32 - self.data_pos_raw >= self.item_size_raw);

                self.data_pos_raw += self.item_size_raw;
                moved_to_item = self.next_property(event_data);
            }
            SubState::ValueSimpleArrayElement => {
                debug_assert!(self.state == EventHeaderEnumeratorState::Value);
                debug_assert!(self.field_type.encoding.without_flags() != FieldEncoding::Struct);
                debug_assert!(self.field_type.encoding.is_array());
                debug_assert!(self.stack_top.array_index < self.stack_top.array_count);
                debug_assert!(self.element_size != 0); // Eligible for fast path.
                debug_assert!(event_data.len() as u32 - self.data_pos_raw >= self.item_size_raw);

                self.data_pos_raw += self.item_size_raw;
                self.stack_top.array_index += 1;

                if self.stack_top.array_count == self.stack_top.array_index {
                    // End of array.
                    self.set_end_state(EventHeaderEnumeratorState::ArrayEnd, SubState::ArrayEnd);
                } else {
                    // Middle of array - get next element.
                    self.start_value_simple(); // Fast path for simple array elements.
                }

                moved_to_item = true;
            }
            SubState::ValueComplexArrayElement => {
                debug_assert!(self.state == EventHeaderEnumeratorState::Value);
                debug_assert!(self.field_type.encoding.without_flags() != FieldEncoding::Struct);
                debug_assert!(self.field_type.encoding.is_array());
                debug_assert!(self.stack_top.array_index < self.stack_top.array_count);
                debug_assert!(self.element_size == 0); // Not eligible for fast path.
                debug_assert!(event_data.len() as u32 - self.data_pos_raw >= self.item_size_raw);

                self.data_pos_raw += self.item_size_raw;
                self.stack_top.array_index += 1;

                if self.stack_top.array_count == self.stack_top.array_index {
                    // End of array.
                    self.set_end_state(EventHeaderEnumeratorState::ArrayEnd, SubState::ArrayEnd);
                    moved_to_item = true;
                } else {
                    // Middle of array - get next element.
                    moved_to_item = self.start_value(event_data); // Normal path for complex array elements.
                }
            }
            SubState::ArrayBegin => {
                debug_assert!(self.state == EventHeaderEnumeratorState::ArrayBegin);
                debug_assert!(self.field_type.encoding.is_array());
                debug_assert!(self.stack_top.array_index == 0);

                if self.stack_top.array_count == 0 {
                    // 0-length array.
                    self.set_end_state(EventHeaderEnumeratorState::ArrayEnd, SubState::ArrayEnd);
                    moved_to_item = true;
                } else if self.element_size != 0 {
                    // First element of simple array.
                    debug_assert!(
                        self.field_type.encoding.without_flags() != FieldEncoding::Struct
                    );
                    self.item_size_cooked = self.element_size as u32;
                    self.item_size_raw = self.element_size as u32;
                    self.set_state(
                        EventHeaderEnumeratorState::Value,
                        SubState::ValueSimpleArrayElement,
                    );
                    self.start_value_simple();
                    moved_to_item = true;
                } else if self.field_type.encoding.without_flags() != FieldEncoding::Struct {
                    // First element of complex array.
                    self.set_state(
                        EventHeaderEnumeratorState::Value,
                        SubState::ValueComplexArrayElement,
                    );
                    moved_to_item = self.start_value(event_data);
                } else {
                    // First element of array of struct.
                    self.start_struct();
                    moved_to_item = true;
                }
            }
            SubState::ArrayEnd => {
                debug_assert!(self.state == EventHeaderEnumeratorState::ArrayEnd);
                debug_assert!(self.field_type.encoding.is_array());
                debug_assert!(self.stack_top.array_count == self.stack_top.array_index);

                // 0-length array of struct means we won't naturally traverse
                // the child struct's metadata. Since self.stackTop.NextOffset
                // won't get updated naturally, we need to update it manually.
                if self.field_type.encoding.without_flags() == FieldEncoding::Struct
                    && self.stack_top.array_count == 0
                    && !self.skip_struct_metadata(event_data)
                {
                    moved_to_item = false;
                } else {
                    moved_to_item = self.next_property(event_data);
                }
            }
            SubState::StructBegin => {
                debug_assert!(self.state == EventHeaderEnumeratorState::StructBegin);
                if self.stack_index >= Self::STRUCT_NEST_LIMIT {
                    moved_to_item = self.set_error_state(EventHeaderEnumeratorError::StackOverflow);
                } else {
                    self.stack[self.stack_index as usize] = self.stack_top;
                    self.stack_index += 1;

                    self.stack_top.remaining_field_count = self.field_type.format.as_int();
                    // Parent's NextOffset is the correct starting point for the struct.
                    moved_to_item = self.next_property(event_data);
                }
            }
            SubState::StructEnd => {
                debug_assert!(self.state == EventHeaderEnumeratorState::StructEnd);
                debug_assert!(self.field_type.encoding.without_flags() == FieldEncoding::Struct);
                debug_assert!(self.item_size_raw == 0);

                self.stack_top.array_index += 1;

                if self.stack_top.array_count != self.stack_top.array_index {
                    debug_assert!(self.field_type.encoding.is_array());
                    debug_assert!(self.stack_top.array_index < self.stack_top.array_count);

                    // Middle of array - get next element.
                    self.start_struct();
                    moved_to_item = true;
                } else if self.field_type.encoding.is_array() {
                    // End of array.
                    self.set_end_state(EventHeaderEnumeratorState::ArrayEnd, SubState::ArrayEnd);
                    moved_to_item = true;
                } else {
                    // End of property - move to next property.
                    moved_to_item = self.next_property(event_data);
                }
            }
            _ => {
                debug_assert!(false, "Unexpected substate.");
                moved_to_item = false;
            }
        }

        return moved_to_item;
    }

    fn move_next_sibling_impl(&mut self, event_data: &[u8]) -> bool {
        debug_assert!(self.state.can_move_next());

        let mut depth = 0; // May reach -1 if we start on ArrayEnd/StructEnd.
        loop {
            match self.state {
                EventHeaderEnumeratorState::ArrayEnd | EventHeaderEnumeratorState::StructEnd => {
                    depth -= 1;
                }
                EventHeaderEnumeratorState::StructBegin => {
                    depth += 1;
                }
                EventHeaderEnumeratorState::ArrayBegin => {
                    if self.element_size == 0 || self.move_next_remaining == 0 {
                        // Use MoveNext for full processing.
                        depth += 1;
                    } else {
                        // Array of simple elements - jump directly to next sibling.
                        debug_assert!(matches!(self.substate, SubState::ArrayBegin));
                        debug_assert!(
                            self.field_type.encoding.without_flags() != FieldEncoding::Struct
                        );
                        debug_assert!(self.field_type.encoding.is_array());
                        debug_assert!(self.stack_top.array_index == 0);
                        self.data_pos_raw +=
                            self.stack_top.array_count as u32 * self.element_size as u32;
                        self.move_next_remaining -= 1;

                        let moved_to_item = self.next_property(event_data);
                        if !moved_to_item || depth <= 0 {
                            return moved_to_item;
                        }

                        continue; // Skip MoveNext().
                    }
                }
                _ => {} // Same as MoveNext.
            }

            let moved_to_item = self.move_next_impl(event_data);
            if !moved_to_item || depth <= 0 {
                return moved_to_item;
            }
        }
    }

    fn move_next_metadata_impl(&mut self, event_data: &[u8]) -> bool {
        if !matches!(self.substate, SubState::ValueMetadata) {
            debug_assert!(self.state == EventHeaderEnumeratorState::BeforeFirstItem);
            debug_assert!(matches!(self.substate, SubState::BeforeFirstItem));
            self.stack_top.array_index = 0;
            self.data_pos_cooked = event_data.len() as u32;
            self.item_size_cooked = 0;
            self.element_size = 0;
            self.set_state(EventHeaderEnumeratorState::Value, SubState::ValueMetadata);
        }

        debug_assert!(
            self.state == EventHeaderEnumeratorState::Value
                || self.state == EventHeaderEnumeratorState::ArrayBegin
        );

        let moved_to_item;
        if self.stack_top.next_offset != self.meta_end {
            self.stack_top.name_offset = self.stack_top.next_offset;

            self.field_type = self.read_field_name_and_type(event_data);
            if self.field_type.encoding == Self::READ_FIELD_ERROR {
                moved_to_item = self.set_error_state(EventHeaderEnumeratorError::InvalidData);
            } else if FieldEncoding::Struct == self.field_type.encoding.without_flags()
                && self.field_type.format == FieldFormat::Default
            {
                // Struct must have at least 1 field (potential for DoS).
                moved_to_item = self.set_error_state(EventHeaderEnumeratorError::InvalidData);
            } else if !self.field_type.encoding.is_array() {
                // Non-array.
                self.stack_top.array_count = 1;
                moved_to_item = true;
                self.set_state(EventHeaderEnumeratorState::Value, SubState::ValueMetadata);
            } else if self.field_type.encoding.is_variable_length_array() {
                // Runtime-variable array length.
                self.stack_top.array_count = 0;
                moved_to_item = true;
                self.set_state(
                    EventHeaderEnumeratorState::ArrayBegin,
                    SubState::ValueMetadata,
                );
            } else if self.field_type.encoding.is_constant_length_array() {
                // Compile-time-constant array length.

                if self.meta_end - self.stack_top.next_offset < 2 {
                    moved_to_item = self.set_error_state(EventHeaderEnumeratorError::InvalidData);
                } else {
                    self.stack_top.array_count = self
                        .byte_reader
                        .read_u16(&event_data[self.stack_top.next_offset as usize..]);
                    self.stack_top.next_offset += 2;

                    if self.stack_top.array_count == 0 {
                        // Constant-length array cannot have length of 0 (potential for DoS).
                        moved_to_item =
                            self.set_error_state(EventHeaderEnumeratorError::InvalidData);
                    } else {
                        moved_to_item = true;
                        self.set_state(
                            EventHeaderEnumeratorState::ArrayBegin,
                            SubState::ValueMetadata,
                        );
                    }
                }
            } else {
                moved_to_item = self.set_error_state(EventHeaderEnumeratorError::NotSupported);
            }
        } else {
            // End of event.

            self.set_end_state(
                EventHeaderEnumeratorState::AfterLastItem,
                SubState::AfterLastItem,
            );
            moved_to_item = false; // No more items.
        }

        return moved_to_item;
    }

    fn write_item_and_move_next_sibling_impl<W: fmt::Write + ?Sized>(
        &mut self,
        event_data: &[u8],
        writer: &mut W,
        add_comma_before_first_item: bool,
        convert_options: PerfConvertOptions,
    ) -> Result<bool, fmt::Error> {
        debug_assert!(self.state.can_move_next());

        let mut want_name = convert_options.has(PerfConvertOptions::RootName);
        let mut json =
            writers::JsonWriter::new(writer, convert_options, add_comma_before_first_item);
        let mut depth = 0i32;

        loop {
            match self.state {
                EventHeaderEnumeratorState::BeforeFirstItem => {
                    depth += 1;
                }

                EventHeaderEnumeratorState::Value => {
                    let item_info = EventHeaderItemInfo::new(self, event_data);
                    if want_name && !item_info.value.metadata().is_element() {
                        json.write_property_name_from_item_info(&item_info)?;
                    }

                    json.write_value(|w| item_info.value.write_json_scalar_to_impl(w))?;
                }

                EventHeaderEnumeratorState::ArrayBegin => {
                    let item_info = EventHeaderItemInfo::new(self, event_data);
                    if want_name {
                        json.write_property_name_from_item_info(&item_info)?;
                    }

                    if item_info.value.metadata().type_size() != 0 {
                        item_info.value.write_json_simple_array_to_impl(&mut json)?;

                        // Use move_next_sibling instead of move_next.
                        let moved_to_item = self.move_next_sibling_impl(event_data);
                        if !moved_to_item || depth <= 0 {
                            break;
                        } else {
                            continue;
                        }
                    }

                    json.write_array_begin()?;
                    depth += 1;
                }

                EventHeaderEnumeratorState::ArrayEnd => {
                    json.write_array_end()?;
                    depth -= 1;
                }

                EventHeaderEnumeratorState::StructBegin => {
                    let item_info = EventHeaderItemInfo::new(self, event_data);

                    if want_name && !item_info.value().metadata().is_element() {
                        json.write_property_name_from_item_info(&item_info)?;
                    }

                    json.write_object_begin()?;
                    depth += 1;
                }

                EventHeaderEnumeratorState::StructEnd => {
                    json.write_object_end()?;
                    depth -= 1;
                }

                _ => {
                    debug_assert!(false, "Enumerator in invalid state.");
                    return Err(fmt::Error);
                }
            }

            want_name = true;

            let moved_to_item = self.move_next_impl(event_data);
            if !moved_to_item || depth <= 0 {
                break;
            }
        }

        return Ok(json.comma());
    }

    fn skip_struct_metadata(&mut self, event_data: &[u8]) -> bool {
        debug_assert!(self.field_type.encoding.without_flags() == FieldEncoding::Struct);

        let ok;
        let mut remaining_field_count = self.field_type.format.as_int();
        loop {
            // It's a bit unusual but completely legal and fully supported to reach
            // end-of-metadata before remainingFieldCount == 0.
            if remaining_field_count == 0 || self.stack_top.next_offset == self.meta_end {
                ok = true;
                break;
            }

            self.stack_top.name_offset = self.stack_top.next_offset;

            // Minimal validation, then skip the field:

            let typ = self.read_field_name_and_type(event_data);
            if typ.encoding == Self::READ_FIELD_ERROR {
                ok = self.set_error_state(EventHeaderEnumeratorError::InvalidData);
                break;
            }

            if FieldEncoding::Struct == typ.encoding.without_flags() {
                remaining_field_count += typ.format.as_int();
            }

            if !typ.encoding.is_constant_length_array() {
                // Scalar or runtime length. We're done with the field.
            } else if !typ.encoding.is_variable_length_array() {
                // CArrayFlag is set, VArrayFlag is unset.
                // Compile-time-constant array length.
                // Skip the array length in metadata.

                if self.meta_end - self.stack_top.next_offset < 2 {
                    ok = self.set_error_state(EventHeaderEnumeratorError::InvalidData);
                    break;
                }

                self.stack_top.next_offset += 2;
            } else {
                // Both CArrayFlag and VArrayFlag are set (reserved encoding).
                ok = self.set_error_state(EventHeaderEnumeratorError::NotSupported);
                break;
            }

            remaining_field_count -= 1;
        }

        return ok;
    }

    fn next_property(&mut self, event_data: &[u8]) -> bool {
        if self.stack_top.remaining_field_count != 0 && self.stack_top.next_offset != self.meta_end
        {
            self.stack_top.remaining_field_count -= 1;
            self.stack_top.array_index = 0;
            self.stack_top.name_offset = self.stack_top.next_offset;

            // Decode a field:

            self.field_type = self.read_field_name_and_type(event_data);
            if self.field_type.encoding == Self::READ_FIELD_ERROR {
                return self.set_error_state(EventHeaderEnumeratorError::InvalidData);
            }

            if !self.field_type.encoding.is_array() {
                // Non-array.

                self.stack_top.array_count = 1;
                if FieldEncoding::Struct != self.field_type.encoding {
                    self.set_state(EventHeaderEnumeratorState::Value, SubState::ValueScalar);
                    return self.start_value(event_data);
                }

                if self.field_type.format == FieldFormat::Default {
                    // Struct must have at least 1 field (potential for DoS).
                    return self.set_error_state(EventHeaderEnumeratorError::InvalidData);
                }

                self.start_struct();
                return true;
            }

            if self.field_type.encoding.is_variable_length_array() {
                // Runtime-variable array length.

                if event_data.len() - (self.data_pos_raw as usize) < 2 {
                    return self.set_error_state(EventHeaderEnumeratorError::InvalidData);
                }

                self.stack_top.array_count = self
                    .byte_reader
                    .read_u16(&event_data[self.data_pos_raw as usize..]);
                self.data_pos_raw += 2;

                return self.start_array(event_data.len() as u32); // StartArray will set Flags.
            }

            if self.field_type.encoding.is_constant_length_array() {
                // Compile-time-constant array length.

                if self.meta_end - self.stack_top.next_offset < 2 {
                    return self.set_error_state(EventHeaderEnumeratorError::InvalidData);
                }

                self.stack_top.array_count = self
                    .byte_reader
                    .read_u16(&event_data[self.stack_top.next_offset as usize..]);
                self.stack_top.next_offset += 2;

                if self.stack_top.array_count == 0 {
                    // Constant-length array cannot have length of 0 (potential for DoS).
                    return self.set_error_state(EventHeaderEnumeratorError::InvalidData);
                }

                return self.start_array(event_data.len() as u32); // StartArray will set Flags.
            }

            return self.set_error_state(EventHeaderEnumeratorError::NotSupported);
        }

        if self.stack_index != 0 {
            // End of struct.
            // It's a bit unusual but completely legal and fully supported to reach
            // end-of-metadata before RemainingFieldCount == 0.

            // Pop child from stack.
            self.stack_index -= 1;
            let child_metadata_offset = self.stack_top.next_offset;
            self.stack_top = self.stack[self.stack_index as usize];

            self.field_type = self.read_field_type(
                event_data,
                self.stack_top.name_offset + self.stack_top.name_len as u32 + 1,
            );
            debug_assert!(FieldEncoding::Struct == self.field_type.encoding.without_flags());
            self.element_size = 0;

            // Unless parent is in the middle of an array, we need to set the
            // "next field" position to the child's metadata position.
            debug_assert!(self.stack_top.array_index < self.stack_top.array_count);
            if self.stack_top.array_index + 1 == self.stack_top.array_count {
                self.stack_top.next_offset = child_metadata_offset;
            }

            self.set_end_state(EventHeaderEnumeratorState::StructEnd, SubState::StructEnd);
            return true;
        }

        // End of event.

        if self.stack_top.next_offset != self.meta_end {
            // Event has metadata for more than MaxTopLevelProperties.
            return self.set_error_state(EventHeaderEnumeratorError::NotSupported);
        }

        self.set_end_state(
            EventHeaderEnumeratorState::AfterLastItem,
            SubState::AfterLastItem,
        );

        return false; // No more items.
    }

    fn read_field_name_and_type(&mut self, event_data: &[u8]) -> FieldType {
        let name_begin = self.stack_top.name_offset;
        debug_assert!(self.meta_end >= name_begin);

        let mut name_end = name_begin;
        while name_end < self.meta_end && event_data[name_end as usize] != 0 {
            name_end += 1;
        }

        let result = if self.meta_end - name_end < 2 {
            // Missing nul termination or missing encoding.
            FieldType {
                encoding: Self::READ_FIELD_ERROR,
                format: FieldFormat::Default,
                tag: 0,
            }
        } else {
            self.stack_top.name_len = (name_end - name_begin) as u16;
            self.read_field_type(event_data, name_end + 1)
        };

        return result;
    }

    fn read_field_type(&mut self, event_data: &[u8], type_offset: u32) -> FieldType {
        let mut pos = type_offset;
        debug_assert!(self.meta_end > pos);

        let mut encoding = FieldEncoding::from_int(event_data[pos as usize]);
        let mut format = FieldFormat::Default;
        let mut tag = 0;
        pos += 1;
        if encoding.has_chain_flag() {
            if self.meta_end == pos {
                // Missing format.
                encoding = Self::READ_FIELD_ERROR;
            } else {
                format = FieldFormat::from_int(event_data[pos as usize]);
                pos += 1;
                if format.has_chain_flag() {
                    if self.meta_end - pos < 2 {
                        // Missing tag.
                        encoding = Self::READ_FIELD_ERROR;
                    } else {
                        tag = self.byte_reader.read_u16(&event_data[pos as usize..]);
                        pos += 2;
                    }
                }
            }
        }

        self.stack_top.next_offset = pos;

        return FieldType {
            encoding: encoding.without_chain_flag(),
            format: format.without_flags(),
            tag,
        };
    }

    /// Returns: moved_to_value
    fn start_array(&mut self, event_data_len: u32) -> bool {
        self.element_size = 0;
        self.item_size_raw = 0;
        self.data_pos_cooked = self.data_pos_raw;
        self.item_size_cooked = 0;
        self.set_state(EventHeaderEnumeratorState::ArrayBegin, SubState::ArrayBegin);

        // Determine the m_elementSize value.
        match self.field_type.encoding.without_flags() {
            FieldEncoding::Struct => return true,

            FieldEncoding::Value8 => {
                self.element_size = 1;
            }

            FieldEncoding::Value16 => {
                self.element_size = 2;
            }

            FieldEncoding::Value32 => {
                self.element_size = 4;
            }

            FieldEncoding::Value64 => {
                self.element_size = 8;
            }

            FieldEncoding::Value128 => {
                self.element_size = 16;
            }

            FieldEncoding::ZStringChar8
            | FieldEncoding::ZStringChar16
            | FieldEncoding::ZStringChar32
            | FieldEncoding::StringLength16Char8
            | FieldEncoding::StringLength16Char16
            | FieldEncoding::StringLength16Char32
            | FieldEncoding::BinaryLength16Char8 => return true,

            FieldEncoding::Invalid => {
                return self.set_error_state(EventHeaderEnumeratorError::InvalidData)
            }

            _ => return self.set_error_state(EventHeaderEnumeratorError::NotSupported),
        }

        // For simple array element types, validate that Count * m_elementSize <= RemainingSize.
        // That way we can skip per-element validation and we can safely expose the array data
        // during ArrayBegin.
        let remaining_len = event_data_len - self.data_pos_raw;
        let array_len = self.stack_top.array_count as u32 * self.element_size as u32;
        if remaining_len < array_len {
            return self.set_error_state(EventHeaderEnumeratorError::InvalidData);
        }

        self.item_size_cooked = array_len;
        self.item_size_raw = array_len;
        return true;
    }

    fn start_struct(&mut self) {
        debug_assert!(self.field_type.encoding.without_flags() == FieldEncoding::Struct);
        self.element_size = 0;
        self.item_size_raw = 0;
        self.data_pos_cooked = self.data_pos_raw;
        self.item_size_cooked = 0;
        self.set_state(
            EventHeaderEnumeratorState::StructBegin,
            SubState::StructBegin,
        );
    }

    fn start_value(&mut self, event_data: &[u8]) -> bool {
        let remaining_len = event_data.len() as u32 - self.data_pos_raw;

        debug_assert!(self.state == EventHeaderEnumeratorState::Value);
        debug_assert!(
            self.field_type.encoding
                == FieldEncoding::from_int(
                    event_data[(self.stack_top.name_offset + self.stack_top.name_len as u32 + 1)
                        as usize]
                )
                .without_chain_flag()
        );
        self.data_pos_cooked = self.data_pos_raw;
        self.element_size = 0;

        match self.field_type.encoding.without_flags() {
            FieldEncoding::Value8 => return self.start_value_fixed_length(event_data, 1),
            FieldEncoding::Value16 => return self.start_value_fixed_length(event_data, 2),
            FieldEncoding::Value32 => return self.start_value_fixed_length(event_data, 4),
            FieldEncoding::Value64 => return self.start_value_fixed_length(event_data, 8),
            FieldEncoding::Value128 => return self.start_value_fixed_length(event_data, 16),

            FieldEncoding::ZStringChar8 => self.start_value_zstring8(event_data),
            FieldEncoding::ZStringChar16 => self.start_value_zstring16(event_data),
            FieldEncoding::ZStringChar32 => self.start_value_zstring32(event_data),
            FieldEncoding::StringLength16Char8 | FieldEncoding::BinaryLength16Char8 => {
                self.start_value_string(event_data, 0)
            }
            FieldEncoding::StringLength16Char16 => self.start_value_string(event_data, 1),
            FieldEncoding::StringLength16Char32 => self.start_value_string(event_data, 2),

            _ => {
                debug_assert!(self.field_type.encoding.without_flags() != FieldEncoding::Struct);
                self.item_size_cooked = 0;
                self.item_size_raw = 0;
                return self.set_error_state(EventHeaderEnumeratorError::InvalidData);
            }
        }

        if remaining_len < self.item_size_raw {
            self.item_size_cooked = 0;
            self.item_size_raw = 0;
            return self.set_error_state(EventHeaderEnumeratorError::InvalidData);
        }

        return true;
    }

    fn start_value_simple(&mut self) {
        debug_assert!(self.stack_top.array_index < self.stack_top.array_count);
        debug_assert!(self.field_type.encoding.is_array());
        debug_assert!(self.field_type.encoding.without_flags() != FieldEncoding::Struct);
        debug_assert!(self.element_size != 0);
        debug_assert!(self.item_size_cooked == self.element_size as u32);
        debug_assert!(self.item_size_raw == self.element_size as u32);
        debug_assert!(self.state == EventHeaderEnumeratorState::Value);
        self.data_pos_cooked = self.data_pos_raw;
    }

    fn start_value_fixed_length(&mut self, event_data: &[u8], size: u8) -> bool {
        self.element_size = size;

        let size32 = size as u32;
        let remaining_len = event_data.len() as u32 - self.data_pos_raw;

        if size32 > remaining_len {
            self.item_size_cooked = 0;
            self.item_size_raw = 0;
            self.set_error_state(EventHeaderEnumeratorError::InvalidData);
            return false;
        }

        self.item_size_cooked = size32;
        self.item_size_raw = size32;
        return true;
    }

    fn start_value_zstring8(&mut self, event_data: &[u8]) {
        type CH = u8;
        const ELEMENT_SIZE: usize = mem::size_of::<CH>();
        let end_pos = event_data.len() - ELEMENT_SIZE + 1;
        let mut pos = self.data_pos_raw as usize;
        while pos < end_pos {
            // Byte order not significant - just need to see if it is all-0-bits.
            if 0 == event_data[pos] {
                self.item_size_cooked = pos as u32 - self.data_pos_raw;
                self.item_size_raw = self.item_size_cooked + ELEMENT_SIZE as u32;
                return;
            }
            pos += ELEMENT_SIZE;
        }

        self.item_size_cooked = event_data.len() as u32 - self.data_pos_raw;
        self.item_size_raw = event_data.len() as u32 - self.data_pos_raw;
    }

    fn start_value_zstring16(&mut self, event_data: &[u8]) {
        type CH = u16;
        const ELEMENT_SIZE: usize = mem::size_of::<CH>();
        let end_pos = event_data.len() - ELEMENT_SIZE + 1;
        let mut pos = self.data_pos_raw as usize;
        while pos < end_pos {
            // Byte order not significant - just need to see if it is all-0-bits.
            if 0 == CH::from_ne_bytes(event_data[pos..pos + ELEMENT_SIZE].try_into().unwrap()) {
                self.item_size_cooked = pos as u32 - self.data_pos_raw;
                self.item_size_raw = self.item_size_cooked + ELEMENT_SIZE as u32;
                return;
            }
            pos += ELEMENT_SIZE;
        }

        self.item_size_cooked = event_data.len() as u32 - self.data_pos_raw;
        self.item_size_raw = event_data.len() as u32 - self.data_pos_raw;
    }

    fn start_value_zstring32(&mut self, event_data: &[u8]) {
        type CH = u32;
        const ELEMENT_SIZE: usize = mem::size_of::<CH>();
        let end_pos = event_data.len() - ELEMENT_SIZE + 1;
        let mut pos = self.data_pos_raw as usize;
        while pos < end_pos {
            // Byte order not significant - just need to see if it is all-0-bits.
            if 0 == CH::from_ne_bytes(event_data[pos..pos + ELEMENT_SIZE].try_into().unwrap()) {
                self.item_size_cooked = pos as u32 - self.data_pos_raw;
                self.item_size_raw = self.item_size_cooked + ELEMENT_SIZE as u32;
                return;
            }
            pos += ELEMENT_SIZE;
        }

        self.item_size_cooked = event_data.len() as u32 - self.data_pos_raw;
        self.item_size_raw = event_data.len() as u32 - self.data_pos_raw;
    }

    fn start_value_string(&mut self, event_data: &[u8], char_size_shift: u8) {
        let remaining = event_data.len() as u32 - self.data_pos_raw;
        if remaining < 2 {
            self.item_size_raw = 2;
        } else {
            self.data_pos_cooked = self.data_pos_raw + 2;

            let cch = self
                .byte_reader
                .read_u16(&event_data[self.data_pos_raw as usize..]);
            self.item_size_cooked = (cch as u32) << char_size_shift;
            self.item_size_raw = self.item_size_cooked + 2;
        }
    }

    fn set_state(&mut self, state: EventHeaderEnumeratorState, substate: SubState) {
        self.state = state;
        self.substate = substate;
    }

    fn set_end_state(&mut self, state: EventHeaderEnumeratorState, substate: SubState) {
        self.data_pos_cooked = self.data_pos_raw;
        self.item_size_raw = 0;
        self.item_size_cooked = 0;
        self.state = state;
        self.substate = substate;
    }

    fn set_error_state(&mut self, error: EventHeaderEnumeratorError) -> bool {
        self.last_error = error;
        self.state = EventHeaderEnumeratorState::Error;
        self.substate = SubState::Error;
        return false;
    }
}

impl Default for EventHeaderEnumeratorContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_to_int() {
        assert_eq!(lowercase_hex_to_int(b"", 0), (0, 0));
        assert_eq!(lowercase_hex_to_int(b" ", 0), (0, 0));
        assert_eq!(lowercase_hex_to_int(b" ", 1), (0, 1));
        assert_eq!(lowercase_hex_to_int(b"0", 0), (0, 1));
        assert_eq!(lowercase_hex_to_int(b"0", 1), (0, 1));
        assert_eq!(lowercase_hex_to_int(b"gfedcba9876543210ABCDEFG", 0), (0, 0));
        assert_eq!(
            lowercase_hex_to_int(b"gfedcba9876543210ABCDEFG", 1),
            (0xfedcba9876543210, 17)
        );
        assert_eq!(
            lowercase_hex_to_int(b"gfedcba9876543210ABCDEFG", 2),
            (0xedcba9876543210, 17)
        );
    }
}
