use std::fmt;
use std::string;
use std::time;
use td::_internal as tdi;
use tracepoint_decode as td;

const ITERATIONS: usize = 5000;

#[inline(never)]
fn impl_1(f: &mut impl fmt::Write, bytes: &[u8]) {
    let mut writer = tdi::ValueWriter::new(f, td::PerfConvertOptions::Default);
    for _ in 0..ITERATIONS {
        let _ = writer.write_latin1_with_no_filter(bytes);
    }
}

#[inline(never)]
fn impl_2(f: &mut impl fmt::Write, bytes: &[u8]) {
    let mut writer = tdi::ValueWriter::new(f, td::PerfConvertOptions::Default);
    for _ in 0..ITERATIONS {
        let _ = writer.write_utf8_with_no_filter(bytes);
    }
}

fn main() {
    let mut buf = string::String::with_capacity(4096);
    let bytes = [0u8; 64];

    buf.clear();
    impl_1(&mut buf, &bytes);
    buf.clear();
    impl_2(&mut buf, &bytes);

    let start1 = time::Instant::now();
    for _ in 0..ITERATIONS {
        buf.clear();
        impl_1(&mut buf, &bytes);
    }
    let end1 = time::Instant::now();

    let start2 = time::Instant::now();
    for _ in 0..ITERATIONS {
        buf.clear();
        impl_2(&mut buf, &bytes);
    }
    let end2 = time::Instant::now();

    println!("impl_1: {:?}", end1 - start1);
    println!("impl_2: {:?}", end2 - start2);
}
