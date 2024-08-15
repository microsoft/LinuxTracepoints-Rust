# Tracepoint decoding

This crate provides support for decoding data from Linux
[Tracepoints](https://www.kernel.org/doc/html/latest/trace/tracepoints.html). This includes
support for parsing the event record, parsing tracefs-style `format` files, parsing
`EventHeader`-style decoding information, extracting fields from the event record, converting
field data to Rust types, and converting field data to strings.

Core types:

- `PerfItemValue` represents the data from a field of a tracepoint event. It has strong type
  information and includes helpers for converting the field value to a Rust type and formatting
  the field value as a string.
- `PerfEventFormat` and `PerfFieldFormat` support parsing a tracefs-style `format` file. They
  include helpers for extracting `PerfItemValue` field values from a tracepoint event record.
- `EventHeaderEnumerator` supports parsing an `EventHeader`-style event.

This crate does not directly interact with any data sources, e.g. it does not consume from
trace buffers or `perf.data` files. Instead, a data source implementation is expected to
expose event data and metadata to the consumer using types from this crate (e.g.
`PerfEventBytes`, `PerfNonSampleEventInfo`, `PerfSampleEventInfo`), and the consumer can use
types from this crate to access event information, access field data, and convert the data to
strings.

For example, the [tracepoint_perf](../tracepoint_perf) crate parses `perf.data` files and
exposes the data using the types from this crate.
