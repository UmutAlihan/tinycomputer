# Using your own sink

The overlay helper described in
[The overlay helper](the-overlay-helper.md) is the default way the cursor
gets drawn, but it is not the only way. A host that already has its own UI,
for example an embedding application with its own window showing the agent's
screen, can draw the cursor itself instead of spawning a separate process.

## The `OverlaySink` trait

Everything `ScreenCursor` needs from a renderer is one method:

```rust
pub trait OverlaySink: Send {
    fn send(&mut self, command: &OverlayCommand) -> std::io::Result<()>;
}
```

`OverlayCommand` is the same two-shape protocol the process helper speaks
(`glide` and `hide`, described in
[The overlay helper](the-overlay-helper.md)), just delivered as a Rust value
instead of a line of JSON. A sink gets exactly the same information the
process helper's stdin gets: timed positions in global screen points, and
whether the cursor is appearing from nothing.

The two error cases `send` can report matter for how `ScreenCursor` behaves:

- return `Err` with `std::io::ErrorKind::WouldBlock` when your sink is busy
  and had to drop this particular command. The cursor keeps the sink around
  and simply does not wait for that glide to land; the next command still
  gets a chance.
- return any other `Err` when your sink is gone for good (a closed window,
  say). `ScreenCursor` treats the link as broken and stops trying to draw
  until the process using it is recreated.
- return `Ok(())` once the command has been handed off. `ScreenCursor` does
  not require you to have actually painted anything by the time `send`
  returns, only that you have accepted the command.

## Wiring it up

```rust
use tinycomputer_cursor::{CursorPace, OverlayCommand, OverlaySink, ScreenCursor};

struct MySink {
    // however your UI wants to receive the path: a channel, a shared
    // buffer the render loop reads, whatever fits your architecture.
}

impl OverlaySink for MySink {
    fn send(&mut self, command: &OverlayCommand) -> std::io::Result<()> {
        match command {
            OverlayCommand::Glide { path, appears } => {
                // path is [t_ms, x, y] triples; forward it to your renderer.
            }
            OverlayCommand::Hide => {
                // fade your own cursor out.
            }
        }
        Ok(())
    }
}

let cursor = ScreenCursor::with_sink(CursorPace::Natural, Box::new(MySink { /* ... */ }));
```

From here on, `cursor.arrive(target)` and `cursor.show(target)` behave
exactly as they would with the process helper: `ScreenCursor` still plans
the human-shaped glide (see
[How a glide is planned](how-a-glide-is-planned.md)), still waits for
`arrive` to report the glide as landed before returning, and still forgets
its position if your sink ever reports it is gone.

## What you get for free, and what you still have to build

`tinycomputer-cursor` still owns the parts that are the same on every
platform: the aim point inside a target, the curved and timed path, and (if
you want it) the pulse-and-fade timeline in `animate::Animator` and the
pixels in `sprite::Sprite`. You are free to reuse either or both of those
from inside your own sink; nothing about `OverlaySink` requires you to
reimplement the motion or the look, only to decide how the result actually
reaches your window.

What you do have to build yourself is the actual drawing: a window, a
canvas element, or whatever surface your host already renders the agent's
screen into. This is the same boundary the process helper's own
`platform/` modules sit on, just moved into your codebase instead of a
separate binary.

## Testing without any window at all

Because `ScreenCursor::without_waiting` exists specifically to make tests
run at full speed (skipping the real-time wait `arrive` would otherwise do),
and because `OverlaySink` is a plain trait, a test double that just records
the commands it received is enough to assert your integration is wired up
correctly, with no display, no helper process, and no timing dependency:

```rust
use std::sync::{Arc, Mutex};
use tinycomputer_cursor::{CursorPace, OverlayCommand, OverlaySink, Rect, ScreenCursor};

#[derive(Default, Clone)]
struct Recording(Arc<Mutex<Vec<OverlayCommand>>>);

impl OverlaySink for Recording {
    fn send(&mut self, command: &OverlayCommand) -> std::io::Result<()> {
        self.0.lock().unwrap().push(command.clone());
        Ok(())
    }
}

let recording = Recording::default();
let cursor = ScreenCursor::with_sink(CursorPace::Natural, Box::new(recording.clone()))
    .without_waiting();
cursor.arrive(Rect::new(100.0, 100.0, 40.0, 20.0));
assert_eq!(recording.0.lock().unwrap().len(), 1);
```
