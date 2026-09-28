# tinycomputer-cursor

This crate draws the little arrow that follows the agent around the screen.
It is the part of tinycomputer that a person actually watches: when the agent
is about to click something, this cursor glides over to it first, the same
way a person's own pointer would.

It is worth saying plainly, because it is easy to assume otherwise: **this
cursor sends no input.** It never moves your mouse, never sends a click, and
never touches a web page's DOM. The engine performs every action exactly as
it would if this crate did not exist. All this crate does is put a picture on
screen that happens to arrive at the same place, at the same moment.

## Who needs this

- **Anyone watching a run**, in a demo, a support call, or just curiosity
  about what the agent is doing. Without a cursor, buttons just change with
  no visible cause, which is confusing to watch.
- **Host applications** that embed tinycomputer and want their own look for
  the cursor, or want to draw it inside their own window instead of a
  system-level overlay. See [Using your own sink](using-your-own-sink.md).
- **Anyone debugging the "it looks robotic" kind of complaint**, since the
  motion this crate produces is deliberately human-shaped: a curved path,
  overshoot, a little wobble, a small tremor.

If you are trying to understand why the agent clicked the wrong thing, this
is not the crate to read: it never decides *what* to click, only how the
on-screen arrow gets there. For that, see
[how it decides](../../how-it-decides.md).

## How it fits

```text
tinycomputer-desktop ─┐
                       ├─► ScreenCursor::arrive(target) ─► OverlaySink ─► pixels on screen
tinycomputer-browser ─┘
```

Desktop and browser are two different surfaces, but there is only one cursor
for the whole screen. Both hand their target to the same `ScreenCursor`, in
global screen points, so the cursor can glide out of a Mail window and into a
browser tab without jumping. `ScreenCursor::arrive` plans the glide, sends it
to a sink to draw, and only returns once the cursor has actually landed, so
the click that follows happens right as the cursor's landing pulse shows on
screen.

By default the sink is a small helper process,
`tinycomputer-cursor-overlay`, that this crate also builds. It is a separate
process on purpose: a crash while drawing a pixel can never take the agent
down, and the helper can keep drawing on top of whichever application or
browser tab currently has focus.

## A tour of the crate

| Folder | What lives there |
|---|---|
| [`src/glide/`](../../../crates/tinycomputer-cursor/src/glide/) | `VirtualCursor` and the human-shaped path it plans: where inside an element to aim, and how to get there. See [How a glide is planned](how-a-glide-is-planned.md). |
| [`src/screen/`](../../../crates/tinycomputer-cursor/src/screen/) | `ScreenCursor`, the one shared cursor, and `ProcessOverlay`, which starts and feeds the helper. |
| [`src/protocol/`](../../../crates/tinycomputer-cursor/src/protocol/) | `OverlayCommand`, the tiny JSON message that tells a sink to glide or hide. |
| [`src/animate/`](../../../crates/tinycomputer-cursor/src/animate/) | `Animator`, which turns those commands plus a clock into "what to draw right now": fades, the landing pulse, hiding when idle. |
| [`src/sprite/`](../../../crates/tinycomputer-cursor/src/sprite/) | `Sprite`, the cursor's actual look, drawn once in Rust as pixels so it is identical on every platform. |
| [`src/pace/`](../../../crates/tinycomputer-cursor/src/pace/) | `CursorPace`: `off`, `brisk`, `natural`, `calm`. See [Paces and configuration](paces-and-configuration.md). |
| [`src/bin/tinycomputer-cursor-overlay/`](../../../crates/tinycomputer-cursor/src/bin/tinycomputer-cursor-overlay/) | the helper binary itself, built with the `overlay` feature. See [The overlay helper](the-overlay-helper.md). |

## Pages in this folder

- [Why a cursor at all](why-a-cursor-at-all.md): the problem this crate
  solves and why it has to be cosmetic.
- [How a glide is planned](how-a-glide-is-planned.md): the actual numbers
  behind the aim point, the overshoot, the bow, the wobble, and the timing.
- [Paces and configuration](paces-and-configuration.md): `off` / `brisk` /
  `natural` / `calm`, and the module's `cursor` config key.
- [The overlay helper](the-overlay-helper.md): the separate process that
  puts pixels on screen, and the protocol it speaks.
- [Using your own sink](using-your-own-sink.md): how a host with its own UI
  can draw the cursor itself instead of using the helper.

## See also

- [`docs/technical/specs/virtual-cursor.md`](../../technical/specs/virtual-cursor.md):
  the technical specification, with the plan and status.
- [How tinycomputer works](../../how-it-works.md): the bigger picture this
  crate is one small, purely visual part of.
- [Watching a run](../../watching-a-run.md): other ways to see what a task
  is doing besides the cursor.
- [Glossary](../../glossary.md): short definitions of terms used across these
  pages.
