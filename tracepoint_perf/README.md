# perf.data file format

This crate provides support for reading and writing files compatible with the
`perf.data` files generated by the Linux
[perf](https://www.man7.org/linux/man-pages/man1/perf-record.1.html) tool.

Core types:

- `PerfDataFileReader` supports enumerating the headers and events from a `perf.data` file.
  It exposes the data using types from the [tracepoint_decode](../tracepoint_decode) crate,
  making it easy to decode the resulting events and their fields.
- `PerfDataFileWriter` supports writing a `perf.data` file containing caller-supplied headers
  and event data. It includes support for synthesizing some of the more commonly-used headers
  from `tracepoint_decode` metadata types, including the `TRACING_DATA` and `EVENT_DESC`
  headers.

Examples:

- **[decode_perf](examples/decode_perf.rs):** demonstrates using `PerfDataFileReader` along
  with the types from the [tracepoint_decode](../tracepoint_decode) crate to decode a
  `perf.data` file and write it as text to stdout.
- **[decode_perf_to_json](examples/decode_perf_to_json.rs):** expands on the `decode_perf`
  sample and converts event data to JSON.
- **[rewrite_perf](examples/rewrite_perf.rs):** demonstrates using `PerfDataFileWriter`,
  generating a new `perf.data` file using content from an existing `perf.data` file.
