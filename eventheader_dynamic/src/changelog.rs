//! Release history

#[allow(unused_imports)]
use crate::*; // For docs

/// # v0.3.4 (TBD)
/// - Changed procedure for locating the `user_events_data` file.
///   - Old: parse `/proc/mounts` to determine the `tracefs` or `debugfs` mount
///     point, then use that as the root for the `user_events_data` path.
///   - New: try `/sys/kernel/tracing/user_events_data`, then try
///     `/sys/kernel/debug/tracing/user_events_data`, and then parse `/proc/mounts`
///     (i.e. only parse `/proc/mounts` if the absolute paths don't exist)
pub mod v0_3_4 {}

/// # v0.3.3 (2023-08-08)
/// - Add [`EventBuilder::add_struct_with_bookmark`] and
///   [`EventBuilder::set_struct_field_count`] methods to support cases where the number
///   of sub-fields of a struct is not known when the struct is started.
pub mod v0_3_3 {}

/// # v0.3.2 (2023-07-24)
/// - Prefer "tracefs" over "debugfs" when searching for `user_events_data`.
///   (Old behavior: no preference - use whichever comes first in mount list.)
pub mod v0_3_2 {}

/// # v0.3.1 (2023-07-12)
/// - Use `c_char` instead of `i8` for ffi strings.
pub mod v0_3_1 {}

/// # v0.3.0 (2023-06-29)
/// - If no consumers have enabled a tracepoint, the kernel now returns
///   `EBADF`. The eventheader_dynamic crate has been updated to be
///   consistent with the new behavior.
pub mod v0_3_0 {}

/// # v0.2.0 (2023-05-16)
/// - Add support for macro-based logging.
pub mod v0_2_0 {}

/// # v0.1.0 (2023-04-13)
/// - Initial release.
pub mod v0_1_0 {}
