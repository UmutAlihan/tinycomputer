# The browser as a Surface

Code: `crates/tinycomputer-browser/src/surface/` (`mod.rs` holds
`BrowserSurface`, `operations.rs` its `Surface` implementation),
`crates/tinycomputer-browser/src/surface/cursor.rs`.

## What a Surface is, briefly

`tinycomputer-core::surface::Surface` is the trait tinycomputer's decision
loops drive: observe the current screen, act on something in it, read a
value, type, press a key, go back, navigate, launch. The desktop adapter
implements it over an accessibility tree on a real OS window; this crate's
`BrowserSurface` implements the same trait over a browser session, so a
flow written once can move between a desktop application and a web page
without knowing which one it is actually looking at.

## Lazy sessions

A `BrowserSurface` does not open a browser the moment it is created. It
holds a `Browser`, a `SessionOptions`, and an empty slot for a session id;
the first call that actually needs a page (`ensure_session`) opens one and
fills the slot. Every later call reuses the same session. This matters for
a decision loop that might build a surface speculatively and then decide
not to use it: nothing is spent until something is actually asked of it.

`BrowserSurface::close` tears the session down without blocking. Because it
might be called from async code, where a blocking call on this surface
would deadlock, it spawns the close onto the surface's own runtime handle
and returns immediately.

## Blocking on purpose

Every method on `Surface` is synchronous, but `Browser`'s own methods are
`async`. `BrowserSurface::block` bridges the two with
`handle.block_on(future)`. The flow runtime is expected to make these calls
off its own executor (`spawn_blocking`), so blocking here is the right
tradeoff rather than a smell.

## Perception: sight or tree

`BrowserSurface::with_perception` sets which way the surface reads a page:
`Perception::Sight` (the default) or `Perception::Tree`. `observe` tries
sight first when it is enabled, and only reads the accessibility tree
snapshot when sight is turned off, fails outright, or hits something it
cannot address (a shadow root, a large frame in front). See
[sight.md](sight.md) for what each of those actually does.

## Executing an operation

`BrowserSurface::execute` is the one big `match` that turns a
`JevOperation`, the vocabulary decision loops actually speak, shared with
the desktop surface, into one of the `Action`s this crate's `convert`
module knows how to send. A few of its branches carry real behavior beyond
a straight translation, and are covered on their own pages rather than
repeated here:

- `Click`, `Expand`, and `Collapse` all become `Action::Click`, followed by
  the click-through-own-card retry and the select-if-ignored retry
  described in [interacting.md](interacting.md#clicking).
- `TypeText` is checked against the currently focused element (with no
  target) or the named target's own editability (with one) before anything
  is sent, also in [interacting.md](interacting.md#typing-text).
- `Check`/`Uncheck` become `Action::Check` with `checked: true`/`false`.
- `Scroll` becomes `Action::Scroll` in the default direction, optionally
  scoped to a target.
- `Wait` becomes a flat half-second pause (`Action::WaitFor` with `ms:
  Some(500)`).
- `Drill` and `Widen`, moving a flow's attention into or out of a
  container, do not touch the page at all; they just report back which
  root a following `observe` should scope to.
- `Done` and `Blocked` are terminal: they report the intent resolved,
  again without touching the page.

Before any operation that uses the pointer, or types text at a named
target, the surface glides the shared cursor onto the target first
(`show_cursor`); see below.

## The cursor

There is exactly one on-screen cursor per agent, shared between the desktop
surface and every `BrowserSurface`, so it glides continuously from a
desktop application into a browser window and back rather than jumping
between two independent cursors. `BrowserSurface::with_cursor` attaches it;
without a call to that, a surface draws nothing (`ScreenCursor::off()`).

The cursor is purely cosmetic. Every action is carried out identically
whether or not anything is drawing it; nothing about how a click or a type
is performed depends on where the cursor picture happens to be.

`shows_cursor` decides whether there is anything to draw over in the first
place: only a session with a real window on screen counts, which means a
headed session, or any attached session (`SessionOptions::endpoint`) even
if it happens to be marked `headless`. An attached browser's window is on
someone else's screen already, headless or not, and reporting it that way
would just make the cursor never appear for a use case where it usefully
could.

Placing the cursor over a browser page needs one extra step a desktop
window does not: converting a coordinate the page reports (in viewport
pixels) into a coordinate on the actual screen. `cursor.rs`'s
`viewport_origin` reads the browser window's own screen position and outer
and inner sizes (via a tiny evaluated script) and works out where the
viewport's top-left corner sits on screen, assuming the window's borders
split evenly left and right and its toolbars sit above the page at 100%
zoom. That assumption is not exact. The doc comment is upfront that a
cursor landing "a few points off" is still a cursor a person watching would
read as being in the right place, but it is deliberately not trying to be
pixel-perfect, because nothing about how the action executes depends on it.

If the page will not say where its window is (`screen_bounds` returns
`None`: no box for the target, or the viewport script fails), the surface
simply draws nothing and proceeds with the action exactly as it would
otherwise; a missing cursor position is never a reason to fail an action.

## What `observe` returns

Whatever perception produced it, `observe` returns the same `Screen` shape
the desktop surface returns: `app` (forced to `"browser"` unless the caller
names something else), an optional `window` title, a `surface` name
(`window`, `sheet`, or `alert`), a list of `candidates` a flow can act on,
a list of `text_nodes` and `context` lines it can read but not act on, and
an `unexplored` list (always empty here: the browser surface has no notion
of unopened submenus the way a desktop tree does).

## Navigating and settling

`Surface::navigate` sends `NavigateRequest::new(url)` through
`Browser::navigate`, defaulting to `WaitUntil::Load`. `Surface::settle`,
called before a flow reads the page again after an action, waits, bounded,
for the network to go quiet, then pauses a further beat regardless, as
described in [interacting.md](interacting.md#scrolling-and-waiting).

## Cross-links

- [sight.md](sight.md): how `observe` actually reads the page.
- [interacting.md](interacting.md): how `execute` actually acts on it.
- [errors.md](errors.md): what a failed operation looks like as a
  `DesktopResponse`.
- [`../../seeing-the-screen.md`](../../seeing-the-screen.md): observation
  as a cross-cutting concept, desktop and browser both.
