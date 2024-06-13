// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

//! Release history

#[allow(unused_imports)]
use crate::*; // For docs

/// # v0.4.1 (TBD)
/// - Move eventheader types from `eventheader` into new `eventheader_types` crate.
/// - New field encoding `BinaryLength16Char8`. Same as
///   `StringLength16Char8` except that its default format
///   is `HexBytes`.
/// - New semantics for `BinaryLength16Char8` and
///   `StringLength16Char8` encodings to support nullable
///   and variable-length fields. These encodings can now be used with any format.
///   When used with a fixed-size format, this indicates a nullable field. For
///   example, a field with encoding `BinaryLength16Char8` and format
///   `SignedInt` with length 1, 2, 4, or 8 would be formatted as a signed
///   integer. The same field with length 0 would be formatted as a `null`. Any
///   other length would be formatted as `HexBytes`.
/// - Deprecated `IPv4` and `IPv6` formats. New code should use the `IPAddress`
///   format. When applied to a 4-byte field, `IPAddress` should format as IPv4,
///   and when applied to a 16-byte field, `IPAddress` should format as IPv6.
pub mod v0_4_1 {}
