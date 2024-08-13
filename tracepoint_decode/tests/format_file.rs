use std::env;
use std::fs;
use std::io;
use std::io::Read;
use std::io::Write;

use zip;

use tracepoint_decode as td;

#[test]
fn format_rewrite() -> io::Result<()> {
    let current_dir = env::current_dir()?;

    let mut input_str = String::new();
    let mut rewrite_str = String::new();

    let mut zip_path = current_dir.clone();
    zip_path.extend(&["test_data", "formats0.zip"]);

    let mut log_path = current_dir.clone();
    log_path.extend(&["actual", "formats0.log"]);
    let mut log_file = fs::File::create(log_path)?;

    let mut zip = zip::read::ZipArchive::new(fs::File::open(zip_path)?)?;
    for zip_index in 0..zip.len() {
        // Read format file from zip.
        let mut zip_file = zip.by_index(zip_index)?;
        let zip_filename = zip_file
            .name()
            .split(&['/', '\\'])
            .last()
            .unwrap_or("")
            .to_string();
        let mut zip_filename_parts = zip_filename.split(' ');
        let system_name = zip_filename_parts.next().unwrap_or("");
        let event_name = zip_filename_parts.next().unwrap_or("");
        input_str.clear();
        zip_file.read_to_string(&mut input_str)?;

        // Parse format file.
        let format = td::PerfEventFormat::parse(false, &system_name, &input_str).unwrap();
        assert_eq!(format.system_name(), system_name);
        assert_eq!(format.name(), event_name);

        // Verify that the rewritten format file is the same as the original.
        // This is overly-strict (some deviations would be acceptable), but at present there
        // aren't any deviations.
        rewrite_str.clear();
        format.write_to(&mut rewrite_str).unwrap();
        assert_eq!(input_str, rewrite_str);

        // Log the analyzed type information so it can be compared with expected output.
        writeln!(log_file, "{}", zip_filename)?;
        writeln!(
            log_file,
            "  sys={}, nam={}, id={}, cfc={}, cfs={} ds={}",
            format.system_name(),
            format.name(),
            format.id(),
            format.common_field_count(),
            format.common_fields_size(),
            format.decoding_style(),
        )?;
        for field in format.fields() {
            writeln!(
                log_file,
                "  {}: \"{}\" {} {} {}",
                field.name(),
                field.field(),
                field.offset(),
                field.size(),
                if let Some(signed) = field.signed() {
                    if signed {
                        "signed"
                    } else {
                        "unsigned"
                    }
                } else {
                    "default"
                }
            )?;
            writeln!(
                log_file,
                "  - array: {} raw={} deduced={}",
                field.array(),
                field.specified_array_count(),
                field.deduced_array_count(),
            )?;
            writeln!(
                log_file,
                "  - enc: raw={}/{} deduced={}/{}",
                field.specified_encoding(),
                field.specified_format(),
                field.deduced_encoding(),
                field.deduced_format(),
            )?;
            writeln!(
                log_file,
                "  - element: size={} shift={}",
                field.element_size(),
                field.element_size_shift(),
            )?;
        }
    }

    return Ok(());
}
