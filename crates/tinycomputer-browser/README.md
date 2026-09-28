# tinycomputer-browser

The agent-browser adapter behind tinycomputer's `Browser` interface. It turns the
typed requests in `tinycomputer_bus::browser` into agent-browser commands and
the engine's replies into typed results. agent-browser is linked in-process as
a library; there is no daemon and no sidecar.

The crate has no bus, no agent loop and no model. `tinycomputer-engine` drives
it as a surface next to the desktop, and `tinycomputer` serves it over TinyBus.

| Path | Holds |
|---|---|
| `src/sessions/` | `Browser`: up to eight sessions, each with its own engine and scratch directory; calls on one session are serialized |
| `src/engine/` | the `Engine` and `Launcher` seam: one JSON command in, one reply out, so everything above it is tested with a scripted engine |
| `src/linked/` | `AgentBrowser` (feature `agent-browser`): agent-browser's dispatcher linked in-process, one `DaemonState` per session, built without reading the host's `AGENT_BROWSER_*` environment |
| `src/convert/` | contract requests to agent-browser commands, as pure functions |
| `src/reply/` | agent-browser replies to typed results |
| `src/surface/` | `BrowserSurface`: one session as a `tinycomputer-core` `Surface`; `sight/`, which reads the rendered page the way a person looks at it (the default, `docs/specs/browser-sight.md`), and `tree.rs`, which parses snapshot text into a `Screen` when sight cannot reach the page or `Perception::Tree` is chosen |
| `src/error/` | `Error`: what a caller should do next, one published wire name per variant |
| `src/outputs/` | held screenshots and PDFs: bounded count, size and lifetime, chunked reads |
| `src/fake/` | the scripted engine tests use |

`BrowserSurface` opens its session lazily and can attach to a running Chrome
through `SessionOptions::endpoint` instead of launching one. Closing an
attached session only disconnects. See `docs/architecture.md` for how the
surface compares with the desktop's.

Tests that launch Chromium run in the Docker lab (`docs/docker-lab.md`), never
on the host.
