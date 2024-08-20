// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

use core::fmt;
use core::str;

use crate::filters;

#[inline]
fn char_from_validated_u32(valid_ch32: u32) -> char {
    debug_assert!(!(0xD800..=0xDFFF).contains(&valid_ch32) && valid_ch32 < 0x110000);
    return unsafe { char::from_u32_unchecked(valid_ch32) };
}

#[inline]
fn str_from_validated_utf8(valid_utf8: &[u8]) -> &str {
    debug_assert!(core::str::from_utf8(valid_utf8).is_ok());
    return unsafe { str::from_utf8_unchecked(valid_utf8) };
}

// **** write_latin1_to

/// Writes a Latin-1-encoded string to a filter.
pub fn write_latin1_to<F: filters::Filter>(bytes: &[u8], filter: &mut F) -> fmt::Result {
    let len = bytes.len();
    let mut written_pos = 0;
    for pos in 0..len {
        let b = bytes[pos];
        if b <= 0x7F {
            // ASCII. Continue.
            continue;
        }

        // bytes[pos..] is not ASCII.

        // Flush the ASCII, if any.
        if written_pos < pos {
            // Validated: substring contains only ASCII.
            filter.write_str(str_from_validated_utf8(&bytes[written_pos..pos]))?;
        }

        filter.write_non_ascii(b as char)?;
        written_pos = pos + 1;
    }

    // Write any remaining ASCII. (Common case: the entire string.)
    return if written_pos < len {
        // Validated: substring contains only ASCII.
        filter.write_str(str_from_validated_utf8(&bytes[written_pos..]))
    } else {
        Ok(())
    };
}

// **** write_utf8_with_latin1_fallback_to

/// Writes a UTF-8-encoded string to a filter. If the string contains any invalid UTF-8 sequences,
/// the sequences are treated as Latin-1.
pub fn write_utf8_with_latin1_fallback_to<F: filters::Filter>(
    bytes: &[u8],
    filter: &mut F,
) -> fmt::Result {
    let len = bytes.len();
    let mut written_pos = 0;
    let mut pos = 0;
    while pos < len {
        // If this is valid UTF-8, update pos and continue to next iteration.
        // If this is not valid UTF-8, fall-through to the Latin1 case.
        let b0 = bytes[pos];

        if b0 <= 0x7F {
            // 0x00..0x7F: Valid UTF-8. Continue.
            pos += 1;
            continue;
        } else if b0 <= 0xBF {
            // Invalid lead byte. Fall-through.
        } else if b0 <= 0xDF {
            if len - pos >= 2 {
                let b1 = bytes[pos + 1];
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
                let b1 = bytes[pos + 1];
                let b2 = bytes[pos + 2];
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
                let b1 = bytes[pos + 1];
                let b2 = bytes[pos + 2];
                let b3 = bytes[pos + 3];
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

        // bytes[pos..] is not valid UTF-8.

        // Flush the valid UTF-8, if any.
        if written_pos < pos {
            // Validated: substring contains only valid UTF-8.
            filter.write_str(str_from_validated_utf8(&bytes[written_pos..pos]))?;
        }

        // Treat bytes[pos] as Latin1 and move forward.
        filter.write_non_ascii(b0 as char)?;
        written_pos = pos + 1;
        pos = written_pos;
    }

    // Write any remaining valid UTF-8. (Common case: the entire string.)
    return if written_pos < len {
        filter.write_str(str_from_validated_utf8(&bytes[written_pos..]))
    } else {
        Ok(())
    };
}

// **** write_utf16_be_to, write_utf16_le_to

fn write_utf16_to<const BIG_ENDIAN: bool, F: filters::Filter>(
    bytes: &[u8],
    filter: &mut F,
) -> fmt::Result {
    let len = bytes.len();
    let mut pos = 0;
    while len - pos >= 2 {
        let high = if BIG_ENDIAN {
            u16::from_be_bytes(bytes[pos..pos + 2].try_into().unwrap())
        } else {
            u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap())
        };
        pos += 2; // Consume the first code unit.

        let ch;
        if high <= 0x7F {
            // ASCII
            filter.write_ascii(high as u8)?;
            continue;
        } else if !(0xD800..=0xDFFF).contains(&high) {
            // Not ASCII, not a surrogate.
            ch = char_from_validated_u32(high as u32);
        } else if high >= 0xDC00 || len - pos < 2 {
            // Invalid or unpaired high surrogate.
            ch = char::REPLACEMENT_CHARACTER;
        } else {
            // Valid high surrogate. Possibly valid low surrogate.
            let low = if BIG_ENDIAN {
                u16::from_be_bytes(bytes[pos..pos + 2].try_into().unwrap())
            } else {
                u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap())
            };

            ch = if !(0xDC00..=0xDFFF).contains(&low) {
                char::REPLACEMENT_CHARACTER // Unpaired high surrogate.
            } else {
                pos += 2; // Consume the second code unit.
                char_from_validated_u32(
                    (((high as u32 - 0xD800) << 10) | (low as u32 - 0xDC00)) + 0x10000,
                )
            };
        }

        filter.write_non_ascii(ch)?;
    }

    return Ok(());
}

/// Writes a UTF-16BE-encoded string to a filter. Invalid code units are replaced with the
/// replacement character.
pub fn write_utf16be_to<F: filters::Filter>(bytes: &[u8], filter: &mut F) -> fmt::Result {
    return write_utf16_to::<true, F>(bytes, filter);
}

/// Writes a UTF-16LE-encoded string to a filter. Invalid code units are replaced with the
/// replacement character.
pub fn write_utf16le_to<F: filters::Filter>(bytes: &[u8], filter: &mut F) -> fmt::Result {
    return write_utf16_to::<false, F>(bytes, filter);
}

// **** write_utf32_be_to, write_utf32_le_to

fn write_utf32_to<const BIG_ENDIAN: bool, F: filters::Filter>(
    bytes: &[u8],
    filter: &mut F,
) -> fmt::Result {
    let len = bytes.len();
    let mut pos = 0;
    while len - pos >= 4 {
        let ch32 = if BIG_ENDIAN {
            u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap())
        } else {
            u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap())
        };
        pos += 4; // Consume.

        let ch = char::from_u32(ch32).unwrap_or(char::REPLACEMENT_CHARACTER);
        filter.write_non_ascii(ch)?;
    }

    return Ok(());
}

/// Writes a UTF-32BE-encoded string to a filter. Invalid code units are replaced with the
/// replacement character.
pub fn write_utf32be_to<F: filters::Filter>(bytes: &[u8], filter: &mut F) -> fmt::Result {
    return write_utf32_to::<true, F>(bytes, filter);
}

/// Writes a UTF-32LE-encoded string to a filter. Invalid code units are replaced with the
/// replacement character.
pub fn write_utf32le_to<F: filters::Filter>(bytes: &[u8], filter: &mut F) -> fmt::Result {
    return write_utf32_to::<false, F>(bytes, filter);
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::string;

    use super::*;

    #[test]
    fn test_write_latin1_to() {
        fn check(expected: &str, input: &[u8]) {
            let mut actual = string::String::new();
            let mut actual_writer = filters::WriteFilter::new(&mut actual);
            write_latin1_to(input, &mut actual_writer).unwrap();
            assert_eq!(expected, actual);
        }

        check("", b"");
        check("A", b"A");
        check("A\u{80}", b"A\x80");
        check("\u{80}", b"\x80");
        check("\u{80}A", b"\x80A");
        check("Hello, world!", b"Hello, world!");
        check("Hello, \u{80}world!", b"Hello, \x80world!");
        check("Hello, \u{FF}", b"Hello, \xFF");
    }

    #[test]
    fn test_write_utf8_with_latin1_fallback_to() {
        fn check(expected: &str, input: &[u8]) {
            let mut actual = string::String::new();
            let mut actual_writer = filters::WriteFilter::new(&mut actual);
            write_utf8_with_latin1_fallback_to(input, &mut actual_writer).unwrap();
            assert_eq!(expected, actual);
        }

        fn check_u32(i: u32) {
            let mut buf4 = [0u8; 4];

            if let Some(ch) = char::from_u32(i) {
                // utf-8 encoding of valid Unicode code point should pass through unchanged.
                let s = ch.encode_utf8(&mut buf4[..]);
                let sb = s.as_bytes();

                check(s, sb);
            } else {
                // Invalid Unicode code points. Encode as UTF-8 anyway, then verify that
                // it is recognized as invalid and treated as Latin-1.
                debug_assert!(i >= 0x800); // There are no invalid code points below 0x800.
                debug_assert!(i <= 0x1FFFFF); // Should not be called after 0x1FFFFF.
                let len;
                if i <= 0xFFFF {
                    buf4[0] = (0xE0 | ((i >> 12) & 0x0F)) as u8;
                    buf4[1] = (0x80 | ((i >> 6) & 0x3F)) as u8;
                    buf4[2] = (0x80 | (i & 0x3F)) as u8;
                    len = 3;
                } else {
                    buf4[0] = (0xF0 | ((i >> 18) & 0x07)) as u8;
                    buf4[1] = (0x80 | ((i >> 12) & 0x3F)) as u8;
                    buf4[2] = (0x80 | ((i >> 6) & 0x3F)) as u8;
                    buf4[3] = (0x80 | (i & 0x3F)) as u8;
                    len = 4;
                }

                let mut str = string::String::new();
                for j in 0..len {
                    str.push(char::from_u32(buf4[j] as u32).unwrap());
                }
                check(&str, &buf4[..len]);
            }
        }

        // Check handling of valid and invalid code points that have been encoded into utf-8.
        for i in 0x0..0xFFFF {
            check_u32(i);
        }
        for i in (0x0..0x1F0000).step_by(0x10000) {
            check_u32(i);
            check_u32(i | 0xFFFF);
        }

        // Valid utf-8 should pass through unchanged.
        let valid_strings = [
            "",
            "Hello, world!",
            "\u{0}\u{7F}\u{80}\u{7FF}\u{800}\u{D7FF}\u{E000}\u{FFFF}\u{10000}\u{10FFFF}",
        ];
        for valid_string in valid_strings.iter() {
            check(valid_string, valid_string.as_bytes());
        }
    }

    #[test]
    fn test_write_utf16_to() {
        fn check(expected: &str, input_big_endian: &mut [u8]) {
            let mut actual = string::String::new();

            let mut actual_writer = filters::WriteFilter::new(&mut actual);
            write_utf16_to::<true, _>(input_big_endian, &mut actual_writer).unwrap();
            assert_eq!(expected, actual);

            for i in 0..(input_big_endian.len() / 2) {
                input_big_endian.swap(i * 2, i * 2 + 1);
            }

            actual.clear();
            let mut actual_writer = filters::WriteFilter::new(&mut actual);
            write_utf16_to::<false, _>(&input_big_endian, &mut actual_writer).unwrap();
            assert_eq!(expected, actual);
        }

        // Note: input is big-endian.
        check("", &mut []); // Empty.
        check("", &mut [99]); // Odd length should be ignored.
        check("0", &mut [0x00, 0x30]); // ASCII.
        check("0", &mut [0x00, 0x30, 99]); // Odd length should be ignored.
        check("\u{FF}", &mut [0x00, 0xFF]); // Non-ASCII.
        check("\u{100}", &mut [0x01, 0x00]); // Non-ASCII.
        check("\u{D7FF}", &mut [0xD7, 0xFF]); // Non-ASCII.
        check("\u{10000}", &mut [0xD8, 0x00, 0xDC, 0x00]); // Valid surrogate pair.
        check("\u{103FF}", &mut [0xD8, 0x00, 0xDF, 0xFF]); // Valid surrogate pair.
        check("\u{10FC00}", &mut [0xDB, 0xFF, 0xDC, 0x00]); // Valid surrogate pair.
        check("\u{10FFFF}", &mut [0xDB, 0xFF, 0xDF, 0xFF]); // Valid surrogate pair.

        check("\u{FFFD}", &mut [0xD8, 0x00]); // Unpaired high surrogate.
        check("\u{FFFD}0", &mut [0xDB, 0xFF, 0x00, 0x30]); // Unpaired high surrogate followed by '0'.
        check("\u{FFFD}", &mut [0xDC, 0x00]); // Unpaired low surrogate.
        check("\u{FFFD}0", &mut [0xDF, 0xFF, 0x00, 0x30]); // Unpaired low surrogate followed by '0'.
    }

    #[test]
    fn test_write_utf32_to() {
        fn check(expected: &str, input_big_endian: &mut [u8]) {
            let mut actual = string::String::new();

            let mut actual_writer = filters::WriteFilter::new(&mut actual);
            write_utf32_to::<true, _>(input_big_endian, &mut actual_writer).unwrap();
            assert_eq!(expected, actual);

            for i in 0..(input_big_endian.len() / 4) {
                input_big_endian.swap(i * 4, i * 4 + 3);
                input_big_endian.swap(i * 4 + 1, i * 4 + 2);
            }

            actual.clear();
            let mut actual_writer = filters::WriteFilter::new(&mut actual);
            write_utf32_to::<false, _>(&input_big_endian, &mut actual_writer).unwrap();
            assert_eq!(expected, actual);
        }

        // Note: input is big-endian.
        check("", &mut []); // Empty.
        check("", &mut [99]); // Odd length should be ignored.
        check("", &mut [0, 99]); // Odd length should be ignored.
        check("", &mut [0, 0, 99]); // Odd length should be ignored.
        check("0", &mut [0x00, 0x00, 0x00, 0x30]); // ASCII.
        check("0", &mut [0x00, 0x00, 0x00, 0x30, 99]); // Odd length should be ignored.
        check("0", &mut [0x00, 0x00, 0x00, 0x30, 0, 99]); // Odd length should be ignored.
        check("0", &mut [0x00, 0x00, 0x00, 0x30, 0, 0, 99]); // Odd length should be ignored.
        check("\u{FF}", &mut [0x00, 0x00, 0x00, 0xFF]); // Non-ASCII.
        check("\u{100}", &mut [0x00, 0x00, 0x01, 0x00]);
        check("\u{D7FF}", &mut [0x00, 0x00, 0xD7, 0xFF]);
        check("\u{10000}", &mut [0x00, 0x01, 0x00, 0x00]);
        check("\u{10FFFF}", &mut [0x00, 0x10, 0xFF, 0xFF]); // Limit

        // Invalid char handling.
        check("\u{FFFD}", &mut [0x00, 0x11, 0x00, 0x00]);
        check(
            "\u{FFFD}0",
            &mut [0x00, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x30],
        );
        check("\u{FFFD}", &mut [0x00, 0x00, 0xDC, 0x00]);
        check(
            "\u{FFFD}0",
            &mut [0x01, 0x00, 0x00, 0x30, 0x00, 0x00, 0x00, 0x30],
        );
    }
}
