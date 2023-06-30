//! Release history

#[allow(unused_imports)]
use crate::*; // For docs

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
