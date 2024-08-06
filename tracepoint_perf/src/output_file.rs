// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

use core::mem;
use core::slice;

use std::fs;
use std::io;
use std::io::Seek;
use std::io::Write;

#[derive(Debug)]
pub(crate) struct OutputFile {
    inner: fs::File,
    inner_pos: u64,
}

impl OutputFile {
    pub fn new(path: &str) -> io::Result<Self> {
        let mut options = fs::OpenOptions::new();
        options.create(true);
        options.truncate(true);
        options.write(true);

        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_SHARE_READ: u32 = 0x00000001;
            const FILE_SHARE_DELETE: u32 = 0x00000004;
            options.share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE);
        }

        return Ok(Self {
            inner: options.open(path)?,
            inner_pos: 0,
        });
    }

    pub fn pos(&self) -> u64 {
        self.inner_pos
    }

    pub fn flush(&mut self) -> io::Result<()> {
        return self.inner.flush();
    }

    pub fn seek_absolute(&mut self, new_pos: u64) -> io::Result<u64> {
        self.inner_pos = self.inner.seek(io::SeekFrom::Start(new_pos))?;
        return Ok(self.inner_pos);
    }

    pub fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        self.inner.write_all(data)?;
        self.inner_pos += data.len() as u64;
        return Ok(());
    }

    pub fn write_vectored(&mut self, bufs: &[io::IoSlice]) -> io::Result<usize> {
        let written = self.inner.write_vectored(bufs)?;
        self.inner_pos += written as u64;
        return Ok(written);
    }

    pub fn write_struct<T>(&mut self, value: &T) -> io::Result<()>
    where
        T: Copy, // Proxy for "T is a plain-old-data struct"
    {
        return self.write_all(unsafe {
            slice::from_raw_parts(value as *const T as *const u8, mem::size_of::<T>())
        });
    }
}
