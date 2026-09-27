# Virtual cursor

**Status:** Accepted. The browser is implemented; the desktop overlay is
pending upstream. **Owner:** tinydesktop maintainers.
**Plan:** [`../plans/virtual-cursor.md`](../plans/virtual-cursor.md).

## Problem

When the agent acts, a person watching sees nothing move. The engine clicks
an element directly, and the page or application just changes. When a UI is
showing the run, a visible cursor showing where the agent is about to act
makes the run much easier to follow. That cursor must be a second pointer
with its own look, never the user's.

Input already works:
- agent-browser sends real CDP pointer and key events;
- agent-desktop performs accessibility actions, and moves the real pointer
  in headed mode.

So the cursor is **purely cosmetic**.

## Goals

- **A drawn cursor that glides onto each element before the action lands on
  it.**
  - It aims near, not at, the element's centre.
  - The path is a hand's: a curved stroke that overshoots and corrects, with
    a gentle wobble and a small tremor, in Fitts's-law time.
  - It pulses when it lands.
- **Jev keeps choosing elements, never pixels.** The aim and the path are
  computed in Rust from the element's bounds.
- **Zero effect on input.**
  - The same engine commands are sent, with the same arguments.
  - The cursor sends no events.
  - It never touches the OS pointer.
  - It stays out of accessibility snapshots and hit tests.

## Non-goals

- Changing how any action is performed.
- Drawing where nobody can see: in headless browser sessions, or in
  screenshots taken for a model.
- Desktop rendering in this change (see [Open questions](#open-questions)).

## Behavior

### `tinydesktop-cursor`

The crate holds no engine, no I/O and no rendering.

- **`CursorPace`:** `off`, `brisk`, `natural` (the default), or `calm`. The
  paces only change the tempo, by ×0.6, ×1.0 and ×1.6.
- **`VirtualCursor::glide(rect) -> Option<Glide>`:**
  - It aims inside the middle 60% × 50% of the element.
  - The glide starts where the last one landed. A cursor with no position
    appears 240–420 px away and fades in.
  - With `off`, it returns `None`.
- **`Glide`** holds `from`, `to`, timed `samples` (whose last one is exactly
  `to`), and `appears`.
- **The path:**
  - Duration is `110 + 120·log2(d/w + 1)` ms, jittered by ±8%, clamped to
    160–1100 ms, then scaled by the pace.
  - Reaches of 80 px or more overshoot 70% of the time, by at most 4% of the
    distance or 24 px. The last 20% of the time is spent correcting.
  - The stroke bows sideways with σ = 10% of the distance, capped at 25% or
    120 px.
  - A wobble of up to 5 px and an 8–12 Hz tremor of about 0.6 px both fade
    to zero at the ends.
  - It is sampled at 60 Hz.

### Browser

- **When it draws.** `BrowserSurface::with_cursor(pace)`, set by the module's
  `browser.cursor` key, draws only when the session is visible: headed, or
  attached to an existing browser through `endpoint`.
- **Which operations.** Before `Click`, `Expand`, `Collapse`, `Check`,
  `Uncheck`, and targeted `TypeText`, the surface does this:
  1. Reads the element's `boundingbox`.
  2. Plans a glide.
  3. Sends one `evaluate` that mounts the cursor, if it isn't already
     mounted, and animates the path with `requestAnimationFrame`.
  4. Waits for the glide's duration.

  The engine's action then runs unchanged.
- **How the cursor is mounted.** It is a `tinydesktop-cursor` element that is
  `aria-hidden` and `inert`, has `pointer-events: none`, and holds a closed
  shadow root at the highest z-index.
- **When drawing is skipped.** With no box, or when the page refuses the
  script, nothing is drawn and the action proceeds. A refused script also
  makes the cursor forget its position.

### Configuration

`browser.cursor` accepts `off`, `brisk`, `natural`, or `calm`. Any other
value is rejected with `ConfigFieldType { field: "browser" }`.

## Invariants

- The engine commands for every action are identical with the cursor on or
  off. The only additions are `boundingbox` and `evaluate`, both read-only
  apart from mounting the inert cursor.
- A glide's last sample is its target, and sample times strictly increase.
- The aim point is always inside the element.
- A headless, non-attached session draws nothing and sends no extra
  commands.

## Acceptance criteria

- Crate tests cover:
  - endpoint exactness and monotone time over 200 seeds;
  - the overshoot and lateral bounds over 500 seeds;
  - Fitts ordering;
  - aim containment;
  - continuity between glides.
- Browser tests show:
  - the cursor drawn before the click;
  - no `mouse*` input;
  - the path lands in the box;
  - the next glide starts where the last one landed;
  - nothing drawn in headless mode, at `off`, with no box, or for scroll and
    untargeted typing;
  - a refused script does not block the action.
- Live: in a headed lab run, the purple cursor glides onto each target
  before it is clicked.

## Open questions

- **Desktop.** agent-desktop already ships the same idea as a native macOS
  overlay: headless only, and it never moves the OS pointer. It launches its
  helper process from `current_exe()`, which inside this cdylib is the
  TinyBus host. It needs a configurable helper executable upstream
  (`lahfir/agent-desktop`) before tinydesktop can enable it per session.
  Windows has no overlay yet.
