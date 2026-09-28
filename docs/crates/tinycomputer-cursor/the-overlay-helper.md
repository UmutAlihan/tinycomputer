# The overlay helper

By default, the picture the cursor becomes is drawn by a small separate
program: `tinycomputer-cursor-overlay`. This page covers what it is, why it
is a separate process rather than code inside the module, and the tiny
protocol the two speak to each other.

## Why a separate process

Two reasons, both in the crate's own docs:

1. A crash in drawing a pixel must never take the agent down. Window
   management code is exactly the kind of thing that can misbehave on a
   given OS version or graphics driver. If it does, the worst that happens
   is the cursor stops appearing; the agent keeps working.
2. The cursor has to sit above whatever the agent is currently looking at,
   whether that is a native application window or a browser tab, and has to
   keep drawing there as focus moves between them. A single always-on-top
   window, owned by its own tiny process, is a simpler way to get that than
   trying to draw inside every surface the agent might touch.

The helper is deliberately dumb: it only knows how to put pixels on screen.
Everything about what the cursor looks like (`sprite::Sprite`) and how it
moves over time (`animate::Animator`) is decided inside
`tinycomputer-cursor` itself and is identical on every platform, so the
cursor looks and moves the same way whether the underlying window is an
`NSWindow` on macOS or a layered Win32 window on Windows.

## Starting it

`ProcessOverlay::spawn`, in
[`src/screen/process.rs`](../../../crates/tinycomputer-cursor/src/screen/process.rs),
starts the helper the first time the cursor is actually shown, not
eagerly at startup. It looks for the binary, in order:

1. the path given directly (from `ScreenCursor::new`'s second argument, or
   the module's `cursor.overlay` config key);
2. the `TINYCOMPUTER_CURSOR_OVERLAY` environment variable
   (`ProcessOverlay::HELPER_ENV`), for a host that installs the helper
   somewhere of its own choosing;
3. `tinycomputer-cursor-overlay` next to the currently running executable;
4. `tinycomputer-cursor-overlay` on `PATH`.

If none of those exist, `spawn` fails, `ScreenCursor` marks the link
`Broken`, and the cursor simply never appears. Nothing about the rest of the
task is affected: `arrive` and `show` just return without waiting, as
though the pace were `off`. The link is not retried on every action, so a
missing helper does not cost anything beyond that first failed lookup.

Once started, commands are written to the helper's standard input by a
background thread, through a queue 8 commands deep. This means sending a
command never blocks the caller on the helper's pipe; if the helper falls
more than 8 glides behind, the newest glides simply get dropped rather than
holding up the action that triggered them. Dropping the child (ending the
task, or losing the last reference to the `ScreenCursor`) kills the helper
process.

## The protocol

Everything sent to the helper is one JSON object per line, defined as
`OverlayCommand` in
[`src/protocol/mod.rs`](../../../crates/tinycomputer-cursor/src/protocol/mod.rs).
There are exactly two commands:

```json
{"type":"glide","path":[[0,120.4,80.1],[16,124.7,81.9], ...],"appears":false}
{"type":"hide"}
```

- `glide.path` is a list of `[t_ms, x, y]` triples, coordinates rounded to
  a tenth of a point. The first entry is always where the glide starts, at
  `t_ms: 0`; the rest are the samples `human_path` produced (see
  [How a glide is planned](how-a-glide-is-planned.md)). `x` and `y` are
  global screen points, origin at the top-left of the primary display.
- `appears` tells the helper to fade the cursor in from nothing rather than
  assume it was already on screen; this is set on a cursor's very first
  glide, or on any glide that interrupts one that had already faded out.
- `hide` fades the cursor out. The next `glide` after a `hide` fades back
  in nearby, exactly as if the cursor had never been on screen.

A line the helper cannot parse as one of these two shapes is ignored rather
than crashing the process; `OverlayCommand::from_line` returns `None` for
it, and the reader just moves on to the next line.

## What happens with a command once it arrives

Inside the helper, `driver::Driver` (platform-independent, and unit tested
without any window at all) turns a stream of commands plus a clock into "what
to show right now":

- `Animator` interpolates along the current glide's path to find the
  cursor's position at the current time;
- it fades the cursor in over 150 ms when it "appears", and fades it out
  over the same 150 ms on `hide`;
- when a glide lands, it triggers a 450 ms landing pulse: a ring around the
  tip that grows and fades, drawn by the sprite's extra frames (see
  `sprite::Sprite::frame_for`);
- if nothing new arrives for 6 seconds after landing, it fades itself out on
  its own, so an idle cursor does not sit on screen forever waiting for a
  `hide` that never comes.

The platform code (`platform/macos.rs`, `platform/windows.rs`) just asks the
driver, roughly 60 times a second, "what do I show now?" and moves,
reframes, or fades a window accordingly. `platform/other.rs` runs the same
driver loop on any other target, just without ever drawing anything, so the
helper still behaves correctly (reads input, exits when it closes) even on a
platform with no overlay yet.

## Platform notes

- **macOS**: a borderless, transparent `NSWindow` at screen-saver level,
  set to ignore mouse events, join every Space, and never activate. The app
  runs with an accessory activation policy, so it never gets a Dock icon and
  never steals focus. The sprite is rendered at 2x for a sharp look on
  Retina displays.
- **Windows**: a layered, click-through, always-on-top, tool window (so it
  never shows up in the taskbar or Alt-Tab), similarly repositioned and
  redrawn on a timer.
- **Everything else**: no window. The helper still runs, reads commands, and
  exits cleanly when the module closes its input, so a task on an
  unsupported platform behaves exactly as if the pace were `off`.

Because the platform modules call into `objc2`/Win32 bindings marked
`unsafe`, this crate is the one place in the workspace with its own lint
table (`unsafe_code = "deny"` rather than the workspace's `forbid`). The
library itself still forbids `unsafe` outright; only `platform/macos.rs` and
`platform/windows.rs` allow it, and every `unsafe` block there carries a
`// SAFETY:` comment explaining exactly why it is sound.

## Building and trying it

```sh
cargo build -p tinycomputer-cursor --features overlay
cargo run -p tinycomputer-examples --bin cursor_demo -- calm 3
```

The first line builds the helper binary; the `overlay` feature is what
makes this crate produce a second artifact (`tinycomputer-cursor-overlay`)
rather than only the library. The second line runs the bundled example,
which drives a real `ScreenCursor` at the `calm` pace for 3 glides so you
can watch the motion described in
[How a glide is planned](how-a-glide-is-planned.md) on your own screen.
