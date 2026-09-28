# Paces and configuration

## The four paces

`CursorPace`, in
[`src/pace/mod.rs`](../../../crates/tinycomputer-cursor/src/pace/mod.rs), is
the one knob that controls the cursor's speed and whether it is drawn at
all:

| Pace | Config name | Tempo multiplier | What it looks like |
|---|---|---|---|
| Off | `off` | (not drawn) | No cursor is planned or shown. Every method that would glide returns `None` instead. |
| Brisk | `brisk` | x0.6 | A practised user in a hurry: the same path, played back faster. |
| Natural | `natural` | x1.0 | An attentive user at an ordinary pace. This is the default. |
| Calm | `calm` | x1.6 | A careful user, slow enough to follow comfortably on a call or a demo. |

Every pace but `Off` follows the exact same curved, overshooting,
wobbling, trembling path described in
[How a glide is planned](how-a-glide-is-planned.md). The paces do not change
the *shape* of the motion, only how long it takes: the tempo multiplier is
applied straight onto the travel-time formula in `travel_ms`. `Off` is not a
"very fast" pace; it disables the cursor entirely, at the `VirtualCursor`
level, so no glide is ever computed and nothing is ever sent to a sink.

`CursorPace::default()` is `Natural`. Parsing is case- and whitespace
insensitive (`" Brisk ".parse::<CursorPace>()` works), and any other string
is a parse error rather than a silent fallback, so a typo in configuration
is caught rather than quietly changing behaviour.

## The module's `cursor` configuration key

When tinycomputer is loaded as a module, its host configuration can carry a
`cursor` key. This is read by `cursor_config` in
[`crates/tinycomputer/src/tinybus_module/dispatch.rs`](../../../crates/tinycomputer/src/tinybus_module/dispatch.rs),
not by this crate itself, since deciding what a host's configuration means
is the module's job, not the cursor's.

The key accepts two shapes:

**Just a pace name**, as a string:

```json
{ "cursor": "calm" }
```

**An object**, with an optional `pace` and an optional `overlay` path to a
helper binary installed somewhere other than the usual places:

```json
{
  "cursor": {
    "pace": "brisk",
    "overlay": "/opt/tinycomputer/bin/tinycomputer-cursor-overlay"
  }
}
```

If `cursor` is left out entirely, the cursor glides at `natural` pace, using
the helper found wherever `ProcessOverlay::locate` looks (covered in
[The overlay helper](the-overlay-helper.md)). Any other shape, including a
pace name that does not parse, is a configuration error the module refuses
to start with, rather than something it silently ignores.

Setting `pace` to `"off"` (either as the bare string or inside the object)
produces a `ScreenCursor` built with `ScreenCursor::off()`: no helper is ever
started, and every call to `arrive` or `show` returns immediately having
done nothing.

## Configuring it directly in code

Outside the module, or in a host embedding this crate directly, the same
choices are made through the crate's own API rather than JSON:

```rust
use tinycomputer_cursor::{CursorPace, ScreenCursor};

// The default sink: starts tinycomputer-cursor-overlay on first use.
let cursor = ScreenCursor::new(CursorPace::Calm, None);

// A specific helper binary instead of the usual search order.
let cursor = ScreenCursor::new(CursorPace::Natural, Some("/opt/bin/tinycomputer-cursor-overlay".into()));

// No cursor at all.
let cursor = ScreenCursor::off();
```

`ScreenCursor::with_sink` is the third option, for a host that wants to draw
the cursor itself instead of using the helper process; see
[Using your own sink](using-your-own-sink.md).
