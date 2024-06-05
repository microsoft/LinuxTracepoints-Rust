// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

use core::fmt;
use core::str;

#[inline]
fn str_from_validated_utf8(valid_utf8: &[u8]) -> &str {
    debug_assert!(core::str::from_utf8(valid_utf8).is_ok());
    return unsafe { str::from_utf8_unchecked(valid_utf8) };
}

// **** Filter

pub trait Filter: fmt::Write {
    fn write_ascii(&mut self, value: u8) -> fmt::Result;
    fn write_non_ascii(&mut self, value: char) -> fmt::Result;
}

// **** WriteFilter

/// Filters into a `fmt::Write`.
/// TODO: Buffering/batching.
pub struct WriteFilter<'wri, W: fmt::Write> {
    writer: &'wri mut W,
}

impl<'wri, W: fmt::Write> WriteFilter<'wri, W> {
    pub fn new(writer: &'wri mut W) -> Self {
        return Self { writer };
    }
}

impl<'wri, W: fmt::Write> Filter for WriteFilter<'wri, W> {
    fn write_ascii(&mut self, value: u8) -> fmt::Result {
        debug_assert!(value < 0x80);
        return self
            .writer
            .write_str(str_from_validated_utf8(core::slice::from_ref(&value)));
    }

    fn write_non_ascii(&mut self, value: char) -> fmt::Result {
        return self.writer.write_char(value);
    }
}

impl<'wri, W: fmt::Write> fmt::Write for WriteFilter<'wri, W> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        return self.writer.write_str(value);
    }
}

// **** ControlCharsSpaceFilter

/// Replaces control char with space.
pub struct ControlCharsSpaceFilter<'low, F: Filter> {
    lower: &'low mut F,
}

impl<'low, F: Filter> ControlCharsSpaceFilter<'low, F> {
    pub fn new(lower: &'low mut F) -> Self {
        return Self { lower };
    }
}

impl<'low, F: Filter> Filter for ControlCharsSpaceFilter<'low, F> {
    fn write_ascii(&mut self, value: u8) -> fmt::Result {
        debug_assert!(value < 0x80);
        return if value < b' ' {
            self.lower.write_ascii(b' ')
        } else {
            self.lower.write_ascii(value)
        };
    }

    fn write_non_ascii(&mut self, value: char) -> fmt::Result {
        self.lower.write_non_ascii(value)
    }
}

impl<'low, F: Filter> fmt::Write for ControlCharsSpaceFilter<'low, F> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let bytes = value.as_bytes();
        let mut written_pos = 0;
        for pos in 0..bytes.len() {
            let b = bytes[pos];
            if b < b' ' {
                if written_pos < pos {
                    // Validated: if input is valid utf-8 then substring is also valid utf-8.
                    self.lower
                        .write_str(str_from_validated_utf8(&bytes[written_pos..pos]))?;
                }

                self.lower.write_ascii(b' ')?;
                written_pos = pos + 1;
            }
        }

        return if written_pos < bytes.len() {
            // Validated: if input is valid utf-8 then substring is also valid utf-8.
            self.lower
                .write_str(str_from_validated_utf8(&bytes[written_pos..]))
        } else {
            Ok(())
        };
    }
}

// **** ControlCharsJsonFilter

/// Replaces control char with JSON-escaped sequence. (Does NOT replace `'"'` or `'\\'`.)
pub struct ControlCharsJsonFilter<'low, F: Filter> {
    json: JsonFilterImpl<'low, true, F>,
}

impl<'low, F: Filter> ControlCharsJsonFilter<'low, F> {
    pub fn new(lower: &'low mut F) -> Self {
        return Self {
            json: JsonFilterImpl { lower },
        };
    }
}

impl<'low, F: Filter> Filter for ControlCharsJsonFilter<'low, F> {
    fn write_ascii(&mut self, value: u8) -> fmt::Result {
        debug_assert!(value < 0x80);
        self.json.write_ascii_impl(value)
    }

    fn write_non_ascii(&mut self, value: char) -> fmt::Result {
        self.json.write_non_ascii_impl(value)
    }
}

impl<'low, F: Filter> fmt::Write for ControlCharsJsonFilter<'low, F> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.json.write_str_impl(value)
    }
}

// **** JsonEscapeFilter

/// Replaces JSON-invalid char with JSON-escaped sequence. (Includes control char, `'"'` and `'\\'`.)
pub struct JsonEscapeFilter<'low, F: Filter> {
    json: JsonFilterImpl<'low, false, F>,
}

impl<'low, F: Filter> JsonEscapeFilter<'low, F> {
    pub fn new(lower: &'low mut F) -> Self {
        return Self {
            json: JsonFilterImpl { lower },
        };
    }
}

impl<'low, F: Filter> Filter for JsonEscapeFilter<'low, F> {
    fn write_ascii(&mut self, value: u8) -> fmt::Result {
        debug_assert!(value < 0x80);
        self.json.write_ascii_impl(value)
    }

    fn write_non_ascii(&mut self, value: char) -> fmt::Result {
        self.json.write_non_ascii_impl(value)
    }
}

impl<'low, F: Filter> fmt::Write for JsonEscapeFilter<'low, F> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.json.write_str_impl(value)
    }
}

// **** JsonFilterImpl

/// Replace invalid char with JSON escape sequence.
/// Includes control chars. Optionally includes `'"'` and `'\\'`.
struct JsonFilterImpl<'low, const CONTROL_CHARS_ONLY: bool, F: Filter> {
    lower: &'low mut F,
}

impl<'low, const CONTROL_CHARS_ONLY: bool, F: Filter> JsonFilterImpl<'low, CONTROL_CHARS_ONLY, F> {
    pub fn write_ascii_impl(&mut self, value: u8) -> fmt::Result {
        debug_assert!(value < 0x80);

        match value {
            b'"' if !CONTROL_CHARS_ONLY => self.lower.write_str("\\\""),
            b'\\' if !CONTROL_CHARS_ONLY => self.lower.write_str("\\\\"),
            b'\x08' => self.lower.write_str("\\b"),
            b'\x0C' => self.lower.write_str("\\f"),
            b'\n' => self.lower.write_str("\\n"),
            b'\r' => self.lower.write_str("\\r"),
            b'\t' => self.lower.write_str("\\t"),

            b'\0'..=b'\x1F' => {
                const UESCAPE_SIZE: usize = 6;
                let uescape: [u8; UESCAPE_SIZE] = [
                    b'\\',
                    b'u',
                    b'0',
                    b'0',
                    if value < 0x10 { b'0' } else { b'1' },
                    Self::to_hex_char_uppercase(value),
                ];

                // Validated: "uescape" is valid utf-8.
                self.lower.write_str(str_from_validated_utf8(&uescape))
            }

            _ => self.lower.write_ascii(value),
        }
    }

    pub fn write_non_ascii_impl(&mut self, value: char) -> fmt::Result {
        self.lower.write_non_ascii(value)
    }

    pub fn write_str_impl(&mut self, value: &str) -> fmt::Result {
        let check_char: u8 = if CONTROL_CHARS_ONLY { 0x1F } else { b'\\' };
        let bytes = value.as_bytes();
        let mut written_pos = 0;
        for pos in 0..bytes.len() {
            let b = bytes[pos];
            if b <= check_char {
                if written_pos <= pos {
                    // Validated: if input is valid utf-8 then substring is also valid utf-8.
                    self.lower
                        .write_str(str_from_validated_utf8(&bytes[written_pos..pos]))?;
                }

                self.write_ascii_impl(b)?;
                written_pos = pos + 1;
            }
        }

        return if written_pos < bytes.len() {
            // Validated: if input is valid utf-8 then substring is also valid utf-8.
            self.lower
                .write_str(str_from_validated_utf8(&bytes[written_pos..]))
        } else {
            Ok(())
        };
    }

    #[inline]
    fn to_hex_char_uppercase(nibble: u8) -> u8 {
        return match nibble & 0xF {
            0..=9 => b'0' + nibble,
            10..=15 => b'A' + nibble - 10,
            _ => unreachable!(),
        };
    }
}
