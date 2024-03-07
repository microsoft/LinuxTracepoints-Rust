// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#![no_std]
#![warn(missing_docs)]
#![allow(clippy::needless_return)]

//! # Linux Tracepoints

// Exports from tracepoint:
pub use descriptors::EventDataDescriptor;
pub use native::NativeImplementation;
pub use native::TracepointState;
pub use native::NATIVE_IMPLEMENTATION;
pub mod changelog;

mod descriptors;
mod native;
