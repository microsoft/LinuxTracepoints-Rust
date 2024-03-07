// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

use core::ffi;
use core::marker;
use core::mem::size_of;
use core::pin::Pin;
use core::sync::atomic::AtomicI32;
use core::sync::atomic::AtomicU32;
use core::sync::atomic::Ordering;

use crate::descriptors::EventDataDescriptor;

#[cfg(all(target_os = "linux", feature = "user_events"))]
use libc as linux;

// Note: this is intentionally leaked.
static USER_EVENTS_DATA_FILE: UserEventsDataFile = UserEventsDataFile::new();

/// Requires: an errno-setting operation has failed.
///
/// Returns the current value of `linux::errno`.
/// Debug-asserts that `errno > 0`.
#[cfg(all(target_os = "linux", feature = "user_events"))]
fn get_failure_errno() -> i32 {
    let errno = unsafe { *linux::__errno_location() };
    debug_assert!(errno > 0); // Shouldn't call this unless an errno-based operation failed.
    return errno;
}

/// Sets `linux::errno` to 0.
#[cfg(all(target_os = "linux", feature = "user_events"))]
fn clear_errno() {
    unsafe { *linux::__errno_location() = 0 };
}

/// linux::open(path0, O_WRONLY)
#[cfg(all(target_os = "linux", feature = "user_events"))]
fn open_wronly(path0: &[u8]) -> ffi::c_int {
    assert!(path0.ends_with(&[0]));
    return unsafe { linux::open(path0.as_ptr().cast::<ffi::c_char>(), linux::O_WRONLY) };
}

struct UserEventsDataFile {
    /// Initial value is -EAGAIN.
    /// Negative value is -errno with the error code from failed open.
    /// Non-negative value is file descriptor for the "user_events_data" file.
    file_or_error: AtomicI32,
}

impl UserEventsDataFile {
    const EAGAIN_ERROR: i32 = -11;

    #[cfg(all(target_os = "linux", feature = "user_events"))]
    const fn is_space_char(ch: u8) -> bool {
        return ch == b' ' || ch == b'\t';
    }

    #[cfg(all(target_os = "linux", feature = "user_events"))]
    const fn is_nonspace_char(ch: u8) -> bool {
        return ch != b'\0' && !Self::is_space_char(ch);
    }

    /// Opens a file descriptor to the `user_events_data` file.
    /// Atomically updates `self.file_or_error` to either a negative
    /// value (-errno returned from `linux::open`) or a non-negative value
    /// (the file descriptor). If `self.file_or_error` already contains a
    /// non-negative value, the existing value is retained and the new
    /// descriptor is closed. In all cases, returns the final value of
    /// `self.file_or_error`.
    fn update(&self) -> i32 {
        let new_file_or_error;

        #[cfg(not(all(target_os = "linux", feature = "user_events")))]
        {
            new_file_or_error = 0;
        }
        #[cfg(all(target_os = "linux", feature = "user_events"))]
        {
            // Need to find the ".../tracing/user_events_data" file in tracefs or debugfs.

            // First, try the usual tracefs mount point.
            if let new_file @ 0.. = open_wronly(b"/sys/kernel/tracing/user_events_data\0") {
                new_file_or_error = new_file;
            } else {
                // Determine tracefs/debugfs mount point by parsing "/proc/mounts".
                // Prefer "tracefs" over "debugfs": if we find a debugfs, save the path but
                // keep looking in case we find a tracefs later.
                clear_errno();
                let mounts_file = unsafe {
                    linux::fopen(
                        "/proc/mounts\0".as_ptr().cast::<ffi::c_char>(),
                        "r\0".as_ptr().cast::<ffi::c_char>(),
                    )
                };
                if mounts_file.is_null() {
                    new_file_or_error = -get_failure_errno();
                } else {
                    let mut path = [0u8; 274]; // 256 + sizeof("/user_events_data")
                    let mut line = [0u8; 4097];
                    loop {
                        let fgets_result = unsafe {
                            linux::fgets(
                                line.as_mut_ptr().cast::<ffi::c_char>(),
                                line.len() as ffi::c_int,
                                mounts_file,
                            )
                        };
                        if fgets_result.is_null() {
                            break;
                        }

                        // line is "device_name mount_point file_system other_stuff..."

                        let mut line_pos = 0;

                        // device_name
                        while Self::is_nonspace_char(line[line_pos]) {
                            line_pos += 1;
                        }

                        // whitespace
                        while Self::is_space_char(line[line_pos]) {
                            line_pos += 1;
                        }

                        // mount_point
                        let mount_begin = line_pos;
                        while Self::is_nonspace_char(line[line_pos]) {
                            line_pos += 1;
                        }

                        let mount_end = line_pos;

                        // whitespace
                        while Self::is_space_char(line[line_pos]) {
                            line_pos += 1;
                        }

                        // file_system
                        let fs_begin = line_pos;
                        while Self::is_nonspace_char(line[line_pos]) {
                            line_pos += 1;
                        }

                        let fs_end = line_pos;

                        if !Self::is_space_char(line[line_pos]) {
                            // Ignore line if no whitespace after file_system.
                            continue;
                        }

                        let path_suffix: &[u8]; // Includes NUL
                        let fs = &line[fs_begin..fs_end];
                        let keep_looking;
                        if fs == b"tracefs" {
                            // "tracefsMountPoint/user_events_data"
                            path_suffix = b"/user_events_data\0";
                            keep_looking = false; // prefer "tracefs" over "debugfs"
                        } else if path[0] == 0 && fs == b"debugfs" {
                            // "debugfsMountPoint/tracing/user_events_data"
                            path_suffix = b"/tracing/user_events_data\0";
                            keep_looking = true; // prefer "tracefs" over "debugfs"
                        } else {
                            continue;
                        }

                        let mount_len = mount_end - mount_begin;
                        let path_len = mount_len + path_suffix.len(); // Includes NUL
                        if path_len > path.len() {
                            continue;
                        }

                        // path = mountpoint + suffix
                        path[0..mount_len].copy_from_slice(&line[mount_begin..mount_end]);
                        path[mount_len..path_len].copy_from_slice(path_suffix); // Includes NUL

                        if !keep_looking {
                            break;
                        }
                    }

                    unsafe { linux::fclose(mounts_file) };

                    if path[0] == 0 {
                        new_file_or_error = -linux::ENOTSUP;
                    } else {
                        // path is now something like "/sys/kernel/tracing/user_events_data\0" or
                        // "/sys/kernel/debug/tracing/user_events_data\0".
                        clear_errno();
                        new_file_or_error = if let new_file @ 0.. = open_wronly(&path) {
                            new_file
                        } else {
                            -get_failure_errno()
                        };
                    }
                }
            }
        }

        let mut old_file_or_error = Self::EAGAIN_ERROR;
        loop {
            match self.file_or_error.compare_exchange(
                old_file_or_error,
                new_file_or_error,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // We updated FILE_OR_ERROR to new.
                    return new_file_or_error;
                }
                Err(current_file_or_error) => {
                    // Somebody else updated FILE_OR_ERROR to current.
                    if current_file_or_error >= 0 || new_file_or_error < 0 {
                        // prefer current.
                        #[cfg(all(target_os = "linux", feature = "user_events"))]
                        if new_file_or_error >= 0 {
                            unsafe { linux::close(new_file_or_error) };
                        }
                        return current_file_or_error;
                    }

                    // current is an error, new is a file, try again.
                    old_file_or_error = current_file_or_error;
                }
            }
        }
    }

    // Initial state is -EAGAIN.
    pub const fn new() -> Self {
        return Self {
            file_or_error: AtomicI32::new(Self::EAGAIN_ERROR),
        };
    }

    // If file is open, closes it. Sets state to -EAGAIN.
    pub fn close(&self) {
        let file_or_error = self
            .file_or_error
            .swap(Self::EAGAIN_ERROR, Ordering::Relaxed);
        if file_or_error >= 0 {
            #[cfg(all(target_os = "linux", feature = "user_events"))]
            unsafe {
                linux::close(file_or_error)
            };
        }
    }

    // Returns existing state. This will be non-negative user_events_data file
    // descriptor or -errno if file is not currently open.
    #[cfg(all(target_os = "linux", feature = "user_events"))]
    pub fn peek(&self) -> i32 {
        return self.file_or_error.load(Ordering::Relaxed);
    }

    // If we have not already tried to open the `user_events_data` file, try
    // to open it, atomically update state, and return the new state. Otherwise,
    // return the existing state. Returns non-negative user_events_data file
    // descriptor on success or -errno for error.
    #[inline]
    pub fn get(&self) -> i32 {
        let file_or_error = self.file_or_error.load(Ordering::Relaxed);
        return if file_or_error == Self::EAGAIN_ERROR {
            self.update()
        } else {
            file_or_error
        };
    }
}

impl Drop for UserEventsDataFile {
    fn drop(&mut self) {
        self.close();
    }
}

/// Low-level API: Represents a tracepoint registration.
pub struct TracepointState {
    /// The kernel will update this variable with tracepoint enable/disable state.
    /// It will be 0 if tracepoint is disabled, nonzero if tracepoint is enabled.
    enable_status: AtomicU32,

    /// This will be a kernel-assigned value if registered,
    /// `UNREGISTERED_WRITE_INDEX` or `BUSY_WRITE_INDEX` if not registered.
    write_index: AtomicU32,

    _pinned: marker::PhantomPinned,
}

impl TracepointState {
    const UNREGISTERED_WRITE_INDEX: u32 = u32::MAX;
    const BUSY_WRITE_INDEX: u32 = u32::MAX - 1;
    const HIGHEST_VALID_WRITE_INDEX: u32 = u32::MAX - 2;

    #[cfg(all(target_os = "linux", feature = "user_events"))]
    const IOC_WRITE: ffi::c_ulong = 1;

    #[cfg(all(target_os = "linux", feature = "user_events"))]
    const IOC_READ: ffi::c_ulong = 2;

    #[cfg(all(target_os = "linux", feature = "user_events"))]
    const DIAG_IOC_MAGIC: ffi::c_ulong = '*' as ffi::c_ulong;

    #[cfg(all(target_os = "linux", feature = "user_events"))]
    const DIAG_IOCSREG: ffi::c_ulong =
        Self::ioc(Self::IOC_WRITE | Self::IOC_READ, Self::DIAG_IOC_MAGIC, 0);

    #[cfg(all(target_os = "linux", feature = "user_events"))]
    const DIAG_IOCSUNREG: ffi::c_ulong = Self::ioc(Self::IOC_WRITE, Self::DIAG_IOC_MAGIC, 2);

    #[cfg(all(target_os = "linux", feature = "user_events"))]
    const fn ioc(dir: ffi::c_ulong, typ: ffi::c_ulong, nr: ffi::c_ulong) -> ffi::c_ulong {
        const IOC_NRBITS: u8 = 8;
        const IOC_TYPEBITS: u8 = 8;
        const IOC_SIZEBITS: u8 = 14;
        const IOC_NRSHIFT: u8 = 0;
        const IOC_TYPESHIFT: u8 = IOC_NRSHIFT + IOC_NRBITS;
        const IOC_SIZESHIFT: u8 = IOC_TYPESHIFT + IOC_TYPEBITS;
        const IOC_DIRSHIFT: u8 = IOC_SIZESHIFT + IOC_SIZEBITS;

        return (dir << IOC_DIRSHIFT)
            | (typ << IOC_TYPESHIFT)
            | (nr << IOC_NRSHIFT)
            | ((size_of::<usize>() as ffi::c_ulong) << IOC_SIZESHIFT);
    }

    /// Creates a new unregistered tracepoint.
    ///
    /// initial_enable_status is normally 0, since an unregistered tracepoint will
    /// normally be considered disabled.
    pub const fn new(initial_enable_status: u32) -> Self {
        return Self {
            enable_status: AtomicU32::new(initial_enable_status),
            write_index: AtomicU32::new(Self::UNREGISTERED_WRITE_INDEX),
            _pinned: marker::PhantomPinned,
        };
    }

    /// Returns true if this tracepoint is enabled, i.e. `enable_status != 0`.
    #[inline(always)]
    pub fn enabled(&self) -> bool {
        return 0 != self.enable_status.load(Ordering::Relaxed);
    }

    /// Unregisters this tracepoint.
    ///
    /// Returns 0 for success, error code (e.g. EBUSY, EALREADY) for error.
    /// Error code is usually ignored in retail code, but may be helpful during
    /// development to understand behavior or track down issues.
    pub fn unregister(&self) -> i32 {
        let error;

        let old_write_index = self
            .write_index
            .swap(Self::BUSY_WRITE_INDEX, Ordering::Relaxed);
        match old_write_index {
            Self::BUSY_WRITE_INDEX => {
                error = 16; // EBUSY: Another thread is registering/unregistering. Do nothing.
                return error; // Return immediately, need to leave write_index = BUSY.
            }
            Self::UNREGISTERED_WRITE_INDEX => {
                error = 116; // EALREADY: Already unregistered. No action needed.
            }
            _ => {
                #[cfg(not(all(target_os = "linux", feature = "user_events")))]
                {
                    error = 0;
                }

                #[cfg(all(target_os = "linux", feature = "user_events"))]
                {
                    #[repr(C, packed)]
                    #[allow(non_camel_case_types)]
                    struct user_unreg {
                        size: u32,
                        disable_bit: u8,
                        reserved1: u8,
                        reserved2: u16,
                        disable_addr: u64,
                    }

                    let unreg = user_unreg {
                        size: size_of::<user_unreg>() as u32,
                        disable_bit: 0,
                        reserved1: 0,
                        reserved2: 0,
                        disable_addr: &self.enable_status as *const AtomicU32 as usize as u64,
                    };

                    clear_errno();
                    let ioctl_result = unsafe {
                        linux::ioctl(USER_EVENTS_DATA_FILE.peek(), Self::DIAG_IOCSUNREG, &unreg)
                    };
                    if 0 > ioctl_result {
                        error = get_failure_errno();
                    } else {
                        error = 0;
                    }
                }
            }
        }

        let old_write_index = self
            .write_index
            .swap(Self::UNREGISTERED_WRITE_INDEX, Ordering::Relaxed);
        debug_assert!(old_write_index == Self::BUSY_WRITE_INDEX);

        return error;
    }

    /// Registers this tracepoint.
    ///
    /// Requires: this `TracepointState` is not currently registered.
    ///
    /// Returns 0 for success, error code (e.g. EACCES, ENOENT) for error. The error code
    /// is usually ignored in retail scenarios but may be helpful during development to
    /// understand behavior or track down issues.
    ///
    /// `_name_args` is the tracepoint definition in the format
    /// `Name[ FieldDef1[;FieldDef2...]]`. For example:
    ///
    /// - `MyTracepoint1`
    /// - `MyTracepoint2 u32 Field1`
    /// - `MyTracepoint3 u32 Field1;char Field2[20]`
    ///
    /// # Safety
    ///
    /// The tracepoint must be unregistered before it is deallocated. `TracepointState`
    /// will unregister itself when dropped, so this is only an issue if the tracepoint
    /// is not dropped before it is deallocated. This might happen for a static variable
    /// in a shared library that gets unloaded.
    pub unsafe fn register(self: Pin<&Self>, _name_args: &ffi::CStr) -> i32 {
        return self.register_with_flags(_name_args, 0);
    }

    /// Advanced: Registers this tracepoint using the specified `user_reg` flags.
    ///
    /// Requires: this `TracepointState` is not currently registered.
    ///
    /// Returns 0 for success, error code (e.g. EACCES, ENOENT) for error. The error code
    /// is usually ignored in retail scenarios but may be helpful during development to
    /// understand behavior or track down issues.
    ///
    /// `_name_args` is the tracepoint definition in the format
    /// `Name[ FieldDef1[;FieldDef2...]]`. For example:
    ///
    /// - `MyTracepoint1`
    /// - `MyTracepoint2 u32 Field1`
    /// - `MyTracepoint3 u32 Field1;char Field2[20]`
    ///
    /// `_flags` is normally `0`, but may also be set to a `user_reg` flag such as
    /// `USER_EVENT_REG_PERSIST`.
    ///
    /// # Safety
    ///
    /// The tracepoint must be unregistered before it is deallocated. `TracepointState`
    /// will unregister itself when dropped, so this is only an issue if the tracepoint
    /// is not dropped before it is deallocated. This might happen for a static variable
    /// in a shared library that gets unloaded.
    pub unsafe fn register_with_flags(
        self: Pin<&Self>,
        _name_args: &ffi::CStr,
        _flags: u16,
    ) -> i32 {
        let error;
        let new_write_index;

        let old_write_index = self
            .write_index
            .swap(Self::BUSY_WRITE_INDEX, Ordering::Relaxed);
        assert!(
            old_write_index == Self::UNREGISTERED_WRITE_INDEX,
            "register of active tracepoint (already-registered or being-unregistered)"
        );

        let user_events_data = USER_EVENTS_DATA_FILE.get();
        if user_events_data < 0 {
            error = -user_events_data;
            new_write_index = Self::UNREGISTERED_WRITE_INDEX;
        } else {
            #[cfg(not(all(target_os = "linux", feature = "user_events")))]
            {
                error = 0;
                new_write_index = 0;
            }

            #[cfg(all(target_os = "linux", feature = "user_events"))]
            {
                #[repr(C, packed)]
                #[allow(non_camel_case_types)]
                struct user_reg {
                    size: u32,
                    enable_bit: u8,
                    enable_size: u8,
                    flags: u16,
                    enable_addr: u64,
                    name_args: u64,
                    write_index: u32,
                }

                let mut reg = user_reg {
                    size: size_of::<user_reg>() as u32,
                    enable_bit: 0,
                    enable_size: 4,
                    flags: _flags,
                    enable_addr: &self.enable_status as *const AtomicU32 as usize as u64,
                    name_args: _name_args.as_ptr() as usize as u64,
                    write_index: 0,
                };

                clear_errno();
                let ioctl_result =
                    unsafe { linux::ioctl(user_events_data, Self::DIAG_IOCSREG, &mut reg) };
                if 0 > ioctl_result {
                    error = get_failure_errno();
                    new_write_index = Self::UNREGISTERED_WRITE_INDEX;
                } else {
                    error = 0;
                    new_write_index = reg.write_index;
                    debug_assert!(new_write_index <= Self::HIGHEST_VALID_WRITE_INDEX);
                }
            }
        }

        let old_write_index = self.write_index.swap(new_write_index, Ordering::Relaxed);
        debug_assert!(old_write_index == Self::BUSY_WRITE_INDEX);

        return error;
    }

    /// Generates an event.
    ///
    /// Requires: `data[0].is_empty()` since it will be used for the event headers.
    ///
    /// Returns 0 for success, error code (e.g. EBADF) for error. The error code
    /// is usually ignored in retail scenarios but may be helpful during development to
    /// understand behavior or track down issues.
    ///
    /// If disabled or unregistered, this method does nothing and returnes EBADF.
    /// Otherwise, sets `data[0] = write_index` then sends `data[..]` to the
    /// `user_events_data` file handle.
    ///
    /// The event's payload is the concatenation of the remaining data blocks, if any
    /// (i.e. `data[1..]`).
    ///
    /// The payload's layout should match the args specified in the call to `register`.
    pub fn write(&self, data: &mut [EventDataDescriptor]) -> i32 {
        debug_assert!(data[0].is_empty());

        let enable_status = self.enable_status.load(Ordering::Relaxed);
        let write_index = self.write_index.load(Ordering::Relaxed);
        if enable_status == 0 || write_index > Self::HIGHEST_VALID_WRITE_INDEX {
            return 9; // linux::EBADF
        }

        let writev_result = self.writev(data, &write_index.to_ne_bytes());
        return writev_result;
    }

    /// Generates an event with headers.
    ///
    /// Requires: `data[0].is_empty()` since it will be used for the event headers;
    /// `headers.len() >= 4` since it will be used for `write_index`.
    ///
    /// Returns 0 for success, error code (e.g. EBADF) for error. The error code
    /// is usually ignored in retail scenarios but may be helpful during development to
    /// understand behavior or track down issues.
    ///
    /// If disabled or unregistered, this method does nothing and returnes EBADF.
    /// Otherwise, sets `data[0] = headers` and `headers[0..4] = write_index`, then sends
    /// `data[..]` to the `user_events_data` file.
    ///
    /// The event's payload is the concatenation of the remaining data blocks, if any
    /// (i.e. `data[1..]`).
    ///
    /// The payload's layout should match the args specified in the call to `register`.
    pub fn write_with_headers(&self, data: &mut [EventDataDescriptor], headers: &mut [u8]) -> i32 {
        debug_assert!(data[0].is_empty());
        debug_assert!(headers.len() >= 4);

        let enable_status = self.enable_status.load(Ordering::Relaxed);
        let write_index = self.write_index.load(Ordering::Relaxed);
        if enable_status == 0 || write_index > Self::HIGHEST_VALID_WRITE_INDEX {
            return 9; // linux::EBADF
        }

        *<&mut [u8; 4]>::try_from(&mut headers[0..4]).unwrap() = write_index.to_ne_bytes();

        let writev_result = self.writev(data, headers);
        return writev_result;
    }

    // Returns 0 for success, errno for error.
    fn writev(&self, _data: &mut [EventDataDescriptor], _headers: &[u8]) -> i32 {
        #[cfg(all(target_os = "linux", feature = "user_events"))]
        unsafe {
            // Unsafe: Putting headers into a container a with longer lifetime.
            _data[0] =
                EventDataDescriptor::from_raw_ptr(_headers.as_ptr() as usize, _headers.len());

            let writev_result = linux::writev(
                USER_EVENTS_DATA_FILE.peek(),
                _data.as_ptr() as *const linux::iovec,
                _data.len() as i32,
            );

            // Clear the container before headers lifetime ends.
            _data[0] = EventDataDescriptor::zero();

            if 0 > writev_result {
                return get_failure_errno();
            }
        }

        return 0;
    }
}

impl Drop for TracepointState {
    fn drop(&mut self) {
        self.unregister();
    }
}

/// Possible configurations under which this crate can be compiled: `LinuxUserEvents` or
/// `Other`.
pub enum NativeImplementation {
    /// Crate compiled for other configuration (no logging is performed).
    Other,

    /// Crate compiled for Linux user_events configuration (logging is performed via
    /// `user_events_data` file).
    LinuxUserEvents,
}

/// The configuration under which this crate was compiled: `LinuxUserEvents` or `Other`.
pub const NATIVE_IMPLEMENTATION: NativeImplementation =
    if cfg!(all(target_os = "linux", feature = "user_events")) {
        NativeImplementation::LinuxUserEvents
    } else {
        NativeImplementation::Other
    };
