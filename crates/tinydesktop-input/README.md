# tinydesktop-input

A virtual mouse and keyboard that move like a person, shared by the desktop
and browser surfaces.

| Module | Holds |
|---|---|
| `mouse/` | `VirtualMouse`: aim points inside an element, human reach paths (overshoot, correction, bow, wobble, tremor, Fitts timing), click / hover / drag gestures |
| `keyboard/` | `VirtualKeyboard`: one key per character, log-normal cadence, faster common pairs, pauses after punctuation |
| `plan/` | `Plan` and `Step`, the `InputSink` an engine implements, and `play` |
| `profile/` | `MotionProfile`: `instant`, `brisk`, `natural` (default), `calm` |
| `rng/` | a seeded generator, so a gesture is reproducible |

Gestures are planned as data and played against an `InputSink`, so the
humanization lives here once and each engine supplies only primitives. The
crate has no engine, no I/O, and no rendering; see
[`docs/specs/virtual-input.md`](../../docs/specs/virtual-input.md).
