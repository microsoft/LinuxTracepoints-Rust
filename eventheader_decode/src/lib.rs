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
pub use enumerator::NameAndTagDisplay;
pub use enumerator::NameDisplay;
pub use perf_item::PerfConvertOptions;
pub use perf_item::PerfInfoOptions;
pub use perf_item::PerfItemMetadata;
pub use perf_item::PerfItemValue;
pub use perf_item::PerfTextEncoding;
pub mod _internal;
pub mod changelog;

mod byte_reader;
mod charconv;
mod enumerator;
mod filters;
mod perf_item;
mod writers;
