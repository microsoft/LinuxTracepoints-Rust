// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

//! Release history

#[allow(unused_imports)]
use crate::*; // For docs

/// # v0.4.1 (TBD)
/// - Move eventheader types into separate `eventheader_types` crate.
pub mod v0_4_1 {}

/// # v0.4.0 (2024-04-12)
/// - BUG FIX: Fix `EADDRINUSE` returned during `register()` on newer kernels.
///   The "name already in use" detection splits on whitespace, while all other
///   processing splits on semicolon. Fix by adding space after each semicolon
///   in `EVENTHEADER_COMMAND_TYPES`.
/// - Move non-eventheader code into separate `tracepoint` crate.
pub mod v0_4_0 {}

/// # v0.3.5 (2024-02-27)
/// - Open `user_events_data` for WRONLY instead of RDWR.
pub mod v0_3_5 {}

/// # v0.3.4 (2023-11-27)
/// - Changed procedure for locating the `user_events_data` file.
///   - Old: parse `/proc/mounts` to determine the `tracefs` or `debugfs` mount
///     point, then use that as the root for the `user_events_data` path.
///   - New: try `/sys/kernel/tracing/user_events_data`; if that doesn't exist,
///     parse `/proc/mounts` to find the `tracefs` or `debugfs` mount point.
///   - Rationale: Probe an absolute path so that containers don't have to
///     create a fake `/proc/mounts` and for efficiency.
pub mod v0_3_4 {}

/// # v0.3.2 (2023-07-24)
/// - Prefer "tracefs" over "debugfs" when searching for `user_events_data`.
///   (Old behavior: no preference - use whichever comes first in mount list.)
pub mod v0_3_2 {}

/// # v0.3.1 (2023-07-12)
/// - Use `c_char` instead of `i8` for ffi strings.
pub mod v0_3_1 {}

/// # v0.3.0 (2023-06-29)
/// - If no consumers have enabled a tracepoint, the kernel now returns
///   `EBADF`. The eventheader crate has been updated to be consistent with
///   the new behavior.
pub mod v0_3_0 {}

/// # v0.2.0 (2023-05-16)
/// - Add support for macro-based logging.
pub mod v0_2_0 {}

/// # v0.1.0 (2023-04-13)
/// - Initial release.
pub mod v0_1_0 {}
