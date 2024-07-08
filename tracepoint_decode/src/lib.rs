// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#![no_std]
#![warn(missing_docs)]
#![allow(clippy::needless_return)]

//! Tracepoint decoding

pub use byte_reader::PerfByteReader;

pub use enumerator::EventHeaderEnumerator;
pub use enumerator::EventHeaderEnumeratorContext;
pub use enumerator::EventHeaderEnumeratorError;
pub use enumerator::EventHeaderEnumeratorState;
pub use enumerator::EventHeaderEventInfo;
pub use enumerator::EventHeaderItemInfo;
pub use enumerator::NameAndTagDisplay;
pub use enumerator::NameDisplay;

pub use perf_abi::PerfEventAttr;
pub use perf_abi::PerfEventAttrOptions;
pub use perf_abi::PerfEventAttrReadFormat;
pub use perf_abi::PerfEventAttrSampleType;
pub use perf_abi::PerfEventAttrSize;
pub use perf_abi::PerfEventAttrType;
pub use perf_abi::PerfEventHeader;
pub use perf_abi::PerfEventHeaderMisc;
pub use perf_abi::PerfEventHeaderMiscCpuMode;
pub use perf_abi::PerfEventHeaderType;

pub use perf_event_data::PerfEventBytes;
pub use perf_event_data::PerfNonSampleEventInfo;
pub use perf_event_data::PerfSampleEventInfo;

pub use perf_event_desc::PerfEventDesc;

pub use perf_event_format::PerfEventDecodingStyle;
pub use perf_event_format::PerfEventFormat;

pub use perf_field_format::PerfFieldArray;
pub use perf_field_format::PerfFieldFormat;

pub use perf_item::PerfConvertOptions;
pub use perf_item::PerfItemMetadata;
pub use perf_item::PerfItemValue;
pub use perf_item::PerfMetaOptions;
pub use perf_item::PerfTextEncoding;

pub use perf_session::PerfSessionInfo;
pub use perf_session::PerfTimeSpec;

pub mod _internal;
pub mod changelog;

mod byte_reader;
mod charconv;
mod enumerator;
mod filters;
mod perf_abi;
mod perf_event_data;
mod perf_event_desc;
mod perf_event_format;
mod perf_field_format;
mod perf_item;
mod perf_session;
mod writers;
