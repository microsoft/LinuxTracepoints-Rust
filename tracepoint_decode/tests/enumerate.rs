use std::env;
use std::fmt;
use std::fs;
use std::time;

use tracepoint_decode::_internal::JsonWriter;
use tracepoint_decode::*;

fn strnlen(bytes: &[u8]) -> usize {
    let mut len = 0;
    while len < bytes.len() && bytes[len] != 0 {
        len += 1;
    }
    return len;
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Method {
    MoveNext,
    MoveNextSibling,
    WriteItem,
}

/// For each event in the EventHeaderInterceptorLE64.dat file, use EventHeaderEnumerator to
/// enumerate the fields of the event. Generate JSON with the results.
fn enumerate_impl(
    output_filename: &str,
    buffer: &mut String,
    tmp_str: &mut String,
    method: Method,
) -> Result<(), fmt::Error> {
    const OPTIONS: PerfConvertOptions = PerfConvertOptions::Default
        .and_not(PerfConvertOptions::BoolOutOfRangeAsString)
        .or(PerfConvertOptions::FloatExtraPrecision);

    let mut dat_path = env::current_dir().unwrap();
    dat_path.push("test_data");
    dat_path.push("EventHeaderInterceptorLE64.dat");

    let mut ctx = EventHeaderEnumeratorContext::new();
    buffer.push('\u{FEFF}');
    let mut json = JsonWriter::new(buffer, OPTIONS, false);

    json.write_array_begin()?;

    let dat_vec = fs::read(dat_path).unwrap();
    let dat_bytes = &dat_vec[..];
    let mut dat_pos = 0;
    while dat_pos < dat_bytes.len() {
        assert!(dat_bytes.len() - dat_pos >= 4);
        let size = u32::from_le_bytes(dat_bytes[dat_pos..dat_pos + 4].try_into().unwrap()) as usize;
        assert!(size >= 4);
        assert!(size <= dat_bytes.len() - dat_pos);

        let name_pos = dat_pos + 4;
        dat_pos += size;

        let name_len = strnlen(&dat_bytes[name_pos..dat_pos]);
        assert!(name_pos + name_len < dat_pos);

        let tracepoint_name =
            std::str::from_utf8(&dat_bytes[name_pos..name_pos + name_len]).unwrap();
        let event_data = &dat_bytes[name_pos + name_len + 1..dat_pos];
        match ctx.enumerate(tracepoint_name, event_data) {
            Err(e) => {
                json.write_newline_before_value(1)?;
                json.write_object_begin()?;
                json.write_property_name_json_safe("n")?;
                json.write_value_quoted(|w| w.write_str_with_json_escape(tracepoint_name))?;
                json.write_property_name_json_safe("enumerate_error")?;
                json.write_value_quoted(|w| w.write_display_with_no_filter(e))?;
                json.write_object_end()?;
            }
            Ok(mut e) => {
                let ei = e.event_info();
                json.write_newline_before_value(2)?;
                json.write_object_begin()?;

                json.write_property_name_json_safe("n")?;
                json.write_value_quoted(|w| w.write_display_with_no_filter(ei.identity_display()))?;

                if Method::WriteItem == method {
                    tmp_str.clear();
                    if e.write_item_and_move_next_sibling(tmp_str, false, OPTIONS)? {
                        json.write_value(|w| w.write_display_with_no_filter(&tmp_str))?;
                    }
                } else if e.move_next() {
                    loop {
                        let ii = e.item_info();
                        let m = ii.metadata();
                        match e.state() {
                            EventHeaderEnumeratorState::Value => {
                                if !m.is_element() {
                                    json.write_property_name_from_item_info(&ii)?;
                                }

                                tmp_str.clear();
                                ii.value().write_json_scalar_to(tmp_str, OPTIONS)?;
                                json.write_value(|w| w.write_display_with_no_filter(&tmp_str))?;
                            }
                            EventHeaderEnumeratorState::StructBegin => {
                                if !m.is_element() {
                                    json.write_property_name_from_item_info(&ii)?;
                                }
                                json.write_object_begin()?;
                            }
                            EventHeaderEnumeratorState::StructEnd => json.write_object_end()?,
                            EventHeaderEnumeratorState::ArrayBegin => {
                                json.write_property_name_from_item_info(&ii)?;
                                if Method::MoveNextSibling == method && m.type_size() != 0 {
                                    tmp_str.clear();
                                    ii.value().write_json_simple_array_to(tmp_str, OPTIONS)?;
                                    json.write_value(|w| w.write_display_with_no_filter(&tmp_str))?;

                                    if !e.move_next_sibling() {
                                        break;
                                    }

                                    continue; // skip move_next()
                                }
                                json.write_array_begin()?;
                            }
                            EventHeaderEnumeratorState::ArrayEnd => json.write_array_end()?,
                            _ => {
                                json.write_property_name_json_safe("unexpected_state")?;
                                json.write_value_quoted(|w| {
                                    w.write_display_with_no_filter(e.state())
                                })?;
                            }
                        }

                        if !e.move_next() {
                            break;
                        }
                    }
                }

                json.write_property_name_json_safe("meta")?;
                json.write_object_begin()?;
                json.write_value(|w| w.write_display_with_no_filter(ei.json_meta_display()))?;
                json.write_object_end()?;

                json.write_object_end()?;
            }
        }
    }

    json.write_array_end()?;

    if cfg!(windows) {
        buffer.push('\r');
    }

    buffer.push('\n');

    if !output_filename.is_empty() {
        let out_path = env::current_dir().unwrap().join(output_filename);
        fs::write(out_path, buffer.as_bytes()).unwrap();
    }

    return Ok(());
}

#[test]
fn enumerate() -> Result<(), fmt::Error> {
    let mut tmp_str = String::new();

    let mut movenext_buffer = String::new();
    enumerate_impl(
        ".enumerate_movenext.json",
        &mut movenext_buffer,
        &mut tmp_str,
        Method::MoveNext,
    )?;

    let mut movenextsibling_buffer = String::new();
    enumerate_impl(
        ".enumerate_movenextsibling.json",
        &mut movenextsibling_buffer,
        &mut tmp_str,
        Method::MoveNextSibling,
    )?;

    let mut writeitem_buffer = String::new();
    enumerate_impl(
        ".enumerate_writeitem.json",
        &mut writeitem_buffer,
        &mut tmp_str,
        Method::WriteItem,
    )?;

    assert_eq!(movenext_buffer, movenextsibling_buffer);
    assert_eq!(movenext_buffer, writeitem_buffer);
    return Ok(());
}

#[test]
#[ignore]
fn benchmark() -> Result<(), fmt::Error> {
    const ITERATIONS: usize = 1000;
    let mut buffer = String::new();
    let mut tmp_str = String::new();

    buffer.clear();
    enumerate_impl("", &mut buffer, &mut tmp_str, Method::MoveNext)?;
    let movenext_start = time::Instant::now();
    for _ in 0..ITERATIONS {
        buffer.clear();
        enumerate_impl("", &mut buffer, &mut tmp_str, Method::MoveNext)?;
    }
    let movenext_duration = movenext_start.elapsed();

    buffer.clear();
    enumerate_impl("", &mut buffer, &mut tmp_str, Method::MoveNextSibling)?;
    let movenextsibling_start = time::Instant::now();
    for _ in 0..ITERATIONS {
        buffer.clear();
        enumerate_impl("", &mut buffer, &mut tmp_str, Method::MoveNextSibling)?;
    }
    let movenextsibling_duration = movenextsibling_start.elapsed();

    print!(
        "MoveNext: {:?}\nMoveNextSibling: {:?}\n",
        movenext_duration, movenextsibling_duration,
    );

    return Ok(());
}
