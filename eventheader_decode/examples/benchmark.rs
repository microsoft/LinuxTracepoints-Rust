use ed::_internal as edi;
use eventheader_decode as ed;
use std::fmt;
use std::string;
use std::time;

const ITERATIONS: usize = 5000;

#[inline(never)]
fn impl_1(f: &mut impl fmt::Write, bytes: &[u8]) {
    let mut writer = edi::TextWriter::new(f);
    for _ in 0..ITERATIONS {
        let _ = writer.write_latin1(bytes);
    }
}

#[inline(never)]
fn impl_2(f: &mut impl fmt::Write, bytes: &[u8]) {
    let mut writer = edi::TextWriter::new(f);
    for _ in 0..ITERATIONS {
        let _ = writer.write_utf8_with_latin1_fallback(bytes);
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
