// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#![doc(hidden)]
//! Internal implementation details for eventheader macros and eventheader_dynamic.
//! Contents subject to change without notice.

use core::mem;
use core::ptr;
use core::time::Duration;

use crate::enums::ExtensionKind;

pub use tracepoint::EventDataDescriptor;
pub use tracepoint::TracepointState;

pub use crate::descriptors::slice_count;
pub use crate::descriptors::EventHeader;
pub use crate::descriptors::EventHeaderExtension;
pub use crate::enums::HeaderFlags;
pub use crate::provider::provider_new;
pub use crate::provider::CommandString;
pub use crate::provider::EventHeaderTracepoint;

/// Type string for use in the DIAG_IOCSREG command string.
pub const EVENTHEADER_COMMAND_TYPES: &str =
    "u8 eventheader_flags; u8 version; u16 id; u16 tag; u8 opcode; u8 level";

/// Maximum length of a Tracepoint name "ProviderName_Attributes\0" (includes nul).
pub const EVENTHEADER_NAME_MAX: usize = 256;

/// Maximum length needed for a DIAG_IOCSREG command "ProviderName_Attributes CommandTypes\0".
pub const EVENTHEADER_COMMAND_MAX: usize =
    EVENTHEADER_NAME_MAX + 1 + EVENTHEADER_COMMAND_TYPES.len();

/// First byte of tag.
pub const fn tag_byte0(tag: u16) -> u8 {
    return tag.to_ne_bytes()[0];
}

/// Second byte of tag.
pub const fn tag_byte1(tag: u16) -> u8 {
    return tag.to_ne_bytes()[1];
}

/// Returns the time_t corresponding to a duration returned by a successful call to
/// `systemtime.duration_since(SystemTime::UNIX_EPOCH)`.
/// ```
/// # use eventheader::_internal as ehi;
/// # use std::time::SystemTime;
/// let systemtime = SystemTime::now();
/// let time_t = match systemtime.duration_since(SystemTime::UNIX_EPOCH) {
///     Ok(dur) => ehi::time_from_duration_after_1970(dur),
///     Err(err) => ehi::time_from_duration_before_1970(err.duration()),
/// };
/// ```
pub const fn time_from_duration_after_1970(duration: Duration) -> i64 {
    const I64_MAX: u64 = i64::MAX as u64;
    let duration_secs = duration.as_secs();
    if duration_secs > I64_MAX {
        i64::MAX
    } else {
        duration_secs as i64
    }
}

/// Returns the time_t corresponding to a duration returned by a failed call to
/// `systemtime.duration_since(SystemTime::UNIX_EPOCH)`.
/// ```
/// # use eventheader::_internal as ehi;
/// # use std::time::SystemTime;
/// let systemtime = SystemTime::now();
/// let filetime = match systemtime.duration_since(SystemTime::UNIX_EPOCH) {
///     Ok(dur) => ehi::time_from_duration_after_1970(dur),
///     Err(err) => ehi::time_from_duration_before_1970(err.duration()),
/// };
/// ```
pub const fn time_from_duration_before_1970(duration: Duration) -> i64 {
    const I64_MAX: u64 = i64::MAX as u64;
    let duration_secs = duration.as_secs();
    if duration_secs > I64_MAX {
        i64::MIN
    } else {
        // Note: Rounding towards negative infinity.
        -(duration_secs as i64) - ((duration.subsec_nanos() != 0) as i64)
    }
}

/// Copies the specified value to the specified location.
/// Returns the pointer after the end of the copy.
///
/// # Safety
///
/// Caller is responsible for making sure there is sufficient space in the buffer.
unsafe fn append_bytes<T: Sized>(dst: *mut u8, src: &T) -> *mut u8 {
    let size = mem::size_of::<T>();
    ptr::copy_nonoverlapping(src as *const T as *const u8, dst, size);
    return dst.add(size);
}

/// Fills in `data[0]` with the event's write_index, event_header,
/// activity extension block (if an activity id is provided), and
/// metadata extension block header (if meta_len != 0), then sends
/// the event to the `user_events_data` file.
///
/// Requires:
/// - `data[0].is_empty()` since it will be used for the headers.
/// - related_id may only be present if activity_id is present.
/// - if activity_id.is_some() || meta_len != 0 then event_header.flags
///   must equal DefaultWithExtension.
/// - If meta_len != 0 then `data[1]` starts with metadata extension
///   block data.
pub fn write_eventheader(
    state: &TracepointState,
    event_header: &EventHeader,
    activity_id: Option<&[u8; 16]>,
    related_id: Option<&[u8; 16]>,
    meta_len: u16,
    data: &mut [EventDataDescriptor],
) -> i32 {
    debug_assert!(data[0].is_empty());
    debug_assert!(related_id.is_none() || activity_id.is_some());
    debug_assert!(
        (activity_id.is_none() && meta_len == 0)
            || event_header.flags == HeaderFlags::DefaultWithExtension
    );

    let mut extension_count = (activity_id.is_some() as u8) + ((meta_len != 0) as u8);

    const HEADERS_SIZE_MAX: usize = mem::size_of::<u32>() // write_index
        + mem::size_of::<EventHeader>() // event_header
        + mem::size_of::<EventHeaderExtension>() + 16 + 16 // activity header + activity_id + related_id
        + mem::size_of::<EventHeaderExtension>(); // metadata header (last because data[1] has the metadata)
    let mut headers: [u8; HEADERS_SIZE_MAX] = [0; HEADERS_SIZE_MAX];
    let headers_len;
    unsafe {
        let mut headers_ptr = headers.as_mut_ptr().add(mem::size_of::<u32>()); // write_index
        headers_ptr = append_bytes(headers_ptr, event_header);

        match activity_id {
            None => debug_assert!(related_id.is_none()),
            Some(aid) => match related_id {
                None => {
                    extension_count -= 1;
                    headers_ptr = append_bytes(
                        headers_ptr,
                        &EventHeaderExtension::from_parts(
                            16,
                            ExtensionKind::ActivityId,
                            extension_count > 0,
                        ),
                    );
                    headers_ptr = append_bytes(headers_ptr, aid);
                }
                Some(rid) => {
                    extension_count -= 1;
                    headers_ptr = append_bytes(
                        headers_ptr,
                        &EventHeaderExtension::from_parts(
                            32,
                            ExtensionKind::ActivityId,
                            extension_count > 0,
                        ),
                    );
                    headers_ptr = append_bytes(headers_ptr, aid);
                    headers_ptr = append_bytes(headers_ptr, rid);
                }
            },
        }

        if meta_len != 0 {
            extension_count -= 1;
            headers_ptr = append_bytes(
                headers_ptr,
                &EventHeaderExtension::from_parts(
                    meta_len,
                    ExtensionKind::Metadata,
                    extension_count > 0,
                ),
            );
        }

        headers_len = headers_ptr.offset_from(headers.as_mut_ptr()) as usize;
    }

    debug_assert!(headers_len <= headers.len());
    debug_assert!(extension_count == 0);

    let writev_result = state.write_with_headers(data, &mut headers[0..headers_len]);
    return writev_result;
}
