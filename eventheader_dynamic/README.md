# EventHeader for Rust

[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]
[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]
![maintenance status][maint-badge]

[crates-badge]: https://img.shields.io/crates/v/eventheader_dynamic.svg
[crates-url]: https://crates.io/crates/eventheader_dynamic
[docs-badge]: https://docs.rs/eventheader_dynamic/badge.svg
[docs-url]: https://docs.rs/eventheader_dynamic
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: LICENSE
[actions-badge]: https://github.com/microsoft/LinuxTracepoints-Rust/actions/workflows/Rust.yml/badge.svg
[actions-url]: https://github.com/microsoft/LinuxTracepoints-Rust/actions/workflows/Rust.yml
[maint-badge]: https://img.shields.io/badge/maintenance-experimental-blue.svg

The `eventheader_dynamic` crate provides a flexible way to log
`EventHeader`-encoded
[Tracepoints](https://www.kernel.org/doc/html/latest/trace/tracepoints.html)
via the Linux [user_events](https://docs.kernel.org/trace/user_events.html)
system. The events can be generated and collected on Linux 6.4 or later
(requires the `user_events` kernel feature to be enabled, the `tracefs` or
`debugfs` filesystem to be mounted, and appropriate permissions configured for
the `/sys/kernel/.../tracing/user_events_data` file).

This "dynamic" implementation is more flexible than the implementation in the
`eventheader` crate. For example, it supports runtime-defined schema and can
easily log arrays of strings. However, it is harder to use, it has higher
runtime costs, and it depends on the `alloc` crate. This dynamic implementation
is intended for use only when the set of events cannot be determined at
compile-time. For example, `eventheader_dynamic` might be used to implement a
middle-layer library providing tracing support to a scripting language like
JavaScript or Python. In other cases, use the `eventheader` crate instead of
this crate.
