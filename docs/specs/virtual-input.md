# Virtual input

**Status:** Accepted; browser and headed-desktop glide implemented, native
overlay and Windows input pending upstream. **Owner:** tinydesktop maintainers.
**Plan:** [`../plans/virtual-input.md`](../plans/virtual-input.md).

## Problem

When Jev acts, nothing that looks like a hand ever reaches the target:

- **Browser.** `Click` was agent-browser's `click`, which presses on the
  element's centre. With no pointer travelling there first, a page never saw
  `pointerover`, `mouseenter`, or `pointermove`. Hover-gated menus stayed shut,
  and controls that check pointer state ignored the click. Text went in with
  one `fill`, so per-key handlers (autocomplete, input masks) never ran.
- **Desktop.** Actions go through accessibility. That is right for a
  background agent, but in headed mode the real pointer only ever jumped.

We also want to *show* the agent's pointer when a UI is watching: a second
cursor with its own design, not the user's.

## Goals

- One shared crate, `tinydesktop-input`, that both surface adapters play
  their pointer and key input through.
- Jev keeps choosing **elements, never pixels**. Rust turns an element's
  bounds into an aim point and a path.
- **Human motion:**
  - an aim point near, not at, the centre;
  - a curved reach that overshoots and corrects, with a gentle wobble and
    a small tremor;
  - Fitts's-law timing;
  - a settle before pressing and a human hold.
- **Human typing:** one key per character, with log-normal gaps and faster
  common pairs.
- The user's own pointer is never taken over unless the host asked for headed
  mode.
- Deterministic and testable: gestures are data, reproducible from a seed.

## Non-goals

- Coordinates from Jev, or vision.
- Replacing accessibility actions on the desktop. The glide precedes them,
  and the action still lands through the accessibility tree.
- Typing long text key by key. Past 400 characters a person pastes, and so
  do we.

## Behavior

### The crate

`tinydesktop-input` holds no engine, no I/O, and no rendering.

| Item | Behavior |
|---|---|
| `MotionProfile` | `instant`, `brisk`, `natural` (default), `calm`; `instant` keeps the old behavior: centre aim, a single jump, text inserted at once |
| `VirtualMouse` | remembers its position. Its methods are `glide`, `approach` (reach and settle), `hover`, `click`, and `drag`, and each returns a `Plan` |
| `VirtualKeyboard` | `type_text`: KeyDown, hold, KeyUp, gap per character. `\n` becomes Enter and `\t` becomes Tab |
| `Plan` / `Step` | Move, Press, Release, KeyDown, KeyUp, Text, and Pause, with consecutive pauses merged |
| `InputSink` + `play` | an engine supplies the primitives, and `play` owns the timing |
| `Pacer` | measures a pause from when the previous primitive started, so an engine's own latency is not added on top |

Path model:
- **Duration:** `110 + 120·log2(d/w + 1)` ms (with w ≥ 8), jittered by ±8%,
  clamped to 160–1100 ms, then scaled by the profile's tempo (0.6 brisk,
  1.6 calm).
- **Overshoot:** reaches of 80 px or more overshoot 70% of the time, by at
  most 4% of the distance and 24 px.
- **Correction:** 20% of the time is spent correcting back onto the target.
- **Bow:** the primary stroke is a quadratic Bézier bowed sideways, with
  σ = 10% of the distance, capped at 25% and 120 px.
- **Timing and texture:** minimum-jerk timing, a wobble of up to 5 px, and
  an 8–12 Hz tremor of about 0.6 px. Every effect fades to zero at both ends.
- **Sampling:** 60 Hz. The last sample is exactly the target.

### Browser

`BrowserSurface::with_motion(profile)` sets the tempo. It defaults to
`natural`, and the module passes the configured `motion`.

- **Pointer operations** (Click, Expand, Collapse, Check, Uncheck) with a
  target:
  1. `boundingbox`, then an aim point.
  2. A glide: one `mousemove` per sample, `inputMode: instant`, so each is
     exactly one CDP `mouseMoved`.
  3. A settle.
  4. The engine's own `click` or `check`, which keeps its actionability and
     occlusion checks.

  With no box, the action proceeds without a glide.
- **Clicks through a result card's own click layer** are pressed by the
  virtual pointer: glide, then `mousedown`/`mouseup`.
- **`TypeText` into a target:**
  1. Click the field.
  2. Select all (`cmd+a` per platform).
  3. Type key by key: `keyboard` keyDown with `text`, then keyUp. Enter and
     Tab go through `press`, so their default actions fire.

  If any step fails, or the text is over 400 characters, the field is filled
  instead. `TypeText` with no target types where the focus is.
- **None of this touches the OS pointer.** CDP input is the page's own,
  second pointer.

### Desktop

`Desktop::with_motion(profile)` sets the tempo, and so does the `motion` key
in the module configuration. Invalid values are rejected with
`ConfigFieldType { field: "motion" }`.

- **Headed and not instant:** before a pointer operation on a candidate with
  bounds, the real pointer glides there with one `mouse_move` per sample,
  paced by `Pacer`. Then the accessibility action runs as before.
- **Headless:** the pointer is never moved.
- **A failed move** abandons the glide and forgets the position. The action
  still runs.
- **Clones of a `Desktop`** share one virtual mouse, because there is one
  real pointer.

### Rendering the virtual cursor

agent-desktop's native click-through cursor overlay is the renderer. Using it
needs three upstream changes (see [Open questions](#open-questions)):

- caller-supplied path samples, so the overlay and the real events follow the
  same path;
- a configurable helper executable, since a cdylib's `current_exe()` is the
  host;
- Windows support.

Until they land, nothing new is drawn.

## Invariants

- A path's last sample is its target, and sample times strictly increase.
- An aim point is always inside the element's middle 60% × 50%.
- `instant` reproduces pre-virtual-input behavior exactly.
- A headless desktop never moves the OS pointer.
- The glide never decides an action's outcome. The engine's action does.

## Acceptance criteria

- `cargo test -p tinydesktop-input` covers:
  - endpoint exactness, monotone time, and the overshoot bound over 500 seeds;
  - Fitts ordering;
  - aim containment;
  - typing that reproduces the text exactly;
  - the cadence band.
- Browser surface tests show:
  - more than 5 `mousemove`s before `click`, ending inside the box;
  - the next reach starting where the last ended;
  - keyDown/keyUp pairs for typed text;
  - fallback to `fill`.
- Desktop tests show that a glide is planned only when the desktop is
  headed, the profile is not instant, and the target has bounds.
- Live: in the headed lab, a hover menu on the browser fixture opens as the
  pointer arrives.

## Open questions

- **Upstream agent-desktop** (`lahfir/agent-desktop`, which has no
  tinyhumansai fork yet):
  - a streamed `mouse_path` input op, so each move is not a full command;
  - `CursorOverlayInstruction::with_path`;
  - `CursorOverlayConfig::helper_path`;
  - a pointer-position query, so the first headed glide starts from the real
    pointer rather than an entry point;
  - Windows `SendInput` and a layered overlay window. The Windows backend is
    a stub today, including its accessibility tree.
- A headed browser's virtual cursor needs viewport-to-screen mapping before
  the native overlay can draw it.
