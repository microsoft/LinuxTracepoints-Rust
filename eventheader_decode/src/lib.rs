// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#![no_std]
#![allow(clippy::needless_return)]
#![warn(missing_docs)]

//! EventHeader decoding

pub use byte_reader::PerfByteReader;
pub use enumerator::EventHeaderEnumerator;
pub use enumerator::EventHeaderEnumeratorContext;
pub use enumerator::EventHeaderEnumeratorError;
pub use enumerator::EventHeaderEnumeratorState;
pub use enumerator::EventHeaderEventInfo;
pub use enumerator::EventHeaderItemInfo;
pub use enumerator::NameChars;
pub use perf_item::PerfConvertOptions;
pub use perf_item::PerfItemMetadata;
pub use perf_item::PerfItemValue;
pub mod _internal;
pub mod changelog;

mod byte_reader;
mod enumerator;
mod perf_item;
