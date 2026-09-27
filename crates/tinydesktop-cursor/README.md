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

The overlay window itself is `crates/tinydesktop-cursor-overlay`; see
[`docs/specs/virtual-cursor.md`](../../docs/specs/virtual-cursor.md).
