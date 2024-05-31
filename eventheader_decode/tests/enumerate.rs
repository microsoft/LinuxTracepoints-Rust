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

fn enumerate<'a>(
    json: &mut JsonWriter<'a, String>,
    e: &mut EventHeaderEnumerator,
    move_next_sibling: bool,
) -> Result<(), fmt::Error> {
    if e.move_next() {
        loop {
            let ii = e.item_info();
            let m = ii.metadata();
            match e.state() {
                EventHeaderEnumeratorState::Value => {
                    if !m.is_element() {
                        json.write_property_name_from_item_info(&ii)?;
                    }
                    json.write_string_value_json_safe("value")?; // TODO
                }
                EventHeaderEnumeratorState::StructBegin => {
                    if !m.is_element() {
                        json.write_property_name_from_item_info(&ii)?;
                    }
                    json.write_start_object()?;
                }
                EventHeaderEnumeratorState::StructEnd => json.write_end_object()?,
                EventHeaderEnumeratorState::ArrayBegin => {
                    json.write_property_name_from_item_info(&ii)?;
                    if move_next_sibling && m.type_size() != 0 {
                        json.write_start_array()?;
                        json.write_string_value_fmt_json_safe(format_args!(
                            "{}",
                            m.element_count()
                        ))?; // TODO
                        json.write_end_array()?;

                        if !e.move_next_sibling() {
                            break;
                        }

                        continue; // skip move_next()
                    }
                    json.write_start_array()?;
                }
                EventHeaderEnumeratorState::ArrayEnd => json.write_end_array()?,
                _ => {
                    json.write_property_name_json_safe("unexpected_state")?;
                    json.write_string_value_fmt_json_safe(format_args!("{:?}", e.state()))?;
                }
            }

            if !e.move_next() {
                break;
            }
        }
    }
    return Ok(());
}

/// For each event in the EventHeaderInterceptorLE64.dat file, use EventHeaderEnumerator to
/// enumerate the fields of the event. Generate JSON with the results.
/// TODO: Compare the resulting JSON with a known-good JSON file, pending implementation of
/// PerfItemValue::write support.
#[test]
fn enumerate_movenext() -> Result<(), fmt::Error> {
    let mut dat_path = env::current_dir().unwrap();
    dat_path.push("test_data");
    dat_path.push("EventHeaderInterceptorLE64.dat");

    let mut ctx = EventHeaderEnumeratorContext::new();
    let mut buffer = String::new();
    let mut json = JsonWriter::new(&mut buffer, PerfConvertOptions::Default, false);

    json.write_start_array()?;

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
                json.write_start_object()?;
                json.write_property_name_json_safe("n")?;
                json.write_string_value(tracepoint_name)?;
                json.write_property_name_json_safe("enumerate_error")?;
                json.write_string_value_fmt_json_safe(format_args!("{:?}", e))?;
                json.write_end_object()?;
            }
            Ok(mut e) => {
                let ei = e.event_info();
                json.write_newline_before_value(1)?;
                json.write_start_object()?;

                json.write_property_name_json_safe("n")?;
                json.write_string_value_fmt(format_args!(
                    "{}:{}",
                    ei.provider_name(),
                    ei.name_chars(),
                ))?;

                enumerate(&mut json, &mut e, false)?;

                json.write_end_object()?;
            }
        }
    }

    json.write_end_array()?;

    println!("{}", buffer);
    return Ok(());
}
