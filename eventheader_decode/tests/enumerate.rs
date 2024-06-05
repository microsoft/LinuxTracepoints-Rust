use std::env;
use std::fmt;
use std::fs;

use eventheader_decode::_internal::JsonWriter;
use eventheader_decode::*;

fn strnlen(bytes: &[u8]) -> usize {
    let mut len = 0;
    while len < bytes.len() && bytes[len] != 0 {
        len += 1;
    }
    return len;
}

/// For each event in the EventHeaderInterceptorLE64.dat file, use EventHeaderEnumerator to
/// enumerate the fields of the event. Generate JSON with the results.
fn enumerate_impl(
    output_filename: &str,
    buffer: &mut String,
    move_next_sibling: bool,
) -> Result<(), fmt::Error> {
    const OPTIONS: PerfConvertOptions = PerfConvertOptions::Default
        .and_not(PerfConvertOptions::BoolOutOfRangeAsString)
        .or(PerfConvertOptions::FloatExtraPrecision);

    let mut tmp_str = String::new();
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

                if e.move_next() {
                    loop {
                        let ii = e.item_info();
                        let m = ii.metadata();
                        match e.state() {
                            EventHeaderEnumeratorState::Value => {
                                if !m.is_element() {
                                    json.write_property_name_from_item_info(&ii)?;
                                }

                                tmp_str.clear();
                                ii.value().write_json_scalar_to(&mut tmp_str, OPTIONS)?;
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
                                if move_next_sibling && m.type_size() != 0 {
                                    tmp_str.clear();
                                    ii.value()
                                        .write_json_simple_array_to(&mut tmp_str, OPTIONS)?;
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
                json.write_value(|w| w.write_display_with_no_filter(ei.json_info_display()))?;
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

    let out_path = env::current_dir().unwrap().join(output_filename);
    fs::write(out_path, buffer.as_bytes()).unwrap();
    println!("{}: {}", output_filename, buffer);
    return Ok(());
}

#[test]
fn enumerate() -> Result<(), fmt::Error> {
    let mut movenext_buffer = String::new();
    enumerate_impl(".enumerate_movenext.json", &mut movenext_buffer, false)?;

    let mut movenextsibling_buffer = String::new();
    enumerate_impl(
        ".enumerate_movenextsibling.json",
        &mut movenextsibling_buffer,
        true,
    )?;

    assert_eq!(movenext_buffer, movenextsibling_buffer);
    return Ok(());
}
