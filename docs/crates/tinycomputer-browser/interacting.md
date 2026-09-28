# Clicking, typing, keys, and going back

Code: `crates/tinycomputer-browser/src/convert/mod.rs` (contract to engine
commands), `crates/tinycomputer-browser/src/surface/mod.rs` (the `Surface`
implementation a decision loop actually calls). Action definitions:
`crates/tinycomputer-bus/src/browser/action/types.rs`.

## Two ways to drive a session

You can drive a `Browser` session two ways, and both end up going through
the same `convert` functions and the same agent-browser engine:

- **Directly**, by calling `Browser::perform(session_id, Action)` with one
  of the `Action` variants (`Click`, `Fill`, `Type`, `Press`, `Scroll`,
  `Check`, ...). This is what a caller talking straight to the crate uses.
- **Through `BrowserSurface`**, which `tinycomputer-engine`'s decision loops
  drive with a small, surface-agnostic vocabulary
  (`JevOperation::Click`, `TypeText`, `Check`, `Scroll`, ...) that
  `BrowserSurface::execute` translates into the same `Action`s underneath.
  See [surface.md](surface.md) for that translation.

This page is mostly about the second path, because it is where the
interesting behavior lives. The direct path is a straightforward
`Action → JSON command` mapping in `convert.rs`, with no logic of its own.

## Targets: ref, selector, or locator

Every action that touches an element names a `Target`:

- `Target::Ref`: a ref from a snapshot (`e12`) or a sight mark (`seen:12`).
  Refs belong to the reading that produced them; acting on one after the
  page has re-rendered is refused rather than guessed at.
- `Target::Selector`: a raw CSS selector, matched against the first
  element found.
- `Target::Locator`: a semantic locator (role, text, label, placeholder,
  test id, alt text, or title), which agent-browser resolves itself through
  its `getbyrole`/`getbytext`/... family of commands. Locators only work
  with a handful of actions (click, fill, check, hover, get text); anywhere
  else, `convert::selector` refuses with `Error::InvalidInput` naming the
  ones that do.

`BrowserSurface` never builds a `Target::Selector` from a plain ref by hand.
`target()` in `surface/mod.rs` decides, per reference, whether it is a sight
mark (routed through `sight::selector`, which turns `seen:12` into
`[data-tc-seen="12"]`) or a tree ref (routed through `Target::reference`,
which becomes agent-browser's own `@e12` form).

## Clicking

`Action::Click` scrolls the element into view and clicks it. If another
element covers the click point (a consent banner, a modal, a sticky header),
agent-browser refuses the click and names the element in the way, rather
than clicking through it and reporting success on a click that landed
nowhere useful. That refusal message is what `reply::classify` turns into
`Error::NotActionable`.

### Click-through on result cards

A refusal because something is "covered" is not always the end of the
story. Plenty of result lists, flight search results, hotel cards, search
engine results, lay a transparent click layer, or the card's own visible
text, directly over the exact link or button you meant to click. From a
pixel's point of view, the card's own content is "covering" its own
control.

`BrowserSurface::click_through_own_card` (in `surface/mod.rs`) handles this
case specifically, and only this case. When a click comes back covered, and
the target has either a name or a sight mark, it:

1. Reads the target's bounding box.
2. Runs a small script (`SAME_CARD_JS`) that finds, among the elements
   stacked at that point, the one whose visible text or `aria-label`
   matches the target's name *exactly* (whitespace runs squashed to one
   space), or, for a sight-minted ref, the element carrying that exact
   mark.
3. Checks that whatever is actually on top at that point sits inside the
   *same* card (`li`, `[role="listitem"]`, `[role="row"]`, `article`,
   `[role="article"]`) as the target, and inside no dialog.
4. Only if both hold, dispatches the click as raw mouse events
   (`mouseMoved`, `mousePressed`, `mouseReleased`) at that point instead of
   asking agent-browser to click the element directly.

The exact-match requirement on step 2 exists because a short name like
"Select" turns up on nearly every card in a results page. Containment
alone is not enough to tell which card's "Select" you meant, so the code
insists on an exact match against the element's own shown text. Google
Flights is the real example that shaped this: each card's duration text
sits visually above its "Select flight" link, and Google renders each
flight card twice, once inside a hidden tab, so containment checks that
ignore layer boundaries would misfire.

A banner or dialog genuinely in front of the whole card still blocks the
click. Step 3's dialog check exists specifically so this shortcut never
becomes a way to click through something that is actually blocking the
page, only through a card's own content sitting on top of itself.

### Selecting tabs, radios, and options that ignore a trusted click

Some pages simply do not react to a synthetic click the moment they have
just rendered. Emirates' own trip-type tabs, freshly loaded, are the
observed case, while the exact same element's own `element.click()` works
fine once the page's own handlers are attached. After a successful click on
something the flow expects to *select* (a `tab`, `radio`, or `option` that
is not already marked selected or checked,
`surface::mod::selects_on_click`), the surface checks whether the click
actually left the element selected. If not, and the ref is a sight mark, it
presses the element again through the DOM (`SELECT_JS`) rather than giving
up. Selecting is idempotent, so pressing an already-selected element a
second time is harmless; a checkbox, which *toggles* rather than selects,
is never pressed this way.

## Typing text

There are two distinct actions, on purpose:

- `Action::Fill` clears a field and sets its value in one step. Fast, but a
  field that reacts to individual keystrokes (an autocomplete, a
  search-as-you-type box) never sees the input events a bulk assignment
  skips.
- `Action::Type` sends text as a real sequence of key events, either into a
  named target or, with no target, wherever the browser's focus currently
  is (which becomes agent-browser's `inserttext` command).

`BrowserSurface::execute` never lets `TypeText` land somewhere by accident.
With a target, it checks `takes_text` first, actually focusing the element
and then checking what ended up focused rather than what role the element
claims, and refuses with `NOT_A_TEXT_FIELD` rather than typing into, say, a
`div` that merely claims `role="textbox"`. Without a target (typing into
"whatever has focus", as into an autocomplete's own newly-opened, unnamed
input), it checks `focused_field_is_editable` first for the same reason: a
stale or unexpected focus must never silently receive typed text, including
a private value pulled from a fact.

`focused_field_is_editable`'s check descends into open shadow roots and
same-origin iframes before judging editability, so that focus checks still
work for the documented shadow-root/frame fallback case, not just the plain
top-level page.

## Pasting without a clipboard

`BrowserSurface::paste` types at a field's caret without touching an
actual system clipboard: focus the target, select everything already there
with `Ctrl+A`/`Cmd+A` when the target supports `SetValue`, then insert the
text. It refuses up front if the target does not turn out to be editable,
the same `NOT_A_TEXT_FIELD` case as typing.

## Pressing keys

`Action::Press` sends one key or chord (`Enter`, `Tab`, `Control+a`).
`BrowserSurface::press` takes a logical combo string like `cmd+a` or
`return` and spells it the way agent-browser expects (`browser_key` in
`surface/mod.rs`): the logical `cmd` becomes whichever platform key
`tinycomputer-core`'s keymap says is "select all" on this OS, arrow names
become `ArrowUp`/`ArrowDown`/etc., and anything else is capitalized to
match agent-browser's naming (`return`/`enter` → `Enter`,
`escape`/`esc` → `Escape`).

## Scrolling and waiting

`Action::Scroll` scrolls the page or a scrollable element within it, in
CSS pixels, defaulting to 300px per call; `ScrollDirection::Top`/`Bottom`
scroll a very large distance (`TO_THE_END`, one million pixels) to reach
either end regardless of document length. `Action::WaitFor` blocks on text
appearing anywhere on the page, an element reaching a given state
(`attached`, `detached`, `visible`, `hidden`), or a flat delay; `convert::
wait_for` refuses if none of the three is given.

`Surface::settle`, called before a decision loop reads the page again, does
two things: waits (bounded, up to `NETWORK_IDLE_MS` = 2 seconds) for the
page's network to go idle, then pauses an extra `SETTLE_MS` = 400
milliseconds regardless, giving a banner or menu that is mid-animation time
to finish closing. A page that polls constantly in the background never
goes properly idle, so the network wait is a cap, not a guarantee.

## Going back

`Surface::back` maps straight to `Action::Back`, which agent-browser
resolves to the browser's own history-back. There is no separate "forward"
exposed through the `Surface` trait (`Action::Forward` exists on the direct
`Browser::perform` path, just not wired into `BrowserSurface::execute`).

## Reading a value back

`Surface::read_value` reads what a field currently holds, trying
`inputvalue` first and falling back to `gettext`, useful for verifying what
actually landed in a field after a fill, especially one that went through
an autocomplete that rewrote it.
