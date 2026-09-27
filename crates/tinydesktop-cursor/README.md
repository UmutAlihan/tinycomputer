# tinydesktop-cursor

The agent's one on-screen cursor, shared by the desktop and browser surfaces:
a second pointer that glides to where the agent is about to act. It is purely
cosmetic. It sends no input and never moves the user's pointer.

| Module | Holds |
|---|---|
| `glide/` | `VirtualCursor` and `Glide`: aim points inside an element, and human paths with overshoot, correction, bow, wobble, tremor, and Fitts timing |
| `screen/` | `ScreenCursor`, the shared cursor; `OverlaySink`; and `ProcessOverlay`, which starts the helper |
| `protocol/` | `OverlayCommand`: one JSON command per line to the overlay |
| `animate/` | `Animator`: frames over time, including fades, the pulse, and the idle hide |
| `sprite/` | `Sprite`: the cursor's look as RGBA frames, and a PNG encoder |
| `pace/` | `CursorPace`: `off`, `brisk`, `natural` (default), `calm` |

With the `overlay` feature the crate also builds `tinydesktop-cursor-overlay`,
the helper that draws the cursor (`src/bin/tinydesktop-cursor-overlay/`):

| Path | Holds |
|---|---|
| `driver/` | commands in, window position, sprite frame, and opacity out; platform-independent and tested |
| `platform/macos.rs` | a borderless, click-through, never-activating `NSWindow` |
| `platform/windows.rs` | a layered, topmost, click-through, non-activating Win32 window |
| `platform/other.rs` | drains input and draws nothing |

Those platform modules are foreign-function code, so this crate carries its
own lint table with `unsafe_code = "deny"` instead of the workspace's
`forbid`. The library forbids `unsafe` itself, and only the platform modules
allow it, each `unsafe` block with a `// SAFETY:` comment.

```sh
cargo build -p tinydesktop-cursor --features overlay
cargo run -p tinydesktop-examples --bin cursor_demo -- calm 3
```

See [`docs/specs/virtual-cursor.md`](../../docs/specs/virtual-cursor.md).
