# tinydesktop-cursor

The agent's on-screen cursor: a second pointer, drawn over the content, that
shows where the agent is acting. It is purely cosmetic — it sends no input and
never moves the user's pointer.

| Module | Holds |
|---|---|
| `glide/` | `VirtualCursor` and `Glide`: aim points inside an element, human paths (overshoot, correction, bow, wobble, tremor, Fitts timing) |
| `pace/` | `CursorPace`: `off`, `brisk`, `natural` (default), `calm` |
| `geometry/` | `Point` and `Rect` |
| `rng/` | a seeded generator, so a glide is reproducible |

A glide is data for a renderer to animate. The browser surface draws it in
the page; see [`docs/specs/virtual-cursor.md`](../../docs/specs/virtual-cursor.md).
