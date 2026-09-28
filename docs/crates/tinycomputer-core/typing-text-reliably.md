# Typing text reliably

Source: [`crates/tinycomputer-core/src/surface/delivery.rs`](../../../crates/tinycomputer-core/src/surface/delivery.rs).

Setting a text field's value sounds like it should be one operation: tell
the accessibility API "this field now holds this text" and move on. In
practice it is not that simple, on either surface. Some fields quietly ignore
a set-value call: a rich-text email body, or a "to" field that turns each
address you type into a little chip (a *token field*). If tinycomputer just
trusted that the set-value call worked, a run could sail on believing a
recipient was filled in when the field is actually still empty.

`deliver_text(surface, app, target, text)` exists so that a caller never has
to think about this. It is the one function in this crate that calls back
into a `Surface`, because verifying delivery genuinely needs to read the
field back after acting on it.

## What it actually does

1. Call `execute(TypeText, target, text)` — the fast, headless path.
2. Read the field back (`read_settled`, described below).
   - If the read-back value holds the text, report success:
     `{"path": "set_value", "verified": true}`.
   - If the read-back value is only attachment tokens (see below), report
     `{"path": "set_value", "verified": false}` — delivered, but a field like
     this can never be read back and compared, so it is reported as
     unverified rather than as a failure.
   - If nothing could be read at all, same thing: unverified, not failed.
3. Otherwise, fall back to `paste(app, target, text)` — put the text on the
   clipboard, focus the field, paste, and restore whatever the clipboard held
   before. Read the field back again the same way.
4. If even that read-back does not hold the text, return a real error:
   `TEXT_NOT_DELIVERED`.

The reply always says *which* path actually delivered the text
(`set_value` or `paste`) and whether it could be verified. That distinction
matters to anyone debugging a run: "the field says it worked, but I could
not check" is a meaningfully different situation from "I checked, and it
worked."

## Giving the field a moment to settle

`read_settled` reads the field once immediately. If that already holds the
text (or the field cannot be read at all), it stops there. Otherwise it calls
`Surface::settle()` — the hook a surface uses to wait for whatever "give the
page a beat" means on its own platform — and reads once more. This is why
`Surface::settle()` exists as a method at all rather than a fixed sleep
somewhere in this crate: a token field that turns a typed address into a chip
needs a moment to do that; a plain text field does not, and a fixed delay
would either be too short for one or wastefully long for the other.

## `holds` and `tokenized`

Two small pieces of logic decide whether a read-back value "counts":

- `holds(held, text)` collapses whitespace on both sides and checks that
  `held` contains `text`. Whitespace is collapsed because a rich-text editor
  is free to rewrap lines or turn a newline into a paragraph break without
  changing what was actually written, and neither of those should count as a
  delivery failure.
- `tokenized(held)` recognizes a token field's read-back: it is made up
  entirely of the object-replacement character (`U+FFFC`), commas, and
  whitespace — the shape a mail client's "to" field takes once every typed
  address has become an attachment chip. A tokenized value can never be
  compared against the text that produced it, so `deliver_text` treats it as
  a special case rather than as a mismatch.

```rust
assert!(holds("Dear Sam,\n\nThanks", "Dear Sam, Thanks"));
assert!(tokenized("\u{fffc}, \u{fffc}"));
```

## Why this matters for safety, not just correctness

A flow step that types a passenger's name or a promo code and moves on
without verifying it landed is exactly the kind of silent failure that later
shows up as a booking made under the wrong name. `deliver_text` is the single
place that guarantees a caller either gets confirmation the text arrived, an
honest "I could not verify this," or a real error — never a false "done."
Every flow step that types something meaningful goes through this function
rather than calling `Surface::execute(TypeText, …)` directly. See
[how tinycomputer decides](../../how-it-works.md) for where a flow step sits
in the bigger loop, and
[safety and privacy](../../safety-and-privacy.md) for the broader set of
guarantees a run gives you before it acts.
