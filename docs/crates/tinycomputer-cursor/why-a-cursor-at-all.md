# Why a cursor at all

tinycomputer does not click the way a person clicks. It reads the
accessibility tree, the same list of controls a screen reader uses, finds the
button it wants, and tells the operating system (or the browser, over CDP)
to activate that button directly. No mouse ever moves. There was never a
pixel position involved in the first place.

That is a good thing for reliability. A click aimed at a screen coordinate
can miss if something shifted since the last look; a click aimed at a named
control either lands on that control or fails safely. But it has a side
effect: if you are watching the screen while this happens, you see buttons
change state with no visible cause. A field fills in. A menu opens. Nothing
moved to explain it. It looks less like a person working and more like the
computer glitching.

`tinycomputer-cursor` exists to fix that impression without changing what
actually happens. It draws a second, purely decorative pointer that glides
to wherever the real action is about to land, arrives, and pulses right as
the click (or key press, or fill) happens underneath it. A person watching
sees something that reads as intentional: an arrow moves to the button, the
button responds. Nothing about the click itself changed. The crate's own
doc comment puts it plainly: this cursor "sends no input and never touches
the user's own pointer."

## What it deliberately does not do

- It does not decide what to click. That decision is made upstream, before
  the cursor is ever shown a target; see
  [how it decides](../../how-it-decides.md).
- It does not affect timing in a way that matters to the action. `arrive`
  waits for the glide to visibly land before returning, but if the sink is
  missing, broken, or too far behind to keep up, the action goes ahead at
  once rather than waiting on a picture nobody can see anyway.
- It does not run on Linux yet. The desktop and browser surfaces still work
  there; there is just no window drawn.

## Why the motion is not just "slide from A to B"

A cursor that snapped in a straight line at constant speed would look just
as robotic as no cursor at all, maybe more so. People do not move a mouse
that way: they overshoot slightly and correct, their hand wobbles a little,
and how long the whole reach takes depends on the distance and how small the
target is (this is Fitts's law, a long-studied result about aimed
movement). `tinycomputer-cursor` models all three, aiming for a cursor that
reads as a hand rather than a plotted line. The exact shape of that path,
with the numbers used at each step, is in
[How a glide is planned](how-a-glide-is-planned.md).

## Why it is its own crate

The cursor is shared by two very different surfaces: `tinycomputer-desktop`
(clicking inside real applications) and the browser surface (clicking inside
a Chrome tab over CDP). Both surfaces convert their target into the same
global screen coordinates and hand it to one `ScreenCursor`, so the same
arrow can leave a Mail window and land on a web page without jumping or
resetting partway. Keeping the planning, the look, and the timing in one
place, separate from either surface, is what makes that possible: neither
surface needs to know anything about how a human hand moves, and the cursor
never needs to know anything about accessibility trees or CDP.
