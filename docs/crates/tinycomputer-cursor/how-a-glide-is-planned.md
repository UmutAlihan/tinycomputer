# How a glide is planned

A "glide" is one trip of the cursor: from wherever it last landed to the
element about to be acted on. It is planned as plain data (a list of timed
points) before anything is drawn, which is why it can be unit tested with no
display and no window at all: `Glide` and `human_path` in
[`src/glide/`](../../../crates/tinycomputer-cursor/src/glide/) never touch a
screen.

Everything below has a seeded random number generator behind it
(`tinycomputer_cursor::Rng`), so the same seed always produces the same
glide. That is what makes the crate's own tests, and its doctest, exact
rather than approximate.

## Step 1: pick a point inside the target, not its centre

People rarely click the exact geometric centre of a button, and essentially
never click its edge. `aim` (in
[`src/glide/aim.rs`](../../../crates/tinycomputer-cursor/src/glide/aim.rs))
picks a point drawn from a normal distribution centred on the target, then
clamps it to stay inside a margin from every edge:

- the point is kept within the middle 60% of the target's width and the
  middle 50% of its height (a margin of 20% off each side horizontally, 25%
  vertically);
- the spread of the underlying normal distribution is 12% of the width and
  15% of the height, so most picks land well inside that margin rather than
  right at its border;
- anything smaller than 2 points on either side (effectively a point, not a
  button) is aimed at dead centre instead, since there is nothing to spread
  across.

## Step 2: decide how long the reach takes

`travel_ms` (in
[`src/glide/path.rs`](../../../crates/tinycomputer-cursor/src/glide/path.rs))
follows Fitts's law: the farther the target and the smaller it is, the
longer a reach takes, on a logarithmic curve rather than linearly.

```text
difficulty = log2(distance / max(width, 8) + 1)
duration   = clamp(110 + 120 * difficulty, 160ms, 1100ms) * jitter * pace tempo
```

The `jitter` is a random multiplier centred on 1.0 (drawn from a normal
distribution with 8% spread, clamped to 0.85-1.2), so two reaches of the same
distance are never quite identical. The pace's tempo then scales the whole
thing: 0.6x for `brisk`, 1x for `natural`, 1.6x for `calm`. See
[Paces and configuration](paces-and-configuration.md) for what sets that.

## Step 3: decide whether the reach overshoots

A fast reach at a real target tends to fly slightly past it and then correct
back, rather than decelerating to a perfect stop. `human_path` reproduces
that only for reaches long enough that it would be noticeable:

- reaches of 80 points or more overshoot 70% of the time (a coin flip
  weighted that way, not always);
- when it does, the overshoot lands past the target by somewhere between
  0.5% and 4% of the distance, capped at 24 points either way, plus a small
  sideways drift;
- the first 80% of the travel time is spent on that overshooting stroke, and
  the last 20% on the short correction back onto the exact target point.

A reach that does not overshoot just travels straight to the target for the
whole duration.

## Step 4: bow the stroke sideways

A straight line between two points is not how an arm actually swings; there
is a gentle curve. The stroke's midpoint is displaced perpendicular to the
direct line by a random amount (normal distribution, standard deviation 10%
of the distance, capped at the smaller of 25% of the distance or 120
points), and a smooth curve (a quadratic Bezier through that displaced
midpoint) is used for the primary stroke instead of a straight line.

## Step 5: add wobble and tremor, then fade both to nothing at the ends

Two more effects ride on top of the curve, both scaled to fade to zero right
at the start and right at the end of the stroke, so the path still starts
and finishes exactly where `Glide::from` and `Glide::to` say it should:

- **Wobble**: a slow side-to-side sway, up to 5 points, 1.2 to 2.2 cycles
  over the primary stroke, shaped by a sine wave so it peaks in the middle
  and vanishes at both ends.
- **Tremor**: a much smaller, faster wiggle (physiological hand tremor sits
  around 8-12 Hz), independently on each axis, at up to about 0.6 points,
  present for the whole glide but faded in and out by another sine envelope
  over the total duration.

## Step 6: sample it at 60 Hz

The whole curve is a continuous function of time, but a renderer needs
discrete points. `human_path` samples it every 16 milliseconds (60 times a
second, matching an ordinary display's refresh), and always finishes with
one final sample exactly at the target point, so `glide.samples.last()` is
always `glide.to` with no floating-point drift.

## Starting from nowhere

The very first glide of a run has no "last position" to start from.
`VirtualCursor::entry` picks a point 240 to 420 points away from the first
target, in a random direction, and the resulting `Glide` is marked
`appears: true` so the renderer knows to fade the cursor in there rather
than assuming it was already visible.

## Putting it together

```rust
use tinycomputer_cursor::{CursorPace, Point, Rect, Rng, VirtualCursor};

let mut cursor = VirtualCursor::with_rng(CursorPace::Natural, Rng::seeded(1))
    .at(Point::new(40.0, 40.0));
let button = Rect::new(400.0, 300.0, 120.0, 32.0);
let glide = cursor.glide(button).expect("a natural cursor glides");

assert!(glide.samples.len() > 10, "the cursor travels, it does not jump");
assert!(button.contains(glide.to));
```

This is the crate's own doctest, from
[`src/lib.rs`](../../../crates/tinycomputer-cursor/src/lib.rs), and it is
compiled and run by `cargo test`, so it cannot silently drift from the code.

`VirtualCursor::glide` returns `None` outright when the pace is `Off`, which
is how "no cursor" is expressed all the way down: there is no glide to plan,
so there is nothing to draw.
