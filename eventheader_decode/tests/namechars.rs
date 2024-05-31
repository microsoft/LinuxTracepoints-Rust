use std::string;

use eventheader_decode::NameChars;

/// Make sure that NameChars::write_to, fmt, and iterator all return the expected value.
fn check_namechars(bytes: &[u8]) {
    // Determine the expected answer:
    // If the first few bytes are valid utf-8, append them.
    // Otherwise append one byte treated as Latin1.
    let mut expected = string::String::new();
    let mut pos = 0;
    let len = bytes.len();
    while pos < len {
        let b0 = bytes[pos];

        // If b0 is a lead byte, how long would the sequence be?
        let cb = match b0 {
            0x00..=0xBF => 1, // 0x80..=0xBF are always invalid, but from_utf will catch them.
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xFF => 4, // 0xF5..=0xFF are always invalid, but from_utf will catch them.
        };

        // Are the next cb bytes a valid UTF-8 sequence? If so, append them and continue.
        if len - pos >= cb {
            if let Ok(s) = core::str::from_utf8(&bytes[pos..pos + cb]) {
                expected.push_str(s);
                pos += cb;
                continue;
            }
        }

        // Not a valid UTF-8 sequence. Append the next byte, treated as latin1.
        expected.push(b0 as char);
        pos += 1;
    }

    let mut from_write_to = string::String::new();
    NameChars::new(bytes).write_to(&mut from_write_to).unwrap();
    assert_eq!(expected, from_write_to);

    let from_enum = NameChars::new(bytes).collect::<String>();
    assert_eq!(expected, from_enum);

    let from_display = format!("{}", NameChars::new(bytes));
    assert_eq!(expected, from_display);
}

#[test]
fn name_chars() {
    // Pick interesting byte values, then test all combinations of those values up to 5 bytes.
    let categories: [u8; 14] = [
        0x00, 0x01, 0x7F, 0x80, 0x81, 0xBF, 0xC0, 0xDF, 0xE0, 0xEF, 0xF0, 0xF4, 0xF5, 0xFF,
    ];
    check_namechars(b"");
    for b0 in categories {
        check_namechars(&[b0]);
        for b1 in categories {
            check_namechars(&[b0, b1]);
            for b2 in categories {
                check_namechars(&[b0, b1, b2]);
                for b3 in categories {
                    check_namechars(&[b0, b1, b2, b3]);
                    for b4 in categories {
                        check_namechars(&[b0, b1, b2, b3, b4]);
                    }
                }
            }
        }
    }
}
