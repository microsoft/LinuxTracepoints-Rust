# Low-level support for Linux Tracepoints

[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]
![maintenance status][maint-badge]

[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/microsoft/LinuxTracepoints-Rust/blob/main/LICENSE
[actions-badge]: https://github.com/microsoft/LinuxTracepoints-Rust/actions/workflows/Rust.yml/badge.svg
[actions-url]: https://github.com/microsoft/LinuxTracepoints-Rust/actions/workflows/Rust.yml
[maint-badge]: https://img.shields.io/badge/maintenance-experimental-blue.svg

The `tracepoint` crate provides low-level building blocks for logging
[Tracepoints](https://www.kernel.org/doc/html/latest/trace/tracepoints.html)
via the Linux [user_events](https://docs.kernel.org/trace/user_events.html)
system. The events can be generated and collected on Linux 6.4 or later
(requires the `user_events` kernel feature to be enabled, the `tracefs` or
`debugfs` filesystem to be mounted, and appropriate permissions configured for
the `/sys/kernel/.../tracing/user_events_data` file).
