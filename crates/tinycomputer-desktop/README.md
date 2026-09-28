# tinycomputer-desktop

Part of [tinycomputer](../../README.md), a decision model (Jev) based harness for
desktop and browser automation, written in Rust. This crate is its desktop side: it reads and drives native applications through their accessibility trees. Its user guide is
[`docs/crates/tinycomputer-desktop/`](../../docs/crates/tinycomputer-desktop/README.md).

The agent-desktop adapter behind tinycomputer: `Desktop`, with one typed method
per desktop member. Each method takes a request from `tinycomputer-bus` and
returns a `DesktopResponse` envelope, so a stale ref or a missing permission
comes back as a result a caller can act on, not as a Rust error.

This crate has no bus, no agent loop and no model. `tinycomputer-engine` builds
the Jev runtime and the task controller on top of it, and `tinycomputer` serves
everything over TinyBus.

| Path | Holds |
|---|---|
| `src/desktop/` | `Desktop`, split by family; `convert.rs` maps contract types to engine arguments, `permission.rs` runs the preflight, `reply.rs` builds the envelope |
| `src/surface/` | `Desktop` as a `tinycomputer_core::surface::Surface`: snapshot parsing for decision loops, window choice, clipboard-backed paste |
| `src/error/` | the crate-wide `Error` and `Result`, for construction failures only |
