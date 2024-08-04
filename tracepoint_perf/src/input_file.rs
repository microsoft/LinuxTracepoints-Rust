// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

extern crate alloc;

use alloc::vec;
use core::mem;
use core::slice;

use std::fs;
use std::io;
use std::io::Read;
use std::io::Seek;

#[derive(Debug)]
pub(crate) struct InputFile {
    inner: fs::File,
    inner_pos: u64,
    inner_len: u64,
}

impl InputFile {
    pub fn new(file: fs::File) -> Self {
        Self {
            inner: file,
            inner_pos: 0,
            inner_len: 0,
        }
    }

    pub fn len(&self) -> u64 {
        self.inner_len
    }

    pub fn pos(&self) -> u64 {
        self.inner_pos
    }

    pub fn update_len(&mut self) -> io::Result<()> {
        self.inner_len = self.inner.metadata()?.len();
        return Ok(());
    }

    pub fn seek_absolute(&mut self, new_pos: u64) -> io::Result<u64> {
        if new_pos == self.inner_pos {
            return Ok(new_pos);
        } else if new_pos > self.inner_len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "seek past end of file",
            ));
        } else {
            self.inner_pos = new_pos;
            return self.inner.seek(io::SeekFrom::Start(new_pos));
        }
    }

    pub fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.inner_pos += buf.len() as u64;
        return self.inner.read_exact(buf);
    }

    pub fn read_struct<T>(&mut self, value: &mut T) -> io::Result<()>
    where
        T: Copy, // Proxy for "T is a plain-old-data struct"
    {
        // Safety: Turning struct into slice-of-byte.
        return self.read_exact(unsafe {
            slice::from_raw_parts_mut(value as *mut T as *mut u8, mem::size_of::<T>())
        });
    }

    pub fn read_assign_vec(&mut self, vec: &mut vec::Vec<u8>, len: usize) -> io::Result<()> {
        vec.clear();
        return self.read_append_vec(vec, len);
    }

    pub fn read_append_vec(&mut self, vec: &mut vec::Vec<u8>, len: usize) -> io::Result<()> {
        let old_len = vec.len();
        let new_len = old_len + len;
        vec.reserve(len);

        unsafe {
            // Safety: We've just reserved space for the new data, so it's safe to write into it.
            let vec_bytes = slice::from_raw_parts_mut(vec.as_mut_ptr(), vec.len());

            let result = self.read_exact(&mut vec_bytes[old_len..]);
            if result.is_ok() {
                // Safety: We've read into the buffer so mark it valid.
                vec.set_len(new_len);
            }

            return result;
        }
    }
}
