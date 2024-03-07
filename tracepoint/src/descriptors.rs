// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#[allow(unused_imports)]
use crate::native::TracepointState; // For docs

use core::marker::PhantomData;
use core::mem;

/// Low-level API: Describes a block of data to be sent to user_events via
/// [`TracepointState::write`].
///
/// Note: This must have the same underlying representation as `struct iovec`.
#[repr(C)]
#[derive(Debug, Default)]
pub struct EventDataDescriptor<'a> {
    ptr: usize,
    size: usize,
    lifetime: PhantomData<&'a [u8]>,
}

impl<'a> EventDataDescriptor<'a> {
    /// Returns an EventDataDescriptor initialized with { null, 0 }.
    pub const fn zero() -> Self {
        return Self {
            ptr: 0,
            size: 0,
            lifetime: PhantomData,
        };
    }

    /// Returns true if this descriptor's size is 0.
    pub const fn is_empty(&self) -> bool {
        return self.size == 0;
    }

    /// Returns an EventDataDescriptor initialized with the specified ptr and size.
    ///
    /// # Safety
    ///
    /// This bypasses lifetime tracking. Caller must ensure that this
    /// EventDataDescriptor is not used after the referenced data's lifetime.
    /// Typically, this is done by overwriting the descriptor with
    /// [`EventDataDescriptor::zero`] after it has been used.
    pub const unsafe fn from_raw_ptr(ptr: usize, size: usize) -> Self {
        return Self {
            ptr,
            size,
            lifetime: PhantomData,
        };
    }

    /// Returns an EventDataDescriptor initialized with the specified slice's bytes.
    pub fn from_bytes(value: &'a [u8]) -> Self {
        return Self {
            ptr: value.as_ptr() as usize,
            size: value.len(),
            lifetime: PhantomData,
        };
    }

    /// Returns an EventDataDescriptor initialized with the specified value's bytes.
    pub fn from_value<T: Copy>(value: &'a T) -> Self {
        return Self {
            ptr: value as *const T as usize,
            size: mem::size_of::<T>(),
            lifetime: PhantomData,
        };
    }

    /// Returns an EventDataDescriptor for a nul-terminated string.
    /// Returned descriptor does NOT include the nul-termination.
    ///
    /// Resulting descriptor's size is the minimum of:
    /// - `size_of::<T>() * 65535`
    /// - `size_of::<T>() * value.len()`
    /// - `size_of::<T>() * (index of first element equal to T::default())`
    pub fn from_cstr<T: Copy + Default + Eq>(mut value: &'a [T]) -> Self {
        let mut value_len = value.len();

        const MAX_LEN: usize = 65535;
        if value_len > MAX_LEN {
            value = &value[..MAX_LEN];
            value_len = value.len();
        }

        let zero = T::default();
        let mut len = 0;
        while len < value_len {
            if value[len] == zero {
                value = &value[..len];
                break;
            }

            len += 1;
        }

        return Self {
            ptr: value.as_ptr() as usize,
            size: mem::size_of_val(value),
            lifetime: PhantomData,
        };
    }

    /// Returns an EventDataDescriptor for a variable-length array field.
    ///
    /// Resulting descriptor's size is the minimum of:
    /// - `size_of::<T>() * 65535`
    /// - `size_of::<T>() * value.len()`
    pub fn from_slice<T: Copy>(mut value: &'a [T]) -> Self {
        let value_len = value.len();

        const MAX_LEN: usize = 65535;
        if MAX_LEN < value_len {
            value = &value[..MAX_LEN];
        }

        return Self {
            ptr: value.as_ptr() as usize,
            size: mem::size_of_val(value),
            lifetime: PhantomData,
        };
    }
}
