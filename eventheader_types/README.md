# EventHeader encoding core types

`EventHeader` is an event encoding format. This format is used to encode event metadata
and data for [Tracepoints](https://www.kernel.org/doc/html/latest/trace/tracepoints.html)
used with the Linux [user_events](https://docs.kernel.org/trace/user_events.html) system.

The `eventheader_types` provides support types for the `EventHeader` encoding format.
This crate is used when logging `EventHeader`-style events and when decoding them.
