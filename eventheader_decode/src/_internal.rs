// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#![doc(hidden)]
//! Internal implementation details for eventheader_decode and tests.
//! Contents subject to change without notice.

use core::fmt;
use core::fmt::Write;
use core::iter;
use core::str;

use crate::EventHeaderItemInfo;
use crate::PerfConvertOptions;

const CHAR_MAX_UTF8_SIZE: usize = 4;

/// Returns true if `maybe_valid_ch32` is a valid Unicode scalar value.
/// Returns false for surrogate code points (0xD800..0xE000) and values >= 0x110000.
#[inline]
const fn is_valid_char(maybe_valid_ch32: u32) -> bool {
    // This is an optimized check for !0xD800..0xE000 && < 0x110000.
    return (maybe_valid_ch32 ^ 0xD800).wrapping_sub(0x800) < 0x110000 - 0x800;
}

#[inline]
fn char_from_u32_caller_validated(valid_ch32: u32) -> char {
    debug_assert!(!(0xD800..=0xDFFF).contains(&valid_ch32) && valid_ch32 < 0x110000);
    return unsafe { char::from_u32_unchecked(valid_ch32) };
}

#[inline]
fn str_from_utf8_caller_validated(valid_utf8: &[u8]) -> &str {
    debug_assert!(core::str::from_utf8(valid_utf8).is_ok());
    return unsafe { str::from_utf8_unchecked(valid_utf8) };
}

fn char_or_replacement_from_u32(maybe_valid_ch32: u32) -> char {
    if !is_valid_char(maybe_valid_ch32) {
        return char::REPLACEMENT_CHARACTER;
    } else {
        return char_from_u32_caller_validated(maybe_valid_ch32);
    }
}

fn char_or_replacement_from_u16_pair(valid_high: u16, maybe_valid_low: u16) -> char {
    debug_assert!((0xD800..=0xDBFF).contains(&valid_high));
    if !(0xDC00..=0xDFFF).contains(&maybe_valid_low) {
        return char::REPLACEMENT_CHARACTER;
    } else {
        return char_from_u32_caller_validated(
            ((valid_high as u32 - 0xD800) << 10) | (maybe_valid_low as u32 - 0xDC00) | 0x10000,
        );
    };
}

fn _write_chars_to<OutputWriter, CharsIterator>(
    output: &mut OutputWriter,
    chars: CharsIterator,
) -> fmt::Result
where
    OutputWriter: fmt::Write,
    CharsIterator: iter::Iterator<Item = char>,
{
    // write_str may be a vtable call. Batch up to BUF_SIZE characters per call.
    const BUF_SIZE: usize = 16;
    let mut buf: [u8; BUF_SIZE] = [0; BUF_SIZE];
    let mut buf_pos = 0;

    for ch in chars {
        if buf_pos > BUF_SIZE - CHAR_MAX_UTF8_SIZE {
            output.write_str(str_from_utf8_caller_validated(&buf[..buf_pos]))?;
            buf_pos = 0;
        }

        match ch {
            '\x00'..='\x7F' => {
                buf[buf_pos] = ch as u8;
                buf_pos += 1;
            }
            _ => {
                buf_pos += ch.encode_utf8(&mut buf[buf_pos..]).len();
            }
        }
    }

    if buf_pos > 0 {
        output.write_str(str_from_utf8_caller_validated(&buf[..buf_pos]))?;
    }

    return Ok(());
}

#[inline]
fn to_hex_char_uppercase(nibble: u8) -> u8 {
    return match nibble & 0xF {
        0..=9 => b'0' + nibble,
        10..=15 => b'A' + nibble - 10,
        _ => unreachable!(),
    };
}

fn write_json_escaped_chars_to<OutputWriter, CharsIterator>(
    output: &mut OutputWriter,
    chars: CharsIterator,
) -> fmt::Result
where
    OutputWriter: fmt::Write,
    CharsIterator: iter::Iterator<Item = char>,
{
    const UESCAPE_SIZE: usize = 6;
    let mut uescape: [u8; UESCAPE_SIZE] = [b'\\', b'u', b'0', b'0', 0, 0];

    // write_str may be a vtable call. Batch up to BUF_SIZE characters per call.
    const BUF_SIZE: usize = 16;
    let mut buf: [u8; BUF_SIZE] = [0; BUF_SIZE];
    let mut buf_pos = 0;

    for ch in chars {
        if buf_pos > BUF_SIZE - UESCAPE_SIZE {
            output.write_str(str_from_utf8_caller_validated(&buf[..buf_pos]))?;
            buf_pos = 0;
        }

        match ch {
            '"' => {
                const ESCAPED: &[u8; 2] = b"\\\"";
                buf[buf_pos..buf_pos + ESCAPED.len()].copy_from_slice(ESCAPED);
                buf_pos += ESCAPED.len();
            }
            '\\' => {
                const ESCAPED: &[u8; 2] = b"\\\\";
                buf[buf_pos..buf_pos + ESCAPED.len()].copy_from_slice(ESCAPED);
                buf_pos += ESCAPED.len();
            }
            '\x08' => {
                const ESCAPED: &[u8; 2] = b"\\b";
                buf[buf_pos..buf_pos + ESCAPED.len()].copy_from_slice(ESCAPED);
                buf_pos += ESCAPED.len();
            }
            '\x0C' => {
                const ESCAPED: &[u8; 2] = b"\\f";
                buf[buf_pos..buf_pos + ESCAPED.len()].copy_from_slice(ESCAPED);
                buf_pos += ESCAPED.len();
            }
            '\x0A' => {
                const ESCAPED: &[u8; 2] = b"\\n";
                buf[buf_pos..buf_pos + ESCAPED.len()].copy_from_slice(ESCAPED);
                buf_pos += ESCAPED.len();
            }
            '\x0D' => {
                const ESCAPED: &[u8; 2] = b"\\r";
                buf[buf_pos..buf_pos + ESCAPED.len()].copy_from_slice(ESCAPED);
                buf_pos += ESCAPED.len();
            }
            '\x09' => {
                const ESCAPED: &[u8; 2] = b"\\t";
                buf[buf_pos..buf_pos + ESCAPED.len()].copy_from_slice(ESCAPED);
                buf_pos += ESCAPED.len();
            }
            '\0'..='\x1F' => {
                let ch8 = ch as u8;
                uescape[4] = to_hex_char_uppercase(ch8 / 16);
                uescape[5] = to_hex_char_uppercase(ch8);
                buf[buf_pos..buf_pos + UESCAPE_SIZE].copy_from_slice(&uescape);
                buf_pos += UESCAPE_SIZE;
            }
            '\x20'..='\x7F' => {
                buf[buf_pos] = ch as u8;
                buf_pos += 1;
            }
            _ => {
                buf_pos += ch.encode_utf8(&mut buf[buf_pos..]).len();
            }
        }
    }

    if buf_pos > 0 {
        output.write_str(str_from_utf8_caller_validated(&buf[..buf_pos]))?;
    }

    return Ok(());
}

#[derive(Clone, Copy, Debug)]
pub struct CharsFromLatin1<'dat> {
    bytes: &'dat [u8],
}

impl<'dat> CharsFromLatin1<'dat> {
    pub fn new(bytes: &'dat [u8]) -> CharsFromLatin1<'dat> {
        CharsFromLatin1 { bytes }
    }

    /// Writes the string to a writer, converting Latin1 to UTF-8.
    pub fn write_to<W: fmt::Write>(&self, w: &mut W) -> fmt::Result {
        // Minimize the number of calls to write_str(), assuming ASCII is the common case:
        // Find ASCII sequences and write them in one call.
        // If the string is ASCII, this calls write_str() at most once.
        let len = self.bytes.len();
        let mut last_flush = 0;
        let mut pos = 0;
        while pos < len {
            let b0 = self.bytes[pos];

            if b0 <= 0x7F {
                // 0x00..0x7F: ASCII. Continue.
                pos += 1;
                continue;
            }

            // Flush the ASCII, if any.
            if last_flush < pos {
                w.write_str(str_from_utf8_caller_validated(&self.bytes[last_flush..pos]))?;
            }

            // Treat self.bytes[pos] as Latin1 and move forward.
            w.write_char(b0 as char)?;
            pos += 1;
            last_flush = pos;
        }

        // Write any remaining valid ASCII.
        if last_flush < pos {
            w.write_str(str_from_utf8_caller_validated(&self.bytes[last_flush..pos]))?;
        }

        return Ok(());
    }
}

impl<'dat> iter::FusedIterator for CharsFromLatin1<'dat> {}

impl<'dat> iter::Iterator for CharsFromLatin1<'dat> {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        if self.bytes.is_empty() {
            return None;
        }

        let b0 = self.bytes[0];
        self.bytes = &self.bytes[1..];
        return Some(b0 as char);
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        return (self.bytes.len(), Some(self.bytes.len()));
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CharsFromUtf8WithLatin1Fallback<'dat> {
    bytes: &'dat [u8],
}

impl<'dat> iter::FusedIterator for CharsFromUtf8WithLatin1Fallback<'dat> {}

impl<'dat> fmt::Display for CharsFromUtf8WithLatin1Fallback<'dat> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> fmt::Result {
        return self.write_to(f);
    }
}

impl<'dat> CharsFromUtf8WithLatin1Fallback<'dat> {
    /// Creates a new iterator over probably-UTF-8 bytes.
    pub fn new(bytes: &'dat [u8]) -> CharsFromUtf8WithLatin1Fallback<'dat> {
        return CharsFromUtf8WithLatin1Fallback { bytes };
    }

    /// Writes the string to a writer, converting invalid UTF-8 to Latin1.
    pub fn write_to<W: fmt::Write>(&self, w: &mut W) -> fmt::Result {
        // Minimize the number of calls to write_str():
        // Find valid sequences of UTF-8 and write them in one call.
        // If the string is valid (common case), this calls write_str() at most once.

        let len = self.bytes.len();
        let mut last_flush = 0;
        let mut pos = 0;
        while pos < len {
            // If this is valid UTF-8, update pos and continue to next iteration.
            // If this is not valid UTF-8, fall-through to the Latin1 case.
            let b0 = self.bytes[pos];

            if b0 <= 0x7F {
                // 0x00..0x7F: Valid UTF-8. Continue.
                pos += 1;
                continue;
            } else if b0 <= 0xBF {
                // Invalid lead byte. Fall-through.
            } else if b0 <= 0xDF {
                if len - pos >= 2 {
                    let b1 = self.bytes[pos + 1];
                    if 0x80 == (b1 & 0xC0) {
                        let ch = ((b0 & 0x1F) as u32) << 6 | ((b1 & 0x3F) as u32);
                        if 0x80 <= ch {
                            // Valid 2-byte UTF-8. Continue.
                            pos += 2;
                            continue;
                        }
                    }
                }
            } else if b0 <= 0xEF {
                if len - pos >= 3 {
                    let b1 = self.bytes[pos + 1];
                    let b2 = self.bytes[pos + 2];
                    if 0x80 == (b1 & 0xC0) && 0x80 == (b2 & 0xC0) {
                        let ch = ((b0 & 0x0F) as u32) << 12
                            | ((b1 & 0x3F) as u32) << 6
                            | ((b2 & 0x3F) as u32);
                        if 0x800 <= ch && !(0xD800..=0xDFFF).contains(&ch) {
                            // Valid 3-byte UTF-8. Continue.
                            pos += 3;
                            continue;
                        }
                    }
                }
            } else if b0 <= 0xF4 {
                #[allow(clippy::collapsible_if)]
                // The symmetry seems helpful in understanding this code.
                if len - pos >= 4 {
                    let b1 = self.bytes[pos + 1];
                    let b2 = self.bytes[pos + 2];
                    let b3 = self.bytes[pos + 3];
                    if 0x80 == (b1 & 0xC0) && 0x80 == (b2 & 0xC0) && 0x80 == (b3 & 0xC0) {
                        let ch = ((b0 & 0x07) as u32) << 18
                            | ((b1 & 0x3F) as u32) << 12
                            | ((b2 & 0x3F) as u32) << 6
                            | ((b3 & 0x3F) as u32);
                        if (0x10000..=0x10FFFF).contains(&ch) {
                            // Valid 4-byte UTF-8. Continue.
                            pos += 4;
                            continue;
                        }
                    }
                }
            }

            // self.bytes[pos..] is not valid UTF-8.

            // Flush the valid UTF-8, if any.
            if last_flush < pos {
                w.write_str(str_from_utf8_caller_validated(&self.bytes[last_flush..pos]))?;
            }

            // Treat self.bytes[pos] as Latin1 and move forward.
            w.write_char(b0 as char)?;
            pos += 1;
            last_flush = pos;
        }

        // Write any remaining valid UTF-8.
        if last_flush < pos {
            w.write_str(str_from_utf8_caller_validated(&self.bytes[last_flush..pos]))?;
        }

        return Ok(());
    }
}

impl<'dat> Iterator for CharsFromUtf8WithLatin1Fallback<'dat> {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        let len = self.bytes.len();
        if len == 0 {
            return None;
        }

        let b0 = self.bytes[0];
        if b0 <= 0xBF {
            // 0x00..0x7F: ASCII - pass through.
            // 0x80..0xBF: Invalid lead byte - pass through.
        } else if len > 1 {
            let b1 = self.bytes[1];
            if (b1 & 0xC0) != 0x80 {
                // Invalid trail byte - pass through.
            } else if b0 <= 0xDF {
                let ch = ((b0 & 0x1F) as u32) << 6 | ((b1 & 0x3F) as u32);
                if 0x80 <= ch {
                    // Valid 2-byte UTF-8.
                    self.bytes = &self.bytes[2..];
                    return Some(char_from_u32_caller_validated(ch));
                }
            } else if len > 2 {
                let b2 = self.bytes[2];
                if (b2 & 0xC0) != 0x80 {
                    // Invalid trail byte - pass through.
                } else if b0 <= 0xEF {
                    let ch = ((b0 & 0x0F) as u32) << 12
                        | ((b1 & 0x3F) as u32) << 6
                        | ((b2 & 0x3F) as u32);
                    if 0x800 <= ch && !(0xD800..=0xDFFF).contains(&ch) {
                        // Valid 3-byte UTF-8.
                        self.bytes = &self.bytes[3..];
                        return Some(char_from_u32_caller_validated(ch));
                    }
                } else if len > 3 {
                    let b3 = self.bytes[3];
                    if (b3 & 0xC0) != 0x80 {
                        // Invalid trail byte - pass through.
                    } else if b0 <= 0xF4 {
                        let ch = ((b0 & 0x07) as u32) << 18
                            | ((b1 & 0x3F) as u32) << 12
                            | ((b2 & 0x3F) as u32) << 6
                            | ((b3 & 0x3F) as u32);
                        if (0x10000..=0x10FFFF).contains(&ch) {
                            // Valid 4-byte UTF-8.
                            self.bytes = &self.bytes[4..];
                            return Some(char_from_u32_caller_validated(ch));
                        }
                    }
                }
            }
        }

        // Pass through: treat b0 as Latin1.
        self.bytes = &self.bytes[1..];
        return Some(b0 as char);
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.bytes.len();
        return (len / 4, Some(len));
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CharsFromUtf16<'dat, const BIG_ENDIAN: bool> {
    bytes: &'dat [u8],
}

impl<'dat, const BIG_ENDIAN: bool> CharsFromUtf16<'dat, BIG_ENDIAN> {
    pub fn new(bytes: &'dat [u8]) -> Self {
        Self { bytes }
    }

    pub fn write_to<OutputWriter: fmt::Write>(&self, output: &mut OutputWriter) -> fmt::Result {
        // write_str may be a vtable call. Batch up to BUF_SIZE characters per call.
        const BUF_SIZE: usize = 16;
        let mut buf: [u8; BUF_SIZE] = [0; BUF_SIZE];
        let mut buf_pos = 0;

        let mut pos = 0;
        loop {
            if self.bytes.len() - pos < 2 {
                break;
            }

            if buf_pos > BUF_SIZE - CHAR_MAX_UTF8_SIZE {
                output.write_str(str_from_utf8_caller_validated(&buf[..buf_pos]))?;
                buf_pos = 0;
            }

            let high = Self::from_xe_bytes(self.bytes[pos..pos + 2].try_into().unwrap());
            pos += 2;

            let result;
            if (0x00..=0x7F).contains(&high) {
                buf[buf_pos] = high as u8;
                buf_pos += 1;
                continue;
            } else if !(0xD800..=0xDFFF).contains(&high) {
                // Not a surrogate.
                result = char_from_u32_caller_validated(high as u32);
            } else if high > 0xDBFF || self.bytes.len() - pos < 2 {
                // Invalid or unpaired high surrogate.
                result = char::REPLACEMENT_CHARACTER;
            } else {
                // Valid high surrogate. Maybe valid low surrogate.
                let low = Self::from_xe_bytes(self.bytes[pos..pos + 2].try_into().unwrap());
                pos += 2;
                result = char_or_replacement_from_u16_pair(high, low);
            }

            buf_pos += result.encode_utf8(&mut buf[buf_pos..]).len();
        }

        if buf_pos > 0 {
            output.write_str(str_from_utf8_caller_validated(&buf[..buf_pos]))?;
        }

        return Ok(());
    }

    #[inline]
    const fn from_xe_bytes(bytes: [u8; 2]) -> u16 {
        if BIG_ENDIAN {
            u16::from_be_bytes(bytes)
        } else {
            u16::from_le_bytes(bytes)
        }
    }
}

impl<'dat, const BIG_ENDIAN: bool> iter::FusedIterator for CharsFromUtf16<'dat, BIG_ENDIAN> {}

impl<'dat, const BIG_ENDIAN: bool> iter::Iterator for CharsFromUtf16<'dat, BIG_ENDIAN> {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        if self.bytes.len() < 2 {
            return None;
        }

        let high = Self::from_xe_bytes(self.bytes[..2].try_into().unwrap());
        self.bytes = &self.bytes[2..];

        let result;
        if !(0xD800..=0xDFFF).contains(&high) {
            // Not a surrogate.
            result = char_from_u32_caller_validated(high as u32);
        } else if high > 0xDBFF || self.bytes.len() < 2 {
            // Invalid or unpaired high surrogate.
            result = char::REPLACEMENT_CHARACTER;
        } else {
            // Valid high surrogate. Maybe valid low surrogate.
            let low = Self::from_xe_bytes(self.bytes[..2].try_into().unwrap());
            self.bytes = &self.bytes[2..];
            result = char_or_replacement_from_u16_pair(high, low);
        }

        return Some(result);
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        return (self.bytes.len() / 4, Some(self.bytes.len() / 2));
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CharsFromUtf32<'dat, const BIG_ENDIAN: bool> {
    bytes: &'dat [u8],
}

impl<'dat, const BIG_ENDIAN: bool> CharsFromUtf32<'dat, BIG_ENDIAN> {
    pub fn new(bytes: &'dat [u8]) -> Self {
        Self { bytes }
    }

    pub fn write_to<OutputWriter: fmt::Write>(&self, output: &mut OutputWriter) -> fmt::Result {
        // write_str may be a vtable call. Batch up to BUF_SIZE characters per call.
        const BUF_SIZE: usize = 16;
        let mut buf: [u8; BUF_SIZE] = [0; BUF_SIZE];
        let mut buf_pos = 0;

        let mut pos = 0;

        loop {
            if self.bytes.len() - pos < 4 {
                break;
            }

            if buf_pos > BUF_SIZE - CHAR_MAX_UTF8_SIZE {
                output.write_str(str_from_utf8_caller_validated(&buf[..buf_pos]))?;
                buf_pos = 0;
            }

            let ch32 = Self::from_xe_bytes(self.bytes[pos..pos + 4].try_into().unwrap());
            pos += 4;

            if ch32 <= 0x7F {
                buf[buf_pos] = ch32 as u8;
                buf_pos += 1;
            } else {
                let result = char_or_replacement_from_u32(ch32);
                buf_pos += result.encode_utf8(&mut buf[buf_pos..]).len();
            }
        }

        if buf_pos > 0 {
            output.write_str(str_from_utf8_caller_validated(&buf[..buf_pos]))?;
        }

        return Ok(());
    }

    #[inline]
    const fn from_xe_bytes(bytes: [u8; 4]) -> u32 {
        if BIG_ENDIAN {
            u32::from_be_bytes(bytes)
        } else {
            u32::from_le_bytes(bytes)
        }
    }
}

impl<'dat, const BIG_ENDIAN: bool> iter::FusedIterator for CharsFromUtf32<'dat, BIG_ENDIAN> {}

impl<'dat, const BIG_ENDIAN: bool> iter::Iterator for CharsFromUtf32<'dat, BIG_ENDIAN> {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        if self.bytes.len() < 4 {
            return None;
        }

        let ch32 = Self::from_xe_bytes(self.bytes[..4].try_into().unwrap());
        self.bytes = &self.bytes[4..];

        let result = char_or_replacement_from_u32(ch32);
        return Some(result);
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        return (self.bytes.len() / 4, Some(self.bytes.len() / 4));
    }
}

pub type CharsFromUtf16BE<'dat> = CharsFromUtf16<'dat, true>;
pub type CharsFromUtf16LE<'dat> = CharsFromUtf16<'dat, false>;
pub type CharsFromUtf32BE<'dat> = CharsFromUtf32<'dat, true>;
pub type CharsFromUtf32LE<'dat> = CharsFromUtf32<'dat, false>;

struct JsonEscapeWriter<'out, OutputWriter: fmt::Write> {
    output: &'out mut OutputWriter,
}

impl<'out, OutputWriter: fmt::Write> JsonEscapeWriter<'out, OutputWriter> {
    pub fn new(output: &'out mut OutputWriter) -> JsonEscapeWriter<'out, OutputWriter> {
        JsonEscapeWriter { output }
    }
}

impl<'out, OutputWriter: fmt::Write> fmt::Write for JsonEscapeWriter<'out, OutputWriter> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        return write_json_escaped_chars_to(self.output, value.chars());
    }
}

pub struct TextWriter<'out, OutputWriter: fmt::Write> {
    output: &'out mut OutputWriter,
}

impl<'out, OutputWriter: fmt::Write> TextWriter<'out, OutputWriter> {
    pub fn new(output: &'out mut OutputWriter) -> TextWriter<'out, OutputWriter> {
        TextWriter { output }
    }

    pub fn write_utf8_with_latin1_fallback(&mut self, bytes: &[u8]) -> fmt::Result {
        return CharsFromUtf8WithLatin1Fallback::new(bytes).write_to(self.output);
    }

    pub fn write_latin1(&mut self, bytes: &[u8]) -> fmt::Result {
        return CharsFromLatin1::new(bytes).write_to(self.output);
    }

    pub fn write_utf16be(&mut self, bytes: &[u8]) -> fmt::Result {
        return CharsFromUtf16::<true>::new(bytes).write_to(self.output);
    }

    pub fn write_utf16le(&mut self, bytes: &[u8]) -> fmt::Result {
        return CharsFromUtf16::<false>::new(bytes).write_to(self.output);
    }

    pub fn write_utf32be(&mut self, bytes: &[u8]) -> fmt::Result {
        return CharsFromUtf32::<true>::new(bytes).write_to(self.output);
    }

    pub fn write_utf32le(&mut self, bytes: &[u8]) -> fmt::Result {
        return CharsFromUtf32::<false>::new(bytes).write_to(self.output);
    }
}

pub struct JsonWriter<'out, OutputWriter: fmt::Write> {
    output: &'out mut OutputWriter,
    comma: bool,
    current_space: bool,
    want_space: bool,
    want_field_tag: bool,
}

impl<'out, OutputWriter: fmt::Write> JsonWriter<'out, OutputWriter> {
    pub fn new(
        output: &'out mut OutputWriter,
        options: PerfConvertOptions,
        comma: bool,
    ) -> JsonWriter<'out, OutputWriter> {
        JsonWriter {
            output,
            comma,
            current_space: comma && options.has(PerfConvertOptions::Space),
            want_space: options.has(PerfConvertOptions::Space),
            want_field_tag: options.has(PerfConvertOptions::FieldTag),
        }
    }

    /// True if a comma should be written before the next value.
    pub fn comma(&self) -> bool {
        return self.comma;
    }

    /// For use before a value.
    /// Writes: comma?-newline-indent? i.e. `,\n  `
    pub fn write_newline_before_value(&mut self, indent: usize) -> fmt::Result {
        if self.comma {
            self.output.write_str(",\n")?;
        } else {
            self.output.write_str("\n")?;
        }
        if self.want_space {
            for _ in 0..indent {
                self.output.write_str("  ")?;
            }
        }
        self.comma = false;
        self.current_space = false;
        return Ok(());
    }

    /// Writes: `, "escaped-name":`
    pub fn write_property_name(&mut self, name: &str) -> fmt::Result {
        self.write_raw_comma_space()?;
        self.comma = false;

        self.output.write_str("\"")?;
        write_json_escaped_chars_to(self.output, name.chars())?;
        return self.output.write_str("\":");
    }

    /// Writes: `, "escaped-name;tag=0xTAG":`
    pub fn write_property_name_from_item_info(
        &mut self,
        item_info: &EventHeaderItemInfo,
    ) -> fmt::Result {
        self.write_raw_comma_space()?;
        self.comma = false;

        self.output.write_str("\"")?;

        let mut escape_writer = JsonEscapeWriter::new(self.output);
        item_info.name_chars().write_to(&mut escape_writer)?;

        if self.want_field_tag {
            let tag = item_info.metadata().field_tag();
            if tag != 0 {
                escape_writer.output.write_str(";tag=0x")?;
                escape_writer.write_fmt(format_args!("{:X}", tag))?;
            }
        }

        return self.output.write_str("\":");
    }

    /// Writes: `, "name":`
    pub fn write_property_name_json_safe(&mut self, json_safe_name: &str) -> fmt::Result {
        self.write_raw_comma_space()?;
        self.comma = false;

        self.output.write_str("\"")?;
        self.output.write_str(json_safe_name)?;
        return self.output.write_str("\":");
    }

    /// Writes: `, {`
    pub fn write_object_begin(&mut self) -> fmt::Result {
        self.write_raw_comma_space()?;
        self.comma = false;

        return self.output.write_str("{");
    }

    /// Writes: ` }`
    pub fn write_object_end(&mut self) -> fmt::Result {
        self.comma = true;
        if self.current_space {
            self.output.write_str(" ")?;
        }
        return self.output.write_str("}");
    }

    /// Writes: `, [`
    pub fn write_array_begin(&mut self) -> fmt::Result {
        self.write_raw_comma_space()?;
        self.comma = false;
        return self.output.write_str("[");
    }

    /// Writes: ` ]`
    pub fn write_array_end(&mut self) -> fmt::Result {
        self.comma = true;
        if self.current_space {
            self.output.write_str(" ")?;
        }
        return self.output.write_str("]");
    }

    /// Writes: `, "escaped-value"`
    pub fn write_value_quoted_escaped(&mut self, value: &str) -> fmt::Result {
        self.write_raw_comma_space()?;
        self.comma = true;

        self.output.write_str("\"")?;
        write_json_escaped_chars_to(self.output, value.chars())?;
        return self.output.write_str("\"");
    }

    /// Writes: `, "value"`
    pub fn write_value_quoted_json_safe(&mut self, json_safe_value: &str) -> fmt::Result {
        self.write_raw_comma_space()?;
        self.comma = true;

        self.output.write_str("\"")?;
        self.output.write_str(json_safe_value)?;
        return self.output.write_str("\"");
    }

    /// Writes: `, "escaped-fmt_args"`
    pub fn write_fmt_value_quoted_escaped(&mut self, fmt_args: fmt::Arguments) -> fmt::Result {
        self.write_raw_comma_space()?;
        self.comma = true;

        self.output.write_str("\"")?;
        JsonEscapeWriter::new(&mut self.output).write_fmt(fmt_args)?;
        return self.output.write_str("\"");
    }

    /// Writes: `, "fmt_args"`
    pub fn write_fmt_value_quoted_json_safe(
        &mut self,
        json_safe_fmt_args: fmt::Arguments,
    ) -> fmt::Result {
        self.write_raw_comma_space()?;
        self.comma = true;

        self.output.write_str("\"")?;
        self.output.write_fmt(json_safe_fmt_args)?;
        return self.output.write_str("\"");
    }

    /// Writes: `, value`
    pub fn write_value_unquoted_json_safe(&mut self, json_safe_value: &str) -> fmt::Result {
        self.write_raw_comma_space()?;
        self.comma = true;

        return self.output.write_str(json_safe_value);
    }

    /// Writes: `, fmt_args`
    pub fn write_fmt_value_unquoted_json_safe(
        &mut self,
        json_safe_fmt_args: fmt::Arguments,
    ) -> fmt::Result {
        self.write_raw_comma_space()?;
        self.comma = true;

        return self.output.write_fmt(json_safe_fmt_args);
    }

    /// Writes: `, `, does not update any state.
    fn write_raw_comma_space(&mut self) -> fmt::Result {
        if self.current_space {
            self.current_space = self.want_space;
            if self.comma {
                return self.output.write_str(", ");
            } else {
                return self.output.write_str(" ");
            }
        } else {
            self.current_space = self.want_space;
            if self.comma {
                return self.output.write_str(",");
            } else {
                return Ok(());
            }
        }
    }
}
