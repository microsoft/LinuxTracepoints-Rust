// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

use core::fmt;
use core::fmt::Write;
use core::str;

use eventheader_types::Guid;

use crate::charconv;
use crate::filters::*;
use crate::EventHeaderItemInfo;
use crate::PerfConvertOptions;

#[cfg(all(windows, feature = "decode_date"))]
mod date_time {
    #[repr(C)]
    pub struct DateTime {
        year: u16,
        month_of_year: u16,
        day_of_week: u16,
        day_of_month: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }

    impl DateTime {
        pub fn new(value: i64) -> Self {
            let mut this = Self {
                year: 0,
                month_of_year: 0,
                day_of_week: 0,
                day_of_month: 0,
                hour: 0,
                minute: 0,
                second: 0,
                milliseconds: 0,
            };

            if (-11644473600..=910692730085).contains(&value) {
                let ft = (value + 11644473600) * 10000000;
                if 0 == unsafe { FileTimeToSystemTime(&ft, &mut this) } {
                    this.month_of_year = 0;
                }
            }

            return this;
        }

        pub const fn valid(&self) -> bool {
            self.month_of_year != 0
        }

        pub const fn year(&self) -> u32 {
            self.year as u32
        }

        pub const fn month_of_year(&self) -> u8 {
            self.month_of_year as u8
        }

        pub const fn day_of_month(&self) -> u8 {
            self.day_of_month as u8
        }

        pub const fn hour(&self) -> u8 {
            self.hour as u8
        }

        pub const fn minute(&self) -> u8 {
            self.minute as u8
        }

        pub const fn second(&self) -> u8 {
            self.second as u8
        }
    }

    extern "system" {
        fn FileTimeToSystemTime(file_time: *const i64, system_time: *mut DateTime) -> i32;
    }
}

#[cfg(all(unix, feature = "decode_date"))]
mod date_time {
    pub struct DateTime {
        tm: libc::tm,
    }

    impl DateTime {
        pub fn new(value: i64) -> Self {
            let mut this = Self {
                tm: unsafe { core::mem::zeroed() },
            };

            if unsafe { core::ptr::null() == libc::gmtime_r(&value, &mut this.tm) } {
                this.tm.tm_mday = 0;
            }

            return this;
        }

        pub const fn valid(&self) -> bool {
            self.tm.tm_mday != 0
        }

        pub const fn year(&self) -> u32 {
            self.tm.tm_year.wrapping_add(1900) as u32
        }

        pub const fn month_of_year(&self) -> u8 {
            self.tm.tm_mon as u8 + 1
        }

        pub const fn day_of_month(&self) -> u8 {
            self.tm.tm_mday as u8
        }

        pub const fn hour(&self) -> u8 {
            self.tm.tm_hour as u8
        }

        pub const fn minute(&self) -> u8 {
            self.tm.tm_min as u8
        }

        pub const fn second(&self) -> u8 {
            self.tm.tm_sec as u8
        }
    }
}

#[cfg(not(any(
    all(windows, feature = "decode_date"),
    all(unix, feature = "decode_date")
)))]
mod date_time {
    pub struct DateTime {}

    impl DateTime {
        pub const fn new(_value: i64) -> Self {
            Self {}
        }

        pub const fn valid(&self) -> bool {
            false
        }

        pub const fn year(&self) -> u32 {
            0
        }

        pub const fn month_of_year(&self) -> u8 {
            0
        }

        pub const fn day_of_month(&self) -> u8 {
            0
        }

        pub const fn hour(&self) -> u8 {
            0
        }

        pub const fn minute(&self) -> u8 {
            0
        }

        pub const fn second(&self) -> u8 {
            0
        }
    }
}

/// Writes JSON values to a `fmt::Write` destination.
pub struct JsonWriter<'wri, W: fmt::Write>(ValueWriter<'wri, W>);

impl<'wri, W: fmt::Write> JsonWriter<'wri, W> {
    /// Creates a new `JsonWriter` with the specified destination and options.
    /// `json_comma` specifies whether a comma should be written before the first JSON item.
    pub fn new(
        writer: &'wri mut W,
        options: PerfConvertOptions,
        json_comma: bool,
    ) -> JsonWriter<'wri, W> {
        JsonWriter(ValueWriter {
            dest: WriteFilter::<'wri, W>::new(writer),
            options,
            json_comma,
            json_space: json_comma && options.has(PerfConvertOptions::Space),
        })
    }

    /// Returns true if a comma will need to be written before the next JSON item.
    ///
    /// This is true after `}`, `]`, and after writing a value or member.
    ///
    /// This is false after `{`, `[`, `json_newline_before_value`, and `json_property_name`.
    pub fn comma(&self) -> bool {
        self.0.json_comma
    }

    /// For use before a value or member.
    /// Writes: comma?-newline-indent? i.e. `,\n  `.
    pub fn write_newline_before_value(&mut self, indent: usize) -> fmt::Result {
        if self.0.json_comma {
            self.0.dest.write_ascii(b',')?;
        }

        if cfg!(windows) {
            self.0.dest.write_str("\r\n")?;
        } else {
            self.0.dest.write_ascii(b'\n')?;
        }

        self.0.json_comma = false;

        if self.0.json_space {
            for _ in 0..indent {
                self.0.dest.write_ascii(b' ')?;
            }
        }
        self.0.json_space = false;

        return Ok(());
    }

    /// Writes: `, "escaped-name":`
    pub fn write_property_name(&mut self, name: &str) -> fmt::Result {
        self.write_raw_comma_space()?;
        self.0.json_comma = false;

        self.0.dest.write_ascii(b'"')?;
        JsonEscapeFilter::new(&mut self.0.dest).write_str(name)?;
        return self.0.dest.write_str("\":");
    }

    /// Writes: `, "name":` (assumes it is already escaped for JSON).
    pub fn write_property_name_json_safe(&mut self, name: &str) -> fmt::Result {
        self.write_raw_comma_space()?;
        self.0.json_comma = false;

        self.0.dest.write_ascii(b'"')?;
        self.0.dest.write_str(name)?;
        return self.0.dest.write_str("\":");
    }

    /// If yes tag, writes: `, "escaped-name;tag=0xTAG":`.
    ///
    /// If no tag, writes: `, "escaped-name":`.
    pub fn write_property_name_from_item_info(
        &mut self,
        item_info: &EventHeaderItemInfo,
    ) -> fmt::Result {
        self.write_raw_comma_space()?;
        self.0.json_comma = false;

        self.0.dest.write_ascii(b'"')?;
        self.0.write_utf8_with_json_escape(item_info.name_bytes())?;
        if self.0.options.has(PerfConvertOptions::FieldTag) {
            let tag = item_info.metadata().field_tag();
            if tag != 0 {
                write!(self.0.dest, ";tag=0x{:X}", tag)?;
            }
        }

        return self.0.dest.write_str("\":");
    }

    /// Writes: `, {`
    pub fn write_object_begin(&mut self) -> fmt::Result {
        self.write_raw_comma_space()?;
        self.0.json_comma = false;

        return self.0.dest.write_ascii(b'{');
    }

    /// Writes: ` }`
    pub fn write_object_end(&mut self) -> fmt::Result {
        self.0.json_comma = true;
        if self.0.json_space {
            self.0.dest.write_ascii(b' ')?;
        }
        return self.0.dest.write_ascii(b'}');
    }

    /// Writes: `, [`
    pub fn write_array_begin(&mut self) -> fmt::Result {
        self.write_raw_comma_space()?;
        self.0.json_comma = false;

        return self.0.dest.write_ascii(b'[');
    }

    /// Writes: ` ]`
    pub fn write_array_end(&mut self) -> fmt::Result {
        self.0.json_comma = true;
        if self.0.json_space {
            self.0.dest.write_ascii(b' ')?;
        }
        return self.0.dest.write_ascii(b']');
    }

    /// Writes leading comma/space if needed,
    /// then invokes `f` to write the value.
    pub fn write_value<F, R>(&mut self, f: F) -> Result<R, fmt::Error>
    where
        F: FnOnce(&mut ValueWriter<'wri, W>) -> Result<R, fmt::Error>,
    {
        self.write_raw_comma_space()?;
        self.0.json_comma = true;

        return f(&mut self.0);
    }

    /// Writes leading comma/space if needed, then writes `"`,
    /// then invokes `f` to write the value, then writes `"`.
    pub fn write_value_quoted<F, R>(&mut self, f: F) -> Result<R, fmt::Error>
    where
        F: FnOnce(&mut ValueWriter<'wri, W>) -> Result<R, fmt::Error>,
    {
        self.write_raw_comma_space()?;
        self.0.json_comma = true;

        return self.0.write_quoted(f);
    }

    /// Writes comma and space as needed.
    /// Updates `json_space`. Does NOT update `json_comma`.
    fn write_raw_comma_space(&mut self) -> fmt::Result {
        if self.0.json_space {
            self.0.json_space = self.0.options.has(PerfConvertOptions::Space);
            if self.0.json_comma {
                return self.0.dest.write_str(", ");
            } else {
                return self.0.dest.write_ascii(b' ');
            }
        } else {
            self.0.json_space = self.0.options.has(PerfConvertOptions::Space);
            if self.0.json_comma {
                return self.0.dest.write_ascii(b',');
            } else {
                return Ok(());
            }
        }
    }
}

/// Writes values to a `fmt::Write` destination.
pub struct ValueWriter<'wri, W: fmt::Write> {
    dest: WriteFilter<'wri, W>,
    options: PerfConvertOptions,

    // The following fields are used only when this is part of a `JsonWriter`.
    // They are stored in the `ValueWriter` because they are in space that
    // would otherwise be padding.
    /// Should the next JSON item be preceded by a comma?
    /// e.g. true after '{', false after '}'.
    json_comma: bool,

    /// Should the next JSON item be preceded by a space?
    json_space: bool,
}

impl<'wri, W: fmt::Write> ValueWriter<'wri, W> {
    const ERRNO_STRINGS: [&'static str; 134] = [
        "ERRNO(0)",
        "EPERM(1)",
        "ENOENT(2)",
        "ESRCH(3)",
        "EINTR(4)",
        "EIO(5)",
        "ENXIO(6)",
        "E2BIG(7)",
        "ENOEXEC(8)",
        "EBADF(9)",
        "ECHILD(10)",
        "EAGAIN(11)",
        "ENOMEM(12)",
        "EACCES(13)",
        "EFAULT(14)",
        "ENOTBLK(15)",
        "EBUSY(16)",
        "EEXIST(17)",
        "EXDEV(18)",
        "ENODEV(19)",
        "ENOTDIR(20)",
        "EISDIR(21)",
        "EINVAL(22)",
        "ENFILE(23)",
        "EMFILE(24)",
        "ENOTTY(25)",
        "ETXTBSY(26)",
        "EFBIG(27)",
        "ENOSPC(28)",
        "ESPIPE(29)",
        "EROFS(30)",
        "EMLINK(31)",
        "EPIPE(32)",
        "EDOM(33)",
        "ERANGE(34)",
        "EDEADLK(35)",
        "ENAMETOOLONG(36)",
        "ENOLCK(37)",
        "ENOSYS(38)",
        "ENOTEMPTY(39)",
        "ELOOP(40)",
        "ERRNO(41)",
        "ENOMSG(42)",
        "EIDRM(43)",
        "ECHRNG(44)",
        "EL2NSYNC(45)",
        "EL3HLT(46)",
        "EL3RST(47)",
        "ELNRNG(48)",
        "EUNATCH(49)",
        "ENOCSI(50)",
        "EL2HLT(51)",
        "EBADE(52)",
        "EBADR(53)",
        "EXFULL(54)",
        "ENOANO(55)",
        "EBADRQC(56)",
        "EBADSLT(57)",
        "ERRNO(58)",
        "EBFONT(59)",
        "ENOSTR(60)",
        "ENODATA(61)",
        "ETIME(62)",
        "ENOSR(63)",
        "ENONET(64)",
        "ENOPKG(65)",
        "EREMOTE(66)",
        "ENOLINK(67)",
        "EADV(68)",
        "ESRMNT(69)",
        "ECOMM(70)",
        "EPROTO(71)",
        "EMULTIHOP(72)",
        "EDOTDOT(73)",
        "EBADMSG(74)",
        "EOVERFLOW(75)",
        "ENOTUNIQ(76)",
        "EBADFD(77)",
        "EREMCHG(78)",
        "ELIBACC(79)",
        "ELIBBAD(80)",
        "ELIBSCN(81)",
        "ELIBMAX(82)",
        "ELIBEXEC(83)",
        "EILSEQ(84)",
        "ERESTART(85)",
        "ESTRPIPE(86)",
        "EUSERS(87)",
        "ENOTSOCK(88)",
        "EDESTADDRREQ(89)",
        "EMSGSIZE(90)",
        "EPROTOTYPE(91)",
        "ENOPROTOOPT(92)",
        "EPROTONOSUPPORT(93)",
        "ESOCKTNOSUPPORT(94)",
        "EOPNOTSUPP(95)",
        "EPFNOSUPPORT(96)",
        "EAFNOSUPPORT(97)",
        "EADDRINUSE(98)",
        "EADDRNOTAVAIL(99)",
        "ENETDOWN(100)",
        "ENETUNREACH(101)",
        "ENETRESET(102)",
        "ECONNABORTED(103)",
        "ECONNRESET(104)",
        "ENOBUFS(105)",
        "EISCONN(106)",
        "ENOTCONN(107)",
        "ESHUTDOWN(108)",
        "ETOOMANYREFS(109)",
        "ETIMEDOUT(110)",
        "ECONNREFUSED(111)",
        "EHOSTDOWN(112)",
        "EHOSTUNREACH(113)",
        "EALREADY(114)",
        "EINPROGRESS(115)",
        "ESTALE(116)",
        "EUCLEAN(117)",
        "ENOTNAM(118)",
        "ENAVAIL(119)",
        "EISNAM(120)",
        "EREMOTEIO(121)",
        "EDQUOT(122)",
        "ENOMEDIUM(123)",
        "EMEDIUMTYPE(124)",
        "ECANCELED(125)",
        "ENOKEY(126)",
        "EKEYEXPIRED(127)",
        "EKEYREVOKED(128)",
        "EKEYREJECTED(129)",
        "EOWNERDEAD(130)",
        "ENOTRECOVERABLE(131)",
        "ERFKILL(132)",
        "EHWPOISON(133)",
    ];

    /// Creates a new `ValueWriter` with the specified destination and options.
    pub fn new(writer: &'wri mut W, options: PerfConvertOptions) -> ValueWriter<'wri, W> {
        ValueWriter {
            dest: WriteFilter::<'wri, W>::new(writer),
            options,
            json_comma: false,
            json_space: false,
        }
    }

    /// Writes `"`, then invokes f, then writes `"`.
    pub fn write_quoted<F, R>(&mut self, f: F) -> Result<R, fmt::Error>
    where
        F: FnOnce(&mut ValueWriter<'wri, W>) -> Result<R, fmt::Error>,
    {
        self.dest.write_ascii(b'"')?;
        let result = f(self)?;
        self.dest.write_ascii(b'"')?;
        return Ok(result);
    }

    /// Writes a string with no filtering of control characters.
    pub fn write_str_with_no_filter(&mut self, value: &str) -> fmt::Result {
        self.dest.write_str(value)
    }

    /// Writes a string with JSON filtering of control/punctuation characters.
    pub fn write_str_with_json_escape(&mut self, value: &str) -> fmt::Result {
        JsonEscapeFilter::new(&mut self.dest).write_str(value)
    }

    /// Writes a string with no filtering of control characters.
    pub fn write_fmt_with_no_filter(&mut self, args: fmt::Arguments) -> fmt::Result {
        self.dest.write_fmt(args)
    }

    /// Writes string from Latin-1 bytes with no filtering of control characters.
    pub fn write_latin1_with_no_filter(&mut self, bytes: &[u8]) -> fmt::Result {
        charconv::write_latin1_to(bytes, &mut self.dest)
    }

    /// Writes string from Latin-1 bytes with JSON filtering of control/punctuation characters.
    pub fn write_latin1_with_json_escape(&mut self, bytes: &[u8]) -> fmt::Result {
        charconv::write_latin1_to(bytes, &mut JsonEscapeFilter::new(&mut self.dest))
    }

    /// Writes string from Latin-1 bytes with filtering of control characters as specified by
    /// the [`PerfConvertOptions::StringControlCharsMask`] flags in `options`.
    pub fn write_latin1_with_control_chars_filter(&mut self, bytes: &[u8]) -> fmt::Result {
        match self.options.and(PerfConvertOptions::StringControlCharsMask) {
            PerfConvertOptions::StringControlCharsReplaceWithSpace => {
                charconv::write_latin1_to(bytes, &mut ControlCharsSpaceFilter::new(&mut self.dest))
            }
            PerfConvertOptions::StringControlCharsJsonEscape => {
                charconv::write_latin1_to(bytes, &mut ControlCharsJsonFilter::new(&mut self.dest))
            }
            _ => self.write_latin1_with_no_filter(bytes),
        }
    }

    /// Writes string from UTF-8 (with Latin-1 fallback) bytes with no filtering of control characters.
    pub fn write_utf8_with_no_filter(&mut self, bytes: &[u8]) -> fmt::Result {
        charconv::write_utf8_with_latin1_fallback_to(bytes, &mut self.dest)
    }

    /// Writes string from UTF-8 (with Latin-1 fallback) bytes with JSON filtering of control/punctuation characters.
    pub fn write_utf8_with_json_escape(&mut self, bytes: &[u8]) -> fmt::Result {
        charconv::write_utf8_with_latin1_fallback_to(
            bytes,
            &mut JsonEscapeFilter::new(&mut self.dest),
        )
    }

    /// Writes string from UTF-8 (with Latin-1 fallback) bytes with filtering of control characters as specified by
    /// the [`PerfConvertOptions::StringControlCharsMask`] flags in `options`.
    pub fn write_utf8_with_control_chars_filter(&mut self, bytes: &[u8]) -> fmt::Result {
        match self.options.and(PerfConvertOptions::StringControlCharsMask) {
            PerfConvertOptions::StringControlCharsReplaceWithSpace => {
                charconv::write_utf8_with_latin1_fallback_to(
                    bytes,
                    &mut ControlCharsSpaceFilter::new(&mut self.dest),
                )
            }
            PerfConvertOptions::StringControlCharsJsonEscape => {
                charconv::write_utf8_with_latin1_fallback_to(
                    bytes,
                    &mut ControlCharsJsonFilter::new(&mut self.dest),
                )
            }
            _ => self.write_utf8_with_no_filter(bytes),
        }
    }

    /// Writes string from UTF-16BE bytes with no filtering of control characters.
    pub fn write_utf16be_with_no_filter(&mut self, bytes: &[u8]) -> fmt::Result {
        charconv::write_utf16be_to(bytes, &mut self.dest)
    }

    /// Writes string from UTF-16BE bytes with JSON filtering of control/punctuation characters.
    pub fn write_utf16be_with_json_escape(&mut self, bytes: &[u8]) -> fmt::Result {
        charconv::write_utf16be_to(bytes, &mut JsonEscapeFilter::new(&mut self.dest))
    }

    /// Writes string from UTF-16BE bytes with filtering of control characters as specified by
    /// the [`PerfConvertOptions::StringControlCharsMask`] flags in `options`.
    pub fn write_utf16be_with_control_chars_filter(&mut self, bytes: &[u8]) -> fmt::Result {
        match self.options.and(PerfConvertOptions::StringControlCharsMask) {
            PerfConvertOptions::StringControlCharsReplaceWithSpace => {
                charconv::write_utf16be_to(bytes, &mut ControlCharsSpaceFilter::new(&mut self.dest))
            }
            PerfConvertOptions::StringControlCharsJsonEscape => {
                charconv::write_utf16be_to(bytes, &mut ControlCharsJsonFilter::new(&mut self.dest))
            }
            _ => self.write_utf16be_with_no_filter(bytes),
        }
    }

    /// Writes string from UTF-16LE bytes with no filtering of control characters.
    pub fn write_utf16le_with_no_filter(&mut self, bytes: &[u8]) -> fmt::Result {
        charconv::write_utf16le_to(bytes, &mut self.dest)
    }

    /// Writes string from UTF-16LE bytes with JSON filtering of control/punctuation characters.
    pub fn write_utf16le_with_json_escape(&mut self, bytes: &[u8]) -> fmt::Result {
        charconv::write_utf16le_to(bytes, &mut JsonEscapeFilter::new(&mut self.dest))
    }

    /// Writes string from UTF-16LE bytes with filtering of control characters as specified by
    /// the [`PerfConvertOptions::StringControlCharsMask`] flags in `options`.
    pub fn write_utf16le_with_control_chars_filter(&mut self, bytes: &[u8]) -> fmt::Result {
        match self.options.and(PerfConvertOptions::StringControlCharsMask) {
            PerfConvertOptions::StringControlCharsReplaceWithSpace => {
                charconv::write_utf16le_to(bytes, &mut ControlCharsSpaceFilter::new(&mut self.dest))
            }
            PerfConvertOptions::StringControlCharsJsonEscape => {
                charconv::write_utf16le_to(bytes, &mut ControlCharsJsonFilter::new(&mut self.dest))
            }
            _ => self.write_utf16le_with_no_filter(bytes),
        }
    }

    /// Writes string from UTF-32BE bytes with no filtering of control characters.
    pub fn write_utf32be_with_no_filter(&mut self, bytes: &[u8]) -> fmt::Result {
        charconv::write_utf32be_to(bytes, &mut self.dest)
    }

    /// Writes string from UTF-32BE bytes with JSON filtering of control/punctuation characters.
    pub fn write_utf32be_with_json_escape(&mut self, bytes: &[u8]) -> fmt::Result {
        charconv::write_utf32be_to(bytes, &mut JsonEscapeFilter::new(&mut self.dest))
    }

    /// Writes string from UTF-32BE bytes with filtering of control characters as specified by
    /// the [`PerfConvertOptions::StringControlCharsMask`] flags in `options`.
    pub fn write_utf32be_with_control_chars_filter(&mut self, bytes: &[u8]) -> fmt::Result {
        match self.options.and(PerfConvertOptions::StringControlCharsMask) {
            PerfConvertOptions::StringControlCharsReplaceWithSpace => {
                charconv::write_utf32be_to(bytes, &mut ControlCharsSpaceFilter::new(&mut self.dest))
            }
            PerfConvertOptions::StringControlCharsJsonEscape => {
                charconv::write_utf32be_to(bytes, &mut ControlCharsJsonFilter::new(&mut self.dest))
            }
            _ => self.write_utf32be_with_no_filter(bytes),
        }
    }

    /// Writes string from UTF-32LE bytes with no filtering of control characters.
    pub fn write_utf32le_with_no_filter(&mut self, bytes: &[u8]) -> fmt::Result {
        charconv::write_utf32le_to(bytes, &mut self.dest)
    }

    /// Writes string from UTF-32LE bytes with JSON filtering of control/punctuation characters.
    pub fn write_utf32le_with_json_escape(&mut self, bytes: &[u8]) -> fmt::Result {
        charconv::write_utf32le_to(bytes, &mut JsonEscapeFilter::new(&mut self.dest))
    }

    /// Writes string from UTF-32LE bytes with filtering of control characters as specified by
    /// the [`PerfConvertOptions::StringControlCharsMask`] flags in `options`.
    pub fn write_utf32le_with_control_chars_filter(&mut self, bytes: &[u8]) -> fmt::Result {
        match self.options.and(PerfConvertOptions::StringControlCharsMask) {
            PerfConvertOptions::StringControlCharsReplaceWithSpace => {
                charconv::write_utf32le_to(bytes, &mut ControlCharsSpaceFilter::new(&mut self.dest))
            }
            PerfConvertOptions::StringControlCharsJsonEscape => {
                charconv::write_utf32le_to(bytes, &mut ControlCharsJsonFilter::new(&mut self.dest))
            }
            _ => self.write_utf32le_with_no_filter(bytes),
        }
    }

    /// If `value` is a control char, write it respecting [`PerfConvertOptions::StringControlCharsMask`].
    /// Otherwise, if `value` is a valid Unicode code point, write it.
    /// Otherwise, write the replacement character.
    pub fn write_char32_with_control_chars_filter(&mut self, value: u32) -> fmt::Result {
        if value >= 0x20 {
            self.dest.write_char(charconv::char_from_u32(value))
        } else {
            match self.options.and(PerfConvertOptions::StringControlCharsMask) {
                PerfConvertOptions::StringControlCharsReplaceWithSpace => {
                    self.dest.write_ascii(b' ')
                }
                PerfConvertOptions::StringControlCharsJsonEscape => {
                    ControlCharsJsonFilter::new(&mut self.dest).write_ascii(value as u8)
                }
                _ => self.dest.write_ascii(value as u8),
            }
        }
    }

    /// Otherwise, if `value` is a valid Unicode code point, write it with JSON escape.
    /// Otherwise, write the replacement character.
    pub fn write_char32_with_json_escape(&mut self, value: u32) -> fmt::Result {
        if value >= ('\\' as u32) {
            self.dest.write_char(charconv::char_from_u32(value))
        } else {
            JsonEscapeFilter::new(&mut self.dest).write_ascii(value as u8)
        }
    }

    /// Writes e.g. `a3a2a1a0-b1b0-c1c0-d7d6-d5d4d3d2d1d0`.
    pub fn write_uuid(&mut self, value: &[u8; 16]) -> fmt::Result {
        self.dest.write_str(unsafe {
            str::from_utf8_unchecked(&Guid::from_bytes_be(value).to_utf8_bytes())
        })
    }

    /// Writes e.g. `01 1f f0`.
    pub fn write_hexbytes(&mut self, bytes: &[u8]) -> fmt::Result {
        if !bytes.is_empty() {
            write!(self.dest, "{:02x}", bytes[0])?;
            for b in bytes.iter().skip(1) {
                write!(self.dest, " {:02x}", b)?;
            }
        }
        return Ok(());
    }

    /// Writes any [`fmt::Display`] using `{}` formatting.
    pub fn write_display_with_no_filter<D: fmt::Display>(&mut self, value: D) -> fmt::Result {
        write!(self.dest, "{}", value)
    }

    /// Writes hex integer e.g. `0x1FF`.
    pub fn write_hex32(&mut self, value: u32) -> fmt::Result {
        write!(self.dest, "0x{:X}", value)
    }

    /// Writes hex integer e.g. `0x1FF`.
    pub fn write_hex64(&mut self, value: u64) -> fmt::Result {
        write!(self.dest, "0x{:X}", value)
    }

    /// Writes an IPv4 address, e.g. `127.0.0.1`.
    pub fn write_ipv4(&mut self, value: [u8; 4]) -> fmt::Result {
        write!(
            self.dest,
            "{}.{}.{}.{}",
            value[0], value[1], value[2], value[3]
        )
    }

    /// Writes hex string or decimal, respecting [`PerfConvertOptions::IntHexAsString`],
    /// e.g. `"0xFF"` or `255`.
    pub fn write_json_hex32(&mut self, value: u32) -> fmt::Result {
        if self.options.has(PerfConvertOptions::IntHexAsString) {
            write!(self.dest, "\"0x{:X}\"", value)
        } else {
            write!(self.dest, "{}", value)
        }
    }

    /// Writes hex string or decimal, respecting [`PerfConvertOptions::IntHexAsString`],
    /// e.g. `"0xFF"` or `255`.
    pub fn write_json_hex64(&mut self, value: u64) -> fmt::Result {
        if self.options.has(PerfConvertOptions::IntHexAsString) {
            write!(self.dest, "\"0x{:X}\"", value)
        } else {
            write!(self.dest, "{}", value)
        }
    }

    /// Writes a boolean, respecting [`PerfConvertOptions::BoolOutOfRangeAsString`]. e.g. `true`,
    /// `false`, `BOOL(-12)`, or `-12`. For values other than 0 and 1, the value is treated as a
    /// signed integer, but the parameter is a `u32` because bool8 and bool16 should NOT be
    /// sign-extended.
    pub fn write_bool(&mut self, value: u32) -> fmt::Result {
        match value {
            0 => self.dest.write_str("false"),
            1 => self.dest.write_str("true"),
            _ => {
                if self.options.has(PerfConvertOptions::BoolOutOfRangeAsString) {
                    write!(self.dest, "BOOL({})", value as i32)
                } else {
                    write!(self.dest, "{}", value as i32)
                }
            }
        }
    }

    /// Writes a boolean, respecting [`PerfConvertOptions::BoolOutOfRangeAsString`]. e.g. `true`,
    /// `false`, `"BOOL(-12)"`, or `-12`. For values other than 0 and 1, the value is treated as a
    /// signed integer, but the parameter is a `u32` because bool8 and bool16 should NOT be
    /// sign-extended.
    pub fn write_json_bool(&mut self, value: u32) -> fmt::Result {
        match value {
            0 => self.dest.write_str("false"),
            1 => self.dest.write_str("true"),
            _ => {
                if self.options.has(PerfConvertOptions::BoolOutOfRangeAsString) {
                    write!(self.dest, "\"BOOL({})\"", value as i32)
                } else {
                    write!(self.dest, "{}", value as i32)
                }
            }
        }
    }

    /// Writes an errno, respecting [`PerfConvertOptions::ErrnoUnknownAsString`],
    /// e.g. `ENOENT(2)`, `ERRNO(-12)`, or `-12`.
    pub fn write_errno(&mut self, value: u32) -> fmt::Result {
        if value < Self::ERRNO_STRINGS.len() as u32 {
            self.dest.write_str(Self::ERRNO_STRINGS[value as usize])
        } else if self.options.has(PerfConvertOptions::ErrnoUnknownAsString) {
            write!(self.dest, "ERRNO({})", value as i32)
        } else {
            write!(self.dest, "{}", value as i32)
        }
    }

    /// Writes an errno, respecting [`PerfConvertOptions::ErrnoKnownAsString`] and
    /// [`PerfConvertOptions::ErrnoUnknownAsString`],
    /// e.g. `"ENOENT(2)"`, `"ERRNO(-12)"`, or `-12`.
    pub fn write_json_errno(&mut self, value: u32) -> fmt::Result {
        if value < Self::ERRNO_STRINGS.len() as u32 {
            if self.options.has(PerfConvertOptions::ErrnoKnownAsString) {
                return write!(self.dest, "\"{}\"", Self::ERRNO_STRINGS[value as usize]);
            }
        } else if self.options.has(PerfConvertOptions::ErrnoUnknownAsString) {
            return write!(self.dest, "\"ERRNO({})\"", value as i32);
        }

        return write!(self.dest, "{}", value as i32);
    }

    /// Writes a time64, respecting [`PerfConvertOptions::UnixTimeOutOfRangeAsString`],
    /// e.g. `2020-02-02T02:02:02Z`, `TIME(1234567890)`, or `1234567890`.
    pub fn write_time64(&mut self, value: i64) -> fmt::Result {
        let dt = date_time::DateTime::new(value);
        if dt.valid() {
            return write!(
                self.dest,
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                dt.year(),
                dt.month_of_year(),
                dt.day_of_month(),
                dt.hour(),
                dt.minute(),
                dt.second()
            );
        } else if self
            .options
            .has(PerfConvertOptions::UnixTimeOutOfRangeAsString)
        {
            return write!(self.dest, "TIME({})", value);
        }

        return write!(self.dest, "{}", value);
    }

    /// Writes a JSON time64, respecting [`PerfConvertOptions::UnixTimeOutOfRangeAsString`]
    /// and [`PerfConvertOptions::UnixTimeWithinRangeAsString`],
    /// e.g. `2020-02-02T02:02:02Z`, `TIME(1234567890)`, or `1234567890`.
    pub fn write_json_time64(&mut self, value: i64) -> fmt::Result {
        let dt = date_time::DateTime::new(value);
        if dt.valid() {
            if self
                .options
                .has(PerfConvertOptions::UnixTimeWithinRangeAsString)
            {
                return write!(
                    self.dest,
                    "\"{:04}-{:02}-{:02}T{:02}:{:02}:{:02}\"",
                    dt.year(),
                    dt.month_of_year(),
                    dt.day_of_month(),
                    dt.hour(),
                    dt.minute(),
                    dt.second()
                );
            }
        } else if self
            .options
            .has(PerfConvertOptions::UnixTimeOutOfRangeAsString)
        {
            return write!(self.dest, "\"TIME({})\"", value);
        }

        return write!(self.dest, "{}", value);
    }

    /// Writes an `f32`, respecting [`PerfConvertOptions::FloatExtraPrecision`] flag.
    pub fn write_float32(&mut self, value: f32) -> fmt::Result {
        if self.options.has(PerfConvertOptions::FloatExtraPrecision) {
            write!(self.dest, "{:.9}", value)
        } else {
            write!(self.dest, "{}", value)
        }
    }

    /// Writes an `f32`, respecting [`PerfConvertOptions::FloatExtraPrecision`] and
    /// [`PerfConvertOptions::FloatNonFiniteAsString`] flags.
    pub fn write_json_float32(&mut self, value: f32) -> fmt::Result {
        if value.is_finite() {
            self.write_float32(value)
        } else if self.options.has(PerfConvertOptions::FloatNonFiniteAsString) {
            write!(self.dest, "\"{}\"", value)
        } else {
            self.dest.write_str("null")
        }
    }

    /// Writes an `f64`, respecting [`PerfConvertOptions::FloatExtraPrecision`] flag.
    pub fn write_float64(&mut self, value: f64) -> fmt::Result {
        if self.options.has(PerfConvertOptions::FloatExtraPrecision) {
            write!(self.dest, "{:.17}", value)
        } else {
            write!(self.dest, "{}", value)
        }
    }

    /// Writes an `f64`, respecting [`PerfConvertOptions::FloatExtraPrecision`] and
    /// [`PerfConvertOptions::FloatNonFiniteAsString`] flags.
    pub fn write_json_float64(&mut self, value: f64) -> fmt::Result {
        if value.is_finite() {
            self.write_float64(value)
        } else if self.options.has(PerfConvertOptions::FloatNonFiniteAsString) {
            write!(self.dest, "\"{}\"", value)
        } else {
            self.dest.write_str("null")
        }
    }
}
