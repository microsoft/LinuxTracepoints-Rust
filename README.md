# Linux Tracepoints for Rust

- [eventheader](eventheader) provides an efficient high-level macro-based API
  for generating compile-time specified events using the `EventHeader`
  convention.  The events are written using the `user_events` system. This is
  intended for use by developers that want to log events from their code.  This
  crate also contains utility code shared with `eventheader_dynamic`.
- [eventheader_dynamic](eventheader_dynamic) provides a mid-level API
  for generating runtime-specified events using the `EventHeader`
  convention. The events are written using the `user_events` system.
  This is intended for use as an implementation layer for a higher-level
  dynamic-event API like OpenTelemetry.
- [eventheader_macros](eventheader_macros) provides proc macros for
  compile-time-defined events. The macros are exposed by the
  `eventheader` crate.
- [eventheader_types](eventheader_types) contains type definitions for the
  `EventHeader` encoding convention.
- [tracepoint](tracepoint) provides low-level building blocks for logging
  [Tracepoints](https://www.kernel.org/doc/html/latest/trace/tracepoints.html)
  via the Linux [user_events](https://docs.kernel.org/trace/user_events.html)
  system.
- [tracepoint_decode](tracepoint_decode) provides support for decoding tracepoint
  event data, including support for both traditional (tracefs) event decoding and
  `EventHeader` decoding.
- [tracepoint_perf](tracepoint_perf) provides support for reading and writing the
  `perf.data` file format.

## Contributing

This project welcomes contributions and suggestions.  Most contributions require you to agree to a
Contributor License Agreement (CLA) declaring that you have the right to, and actually do, grant us
the rights to use your contribution. For details, visit https://cla.opensource.microsoft.com.

When you submit a pull request, a CLA bot will automatically determine whether you need to provide
a CLA and decorate the PR appropriately (e.g., status check, comment). Simply follow the instructions
provided by the bot. You will only need to do this once across all repos using our CLA.

This project has adopted the [Microsoft Open Source Code of Conduct](https://opensource.microsoft.com/codeofconduct/).
For more information see the [Code of Conduct FAQ](https://opensource.microsoft.com/codeofconduct/faq/) or
contact [opencode@microsoft.com](mailto:opencode@microsoft.com) with any additional questions or comments.

## Trademarks

This project may contain trademarks or logos for projects, products, or services. Authorized use of Microsoft 
trademarks or logos is subject to and must follow 
[Microsoft's Trademark & Brand Guidelines](https://www.microsoft.com/en-us/legal/intellectualproperty/trademarks/usage/general).
Use of Microsoft trademarks or logos in modified versions of this project must not cause confusion or imply Microsoft sponsorship.
Any use of third-party trademarks or logos are subject to those third-party's policies.
