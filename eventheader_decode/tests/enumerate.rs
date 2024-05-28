use std::env;
use std::fmt::Write;
use std::fs;

use eventheader_decode::*;

fn strnlen(bytes: &[u8]) -> usize {
    let mut len = 0;
    while len < bytes.len() && bytes[len] != 0 {
        len += 1;
    }
    return len;
}

struct JsonWriter {
    buffer: String,
    comma: bool,
    current_space: bool,
    next_space: bool,
}

impl JsonWriter {
    pub fn new(space: bool, comma: bool) -> JsonWriter {
        JsonWriter {
            buffer: String::new(),
            comma,
            current_space: space && comma,
            next_space: space,
        }
    }

    pub fn write_newline(&mut self, indent: usize) {
        if self.comma {
            self.buffer.push(',');
        }
        self.buffer.push('\n');
        for _ in 0..indent {
            self.buffer.push_str("  ");
        }
        self.comma = false;
        self.current_space = self.next_space;
    }

    pub fn write_property_name(&mut self, name: &str) {
        self.comma_space();
        self.buffer.push('"');
        self.write_raw_escaped(name);
        self.buffer.push_str("\":");
        self.comma = false;
    }

    pub fn write_property_name_no_escape(&mut self, name: &str) {
        self.comma_space();
        self.buffer.push('"');
        self.write_raw(name);
        self.buffer.push_str("\":");
        self.comma = false;
    }

    pub fn write_start_object(&mut self) {
        self.comma_space();
        self.buffer.push('{');
        self.comma = false;
    }

    pub fn write_end_object(&mut self) {
        if self.current_space {
            self.buffer.push(' ');
        }
        self.buffer.push('}');
        self.comma = true;
    }

    pub fn write_start_array(&mut self) {
        self.comma_space();
        self.buffer.push('[');
        self.comma = false;
    }

    pub fn write_end_array(&mut self) {
        if self.current_space {
            self.buffer.push(' ');
        }
        self.buffer.push(']');
        self.comma = true;
    }

    pub fn write_value_str(&mut self, s: &str) {
        self.write_value_start();
        self.buffer.push('"');
        self.write_raw_escaped(s);
        self.buffer.push('"');
    }

    pub fn write_value_fmt_str(&mut self, fmt: std::fmt::Arguments) {
        self.write_value_start();
        self.buffer.push('"');
        self.buffer.write_fmt(fmt).unwrap();
        self.buffer.push('"');
    }

    pub fn write_value_start_str(&mut self) {
        self.comma_space();
        self.buffer.push('"');
        self.comma = true;
    }

    pub fn write_value_end_str(&mut self) {
        self.buffer.push('"');
    }

    pub fn write_value_fmt(&mut self, fmt: std::fmt::Arguments) {
        self.write_value_start();
        self.buffer.write_fmt(fmt).unwrap();
    }

    pub fn write_value_start(&mut self) {
        self.comma_space();
        self.comma = true;
    }

    pub fn write_raw(&mut self, s: &str) {
        self.buffer.push_str(s);
    }

    pub fn write_raw_fmt(&mut self, fmt: std::fmt::Arguments) {
        self.buffer.write_fmt(fmt).unwrap();
    }

    pub fn write_raw_escaped(&mut self, s: &str) {
        for c in s.chars() {
            match c {
                '"' => self.buffer.push_str("\\\""),
                '\\' => self.buffer.push_str("\\\\"),
                '\x08' => self.buffer.push_str("\\b"),
                '\x0C' => self.buffer.push_str("\\f"),
                '\x0A' => self.buffer.push_str("\\n"),
                '\x0D' => self.buffer.push_str("\\r"),
                '\x09' => self.buffer.push_str("\\t"),
                '\0'..='\x1F' => unsafe {
                    let c8 = c as u8;
                    self.buffer.push_str(std::str::from_utf8_unchecked(&[
                        b'\\',
                        b'u',
                        b'0',
                        b'0',
                        Self::to_hex_char(c8 / 16),
                        Self::to_hex_char(c8),
                    ]))
                },
                _ => self.buffer.push(c),
            }
        }
    }

    fn to_hex_char(nibble: u8) -> u8 {
        return match nibble & 0xF {
            0..=9 => b'0' + nibble,
            10..=15 => b'A' + nibble - 10,
            _ => unreachable!(),
        };
    }

    fn comma_space(&mut self) {
        if self.comma {
            self.buffer.push(',');
        }
        if self.current_space {
            self.buffer.push(' ');
        }
        self.current_space = self.next_space;
    }
}

fn enumerate(json: &mut JsonWriter, e: &mut EventHeaderEnumerator, move_next_sibling: bool) {
    if e.move_next() {
        loop {
            let ii = e.item_info();
            let m = ii.metadata();
            match e.state() {
                EventHeaderEnumeratorState::Value => {
                    if !m.is_element() {
                        json.write_property_name(ii.name().unwrap());
                    }
                    json.write_value_str("value"); // TODO
                }
                EventHeaderEnumeratorState::StructBegin => {
                    if !m.is_element() {
                        json.write_property_name(ii.name().unwrap());
                    }
                    json.write_start_object();
                }
                EventHeaderEnumeratorState::StructEnd => json.write_end_object(),
                EventHeaderEnumeratorState::ArrayBegin => {
                    json.write_property_name(ii.name().unwrap());
                    if move_next_sibling && m.type_size() != 0 {
                        json.write_start_array();
                        json.write_value_fmt(format_args!("{}", m.element_count())); // TODO
                        json.write_end_array();

                        if !e.move_next_sibling() {
                            break;
                        }

                        continue; // skip move_next()
                    }
                    json.write_start_array();
                }
                EventHeaderEnumeratorState::ArrayEnd => json.write_end_array(),
                _ => {
                    json.write_property_name_no_escape("unexpected_state");
                    json.write_value_fmt_str(format_args!("{:?}", e.state()));
                }
            }

            if !e.move_next() {
                break;
            }
        }
    }
}

#[test]
fn enumerate_movenext() {
    let mut dat_path = env::current_dir().unwrap();
    dat_path.push("test_data");
    dat_path.push("EventHeaderInterceptorLE64.dat");

    let mut ctx = EventHeaderEnumeratorContext::new();
    let mut json = JsonWriter::new(true, false);

    json.write_start_array();

    let dat_vec = fs::read(dat_path).unwrap();
    let dat_bytes = &dat_vec[..];
    let dat_size = dat_bytes.len();
    let mut pos = 0;
    while pos < dat_size {
        assert!(dat_size - pos >= 4);
        let size = u32::from_le_bytes(dat_bytes[pos..pos + 4].try_into().unwrap()) as usize;
        assert!(size >= 4);
        assert!(size <= dat_bytes.len() - pos);

        let name_pos = pos + 4;
        pos += size;

        let name_len = strnlen(&dat_bytes[name_pos..pos]);
        assert!(name_pos + name_len < pos);

        let tracepoint_name =
            std::str::from_utf8(&dat_bytes[name_pos..name_pos + name_len]).unwrap();
        let event_data = &dat_bytes[name_pos + name_len + 1..pos];
        match ctx.enumerate(tracepoint_name, event_data) {
            Err(e) => {
                json.write_newline(1);
                json.write_start_object();
                json.write_property_name_no_escape("n");
                json.write_value_str(tracepoint_name);
                json.write_property_name_no_escape("enumerate_error");
                json.write_value_fmt_str(format_args!("{:?}", e));
                json.write_end_object();
            }
            Ok(mut e) => {
                let ei = e.event_info();
                json.write_newline(1);
                json.write_start_object();

                json.write_property_name_no_escape("n");
                json.write_value_start_str();
                json.write_raw_escaped(ei.provider_name());
                json.write_raw(":");
                json.write_raw_escaped(ei.name().unwrap());
                json.write_value_end_str();

                enumerate(&mut json, &mut e, false);

                json.write_end_object();
            }
        }
    }

    json.write_end_array();
    println!("{}", json.buffer);
}
