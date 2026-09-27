# tinydesktop-cursor-overlay

The helper process that draws the agent's cursor above everything on screen.
The module starts it on first use and writes `tinydesktop-cursor`'s
`OverlayCommand`s to its standard input, one per line. It exits when that
input closes.

| Path | Holds |
|---|---|
| `src/driver/` | commands in, window position, sprite frame, and opacity out; platform-independent and tested |
| `src/platform/macos.rs` | a borderless, click-through, never-activating `NSWindow` |
| `src/platform/windows.rs` | a layered, topmost, click-through, non-activating Win32 window |
| `src/platform/other.rs` | drains input and draws nothing |

The platform modules are foreign-function code, so this crate carries its own
lint table with `unsafe_code = "deny"` instead of the workspace's `forbid`.
Only those modules allow `unsafe`, and each `unsafe` block has a `// SAFETY:`
comment.

To try it:

```sh
cargo build -p tinydesktop-cursor-overlay
cargo run -p tinydesktop-examples --bin cursor_demo -- calm 3
```
