// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#![no_std]
#![allow(clippy::needless_return)]
#![warn(missing_docs)]

//! EventHeader decoding

pub use enumerator::EventHeaderEnumerator;
pub use enumerator::EventHeaderEnumeratorContext;
pub use enumerator::EventHeaderEnumeratorError;
pub use enumerator::EventHeaderEnumeratorState;
pub use enumerator::EventHeaderEventInfo;
pub use enumerator::EventHeaderItemInfo;
pub use byte_reader::PerfByteReader;
pub use perf_item::PerfItemMetadata;
pub use perf_item::PerfItemValue;

pub mod changelog;

mod enumerator;
mod perf_item;
mod byte_reader;
