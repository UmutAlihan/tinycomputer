# Virtual cursor

**Status:** Implemented. The overlay runs on macOS and Windows; Linux draws
nothing yet. **Owner:** tinycomputer maintainers.
**Plan:** [`../plans/virtual-cursor.md`](../plans/virtual-cursor.md).

## Problem

When the agent acts, a person watching sees nothing move. The engine clicks
an element directly, and the application or page just changes. A visible
second cursor that glides to where the agent is about to act makes a run easy
to follow. That cursor has to be:
- one cursor for the whole screen, so it can move from Mail into a browser
  tab and back without jumping;
- never the user's own pointer.

Input already works:
- agent-browser sends real CDP events;
- agent-desktop performs accessibility actions.

So the cursor is **purely cosmetic**.

## Goals

- **One cursor for the whole screen**, working in global screen points and
  shared by the desktop surface and every browser surface.
- **Human motion.** The cursor aims near, not at, the element's centre and
  reaches it with a curved stroke that overshoots and corrects, with a gentle
  wobble and a small tremor, in Fitts's-law time. It pulses when it lands.
- **Jev keeps choosing elements, never pixels.**
- **Zero effect on input.** The same engine commands go out with the same
  arguments, no input events are sent, the OS pointer is untouched, and
  nothing is injected into pages.
- **One look on every platform.** The sprite and the timing are decided in
  Rust; the platform code only puts pixels on screen.

## Non-goals

- Changing how any action is performed.
- Drawing where there is no screen, such as a headless browser.
- A Linux overlay, for now.

## Behavior

### `tinycomputer-cursor`

| Item | Behavior |
|---|---|
| `CursorPace` | `off`, `brisk`, `natural` (default), `calm`. These scale the tempo by ×0.6, ×1, ×1.6 |
| `VirtualCursor::glide(rect)` | aims inside the element's middle 60% × 50% and glides from where the cursor last landed. With no previous position it appears 240–420 px away and fades in |
| `ScreenCursor` | the shared cursor. `arrive(rect)` plans a glide, hands it to an `OverlaySink`, and returns as the cursor lands, so the surface's action fires in time with the landing pulse, like a click. The first glide through a freshly started helper adds 300 ms for it to appear. `show(rect)` does the same without waiting and returns the glide's duration. `hide()` fades the cursor out. Only a delivered glide is waited for: with no sink, a failing one, or a full queue, the action goes ahead at once |
| `OverlaySink` / `ProcessOverlay` | the default sink starts the helper on first use and writes one `OverlayCommand` per line to its stdin from a background thread, through an 8-deep queue that drops glides rather than block. A host UI can supply its own sink and draw the cursor itself |
| `OverlayCommand` | `{"type":"glide","path":[[t_ms,x,y],…],"appears":bool}` or `{"type":"hide"}` |
| `animate::Animator` | turns commands plus time into frames: path interpolation, a 150 ms fade in or out, a 450 ms pulse, and a fade-out after 6 s idle |
| `sprite::Sprite` | a violet arrow with a white outline and a soft shadow, plus 12 pulse frames, on a 64-point canvas with the tip at the centre. Frames are straight RGBA, supersampled 4×4. It also provides premultiplied BGRA and a stored-deflate PNG encoder |

**The path:**
- **Duration:** `110 + 120·log2(d/w + 1)` ms, jittered by ±8%, clamped to
  160–1100 ms, then scaled by the pace.
- **Overshoot:** reaches of 80 px or more overshoot 70% of the time, by at
  most 4% of the distance and at most 24 px.
- **Correction:** the last 20% of the travel time.
- **Bow:** σ = 10% of the distance, capped at 25% and at 120 px.
- **Wobble and tremor:** a wobble of up to 5 px, and a 0.6 px tremor at
  8–12 Hz. Both fade to zero at the ends.
- **Sampling:** 60 Hz.

### The overlay (`tinycomputer-cursor-overlay`)

This is the crate's binary, built with the `overlay` feature.

The helper reads commands on stdin and exits when stdin closes. It shows a
64-point window positioned at `tip − hotspot` and ticks the animator at
60 Hz.

- **macOS:** a borderless, transparent `NSWindow` at screen-saver level that
  ignores the mouse, joins every Space, and holds an `NSImageView`. The app
  runs as an accessory, so it has no Dock icon and never activates. The sprite
  is drawn at 2×.
- **Windows:** a `WS_EX_LAYERED | TRANSPARENT | TOPMOST | TOOLWINDOW |
  NOACTIVATE` popup, drawn with `UpdateLayeredWindow` from premultiplied BGRA.
  The helper is DPI-unaware, so it uses logical units.
- **Elsewhere:** it drains its input and draws nothing.

The platform modules are the only `unsafe` code in the workspace. The crate
sets `unsafe_code = "deny"` instead of `forbid`, the library forbids `unsafe`
itself, and every `unsafe` block carries a `// SAFETY:` comment.

### Surfaces

- **Desktop.** `Desktop::with_cursor`. Before `Click`, `Expand`, `Collapse`,
  `Check`, or `Uncheck` on a candidate with bounds (agent-desktop already
  reports screen points), the cursor glides there. Then the accessibility
  action runs unchanged.
- **Browser.** `BrowserSurface::with_cursor`. This applies only to a visible
  session, meaning headed or attached through `endpoint`. For the pointer
  operations above and targeted `TypeText`, the surface:
  1. reads the element's `boundingbox`;
  2. places the viewport on screen from `screenX/screenY` and the difference
     between the window's outer and inner size (side borders split evenly,
     toolbars above, 100% zoom);
  3. glides the shared cursor there.

  Then the engine's action runs unchanged.
- **Module.** The module builds a single `ScreenCursor` from `cursor` and
  shares it between the desktop and every task's browser.

### Configuration

`cursor` is either a pace name, or `{ "pace": …, "overlay": "/path/to/helper" }`.
Anything else is `ConfigFieldType { field: "cursor" }`. When `overlay` is
absent, the helper is looked for in this order:
1. `$TINYCOMPUTER_CURSOR_OVERLAY`;
2. beside the host executable;
3. on `PATH`.

Release packages ship the helper beside the module on macOS and Windows.

## Invariants

- The engine commands for every action are identical with the cursor on or
  off. The browser only adds read-only `boundingbox` and `evaluate` calls.
- A glide's last sample is its target, and sample times strictly increase.
  The aim point is always inside the element.
- The cursor never fails or changes an action. It only times it to the
  landing of a glide that was actually delivered; a missing, stuck, or failed
  overlay adds no delay.
- The helper never outlives the module: it exits on EOF, and its
  `ProcessOverlay` kills it on drop.

## Acceptance criteria

- **Crate tests:**
  - the path's endpoint, time, overshoot, and lateral bounds hold across
    seeds;
  - Fitts ordering;
  - aim containment;
  - glide continuity across targets;
  - animator fades, pulse, and idle hide;
  - sprite shape and pulse growth;
  - PNG and checksum reference values;
  - the protocol's wire form;
  - `ScreenCursor` with recording, failing, and missing sinks.
- **Surface tests:**
  - desktop and browser glides are sent, aimed inside the target's screen box, before
    the action;
  - no input is sent;
  - nothing is drawn headless, `off`, without a box, or for non-pointer
    operations.
- **Live:** `cursor_demo` tours twelve targets on macOS. The Windows overlay
  compiles and passes clippy for `x86_64-pc-windows-msvc`.

## Open questions

- A Linux overlay (X11 shape or override-redirect, or wlr-layer-shell on
  Wayland).
- Page zoom and unusual window chrome shift the browser cursor slightly. The
  exact viewport position is available through CDP's `Browser.getWindowBounds`
  combined with the page's layout metrics, if agent-browser exposes them.
