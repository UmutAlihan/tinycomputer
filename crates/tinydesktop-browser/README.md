# tinydesktop-browser

The agent-browser adapter behind tinydesktop's `Browser` interface. It turns the
typed requests in `tinydesktop_bus::browser` into agent-browser commands and
the engine's replies into typed results. agent-browser is linked in-process as
a library; there is no daemon and no sidecar.

The crate has no bus, no agent loop and no model. `tinydesktop-engine` drives
it as a surface next to the desktop, and `tinydesktop` serves it over TinyBus.

| Path | Holds |
|---|---|
| `src/error/` | `Error`: what a caller should do next, one published wire name per variant |
| `src/outputs/` | held screenshots and PDFs: bounded count, size and lifetime, chunked reads |

Tests that launch Chromium run in the Docker lab (`docs/docker-lab.md`), never
on the host.
