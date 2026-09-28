# Desktop as a surface

## Two different callers, two different jobs

`Desktop` gets called in two quite different ways.

The first is a caller (a host, a script, a test) that already knows exactly
what it wants: "click this ref," "read that field." It sends a request, gets
back a `DesktopResponse`, and decides what to do next itself. Everything on
the earlier pages of this section (observation, interaction, permissions,
errors) is written from that point of view.

The second is a Jev decision loop, the part of `tinycomputer-engine` that is
actually deciding what to click next based on what is on screen. It does not
call `Desktop::snapshot` and `Desktop::click` directly with knowledge of the
contract's request types. It calls a much smaller, more abstract interface:
"show me what's here," "act on this thing I picked," "read this thing's
current value," "type this in." That abstract interface is
`tinycomputer_core::surface::Surface`, and it is shared with the browser
adapter, so a decision loop does not need to know or care whether it is
driving a desktop application or a web page.

`crates/tinycomputer-desktop/src/surface/mod.rs` is where `Desktop`
implements `Surface`, with observation, acting, and pasting in
`observation.rs`, `act.rs`, and `paste.rs` beside it. This page describes what that implementation actually
does, since it is doing considerably more than forwarding calls.

## `observe`: turning a snapshot into a `Screen`

`Surface::observe` is what a decision loop calls to see the current state of
an application. Underneath, it calls `Desktop::snapshot`, but it does three
things a raw snapshot does not:

1. **Resolves ambiguity automatically.** If an application has several
   windows and the snapshot comes back `AMBIGUOUS_TARGET`, `observe` looks at
   the candidates the error reported, or falls back to asking
   `list_windows` for the front one, and retries the snapshot scoped to that
   window. A decision loop never sees the ambiguity; it sees the front
   window, the same way a person glancing at their screen would.
2. **Falls back to a shallower snapshot on failure.** If the first snapshot
   attempt fails outright, `observe` retries once with a shallow tree
   (`max_depth: 4`) rather than giving up, since a smaller ask sometimes
   succeeds where a larger one does not.
3. **Detects the surface it actually landed on.** If the snapshot's own tree
   contains a sheet, alert, menu, or popover near the top, `observe`
   re-snapshots scoped to that overlay surface specifically, so the decision
   loop is looking at the dialog that is actually blocking the window, not
   the window underneath it.

The result is a `Screen`: an application name, a window title, the surface
kind, a flat list of actionable candidates, a list of unexplored subtrees
(where the tree walk stopped early), and a list of static text kept as
context.

## What gets kept as context, and what does not

Not every ref-less piece of text on screen is useful to hand a decision
model as context. Labels, headings, and status text ("New Message," "Now
playing") say where the loop currently is, and those are kept, up to 60
lines of up to 160 characters each. But text that mirrors the *contents* of
a field, such as a mail body rendered inside a web area, is deliberately
excluded from that shared context. That content is field data, not screen
chrome, and it stays out of what every request sees regardless of settings;
a decision loop reads field contents through a separate, gated path instead.
This split exists because "screen text is data, never instructions" is a
rule the whole engine follows, and mixing field content into general
context would make it easy to lose track of which text is safe to treat as
navigational and which is not. See
[Safety and privacy](../../safety-and-privacy.md) for the wider version of
this rule.

## `execute`: turning a decision into an action

Once a decision loop has picked a candidate and an operation (click, type,
check, expand, scroll, wait, and so on), `Surface::execute` turns that into
the matching `Desktop` method call. If a screen cursor is attached (see
`Desktop::with_cursor`), it also moves the on-screen cursor to the target's
bounds first, so a person watching the run sees the cursor glide to what is
about to be clicked before the click lands. The cursor is cosmetic: the same
accessibility action happens whether or not a cursor is attached, and the
real mouse pointer never moves.

## `paste`: typing text without keystroke-by-keystroke risk

`Surface::paste` is the path used to enter longer or sensitive text (a fact
value, for instance) without sending it through the keyboard one character
at a time. It:

1. Reads whatever the pasteboard currently holds, so it can be restored
   afterward.
2. Focuses the target field (falling back to a click if focus alone does
   not work).
3. Stages the new text on the pasteboard.
4. Selects the field's existing contents first if the field supports
   replacing its value outright, so the paste replaces rather than appends.
5. Sends the paste keystroke, using the platform-appropriate modifier (see
   below).
6. Waits briefly for the field to settle, then restores whatever the
   pasteboard held before step 1, or clears it if it held nothing readable.

If the pasteboard restoration itself fails, the field's own success is
still reported truthfully (the text did arrive), but the response gains a
`clipboard_restored: false` flag so a caller that checks can see the user's
prior clipboard was not put back.

## Platform-neutral shortcuts

A flow or the `paste` path above writes shortcuts using a platform-neutral
`cmd` modifier, for example `cmd+a` to select all. The engine's own combo
parser maps `cmd` literally onto the Meta key, which is Command on macOS but
the Windows or Super key everywhere else. Left unmapped, `cmd+n` would open
the Start menu on Windows instead of a new document. `platform_combo`
rewrites `cmd` to `ctrl` on non-macOS platforms before the combo reaches
`Desktop::press`, so the same flow logic produces the right shortcut on
whichever platform it runs on.

## Reading a value back

`Surface::read_value` reads a field's current value, trying the `Value`
property first and falling back to `Text`, since a plain form field answers
with a value but a rich-text area often has none and answers with its text
instead.

## Where this lives in the code

All of the above is in
[`crates/tinycomputer-desktop/src/surface/`](../../../crates/tinycomputer-desktop/src/surface/mod.rs):
`mod.rs` holds the `Surface` implementation, `observation.rs` the snapshot
parsing, `act.rs` the operations, and `paste.rs` the clipboard paste. It is the
busiest module in the crate for a reason: it is where "a pile of
accessibility nodes" becomes "the small, curated view a decision loop
reasons over," and that translation carries real judgment calls, not just
type conversion. See
[How it decides](../../how-it-decides.md) for what the decision loop itself
does with a `Screen` once it has one, and
[Seeing the screen](../../seeing-the-screen.md) for the equivalent story on
the browser side.
