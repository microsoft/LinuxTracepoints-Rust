// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#![no_std]
#![allow(clippy::needless_return)]
//#![warn(missing_docs)] // TODO

//! EventHeader decoding

pub use enumerator::EventHeaderEnumerator;
pub use enumerator::EventHeaderEnumeratorContext;
pub use enumerator::EventHeaderEnumeratorError;
pub use enumerator::EventHeaderEnumeratorState;
pub use enumerator::EventHeaderEventInfo;
pub use enumerator::EventHeaderItemInfo;
pub use reader::PerfByteReader;
pub use perf_item::PerfItemType;
pub use perf_item::PerfItemValue;

pub mod changelog;

mod enumerator;
mod perf_item;
mod reader;
