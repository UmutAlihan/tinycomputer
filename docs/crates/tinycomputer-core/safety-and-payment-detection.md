# Safety and payment detection

Source: [`crates/tinycomputer-core/src/safety/mod.rs`](../../../crates/tinycomputer-core/src/safety/mod.rs).

This module holds the checks that stop a run before something happens that
cannot be undone, whatever a model decided along the way. It runs
independently of Jev: even if a decision model were somehow convinced that
clicking "Confirm and Pay" was fine, this code would still stop the click.
For the plain-language version of why that separation matters, see
[safety and privacy](../../safety-and-privacy.md); this page is the how.

## `consequence`: what pressing a button commits to

`consequence(label)` reads a control's visible label and classifies what
pressing it would commit the user to:

- `Consequence::Reversible` — nothing that cannot be undone or navigated away
  from. This is the default: most buttons ("Book", "Continue", "Next") are
  reversible, because a booking flow keeps leading to more forms before
  anything is actually charged.
- `Consequence::Irreversible` — a message sent, data deleted, a post
  published, a reservation confirmed. Needs explicit human approval.
- `Consequence::Payment` — money changes hands. Never made on its own; always
  stopped at, or held for approval.

```rust
use tinycomputer_core::{Consequence, consequence};

assert_eq!(consequence("Pay ₹6,840"), Consequence::Payment);
assert_eq!(consequence("Send"), Consequence::Irreversible);
assert_eq!(consequence("Book"), Consequence::Reversible);
assert_eq!(consequence("Continue to traveller details"), Consequence::Reversible);
```

The check is a whole-word match against two short word lists (`PAYMENT` and
`IRREVERSIBLE`), not a machine-learned classifier — a control's label is
lower-cased, punctuation is turned into spaces, and each list is checked as a
whole-word phrase so that, say, "Payment history" does not accidentally
match "pay" as a substring of something else. An **empty** label is treated
as `Irreversible`: a button that says nothing about itself cannot be proven
harmless, so the safe default wins.

### Counters are a deliberate exception

A stepper's minus button often says something that sounds irreversible —
"Remove Adult, 2 Adult Remaining" reads a lot like "Remove" on its own would.
But all it does is lower a number that the plus button right next to it
would put back. `adjusts_a_count(label)` recognizes this shape: a decrease
word (`remove`, `decrease`, `reduce`, `minus`, `subtract`) immediately
followed by something the booking is counting (`adult`, `passenger`, `room`,
`guest`, and their plurals), as long as the label is not actually about
"details" or "information" for that thing. `consequence` checks this before
falling back to the irreversible list, so:

```rust
use tinycomputer_core::consequence;
use tinycomputer_core::Consequence;

assert_eq!(consequence("Remove Adult"), Consequence::Reversible);
```

## `payment_evidence`: is this page itself a payment step

A payment step does not always say "Pay" on the button. Sometimes the button
says "Continue" and the only tell is the page around it: a card number
field, `cc-*` autocomplete attributes, or a checkout URL. `payment_evidence`
looks at the page rather than just the one control:

```rust
use tinycomputer_core::{FieldHint, payment_evidence};

let card = FieldHint { autocomplete: Some("cc-number".into()), ..FieldHint::default() };
assert!(payment_evidence("https://ota.test/traveller", &[card], &[]).is_some());
assert!(payment_evidence("https://ota.test/results", &[], &["Book"]).is_none());
```

The rule: a card field is enough on its own — either a `cc-*` autocomplete
attribute, or a label/name matching card wording ("card number", "cvv",
"expiry date", "upi id", …). Without a card field, a payment-shaped URL path
(`payment`, `billing`, `checkout`, …) *and* a payment-classified control
together also count. Neither alone is enough: a URL with "pay" in the path
but no payment control (or vice versa) does not trigger it, because either
one alone is too easy to false-positive on (a "How to pay" help page has the
word; a "Pay it forward" charity link has a payment-sounding button).

Every match comes back with a plain-word reason (`"a card field (autocomplete cc-number)"`,
`"a card field (cvv)"`, `"a payment address with a \"Book\" control"`), which
is what a caller shows a person at a checkpoint rather than a bare boolean.

## `screen_payment_evidence`: the same check without a web page

Desktop applications have no URL and no HTML `autocomplete` attribute, so
`payment_evidence` on its own cannot run there. `screen_payment_evidence(screen)`
builds the same kind of evidence out of a plain `Screen`: it looks at fields
that may take input (marked as such, or carrying no role and no actions —
meaning nothing says they cannot), reads their labels, and separately looks
at *nearby* text (within five nodes in document order — enough room for the
usual wrapper elements between a label and its field, not enough to reach a
footer far down the page) for strong card wording like "CVV" or "card
number" that a promotional line would never use ("save 10% with your credit
card" does not count; that phrase is deliberately excluded from the strong
list).

```rust
use tinycomputer_core::surface::{Candidate, Screen};
use tinycomputer_core::screen_payment_evidence;

let field = |name: &str| Candidate {
    role: "textbox".to_owned(),
    name: Some(name.to_owned()),
    available_actions: vec!["SetValue".to_owned()],
    ..Candidate::default()
};
let mut screen = Screen {
    app: "browser".to_owned(),
    window: None,
    surface: "window".to_owned(),
    candidates: vec![field("Card number")],
    context: Vec::new(),
    unexplored: Vec::new(),
    text_nodes: Vec::new(),
};
assert!(screen_payment_evidence(&screen).is_some());

screen.candidates = vec![field("Traveller name")];
screen.context = vec!["Pay less with your credit card".to_owned()];
assert!(screen_payment_evidence(&screen).is_none());
```

This is the check every surface's "am I about to click something
destructive" gate runs, so the same rule applies whether the run is filling
in a native macOS form or a checkout page in a browser tab.

## `human_needed`: things only a person can get past

Some obstacles are not about safety at all; they are about the run simply
being unable to continue without a person. `human_needed(texts)` scans a
page's visible text for a captcha, a one-time password prompt, two-factor
authentication, or a sign-in wall, and returns what a person needs to do
about it in plain words:

```rust
use tinycomputer_core::human_needed;

let page = ["Security check".to_owned(), "Verify you are human".to_owned()];
assert_eq!(human_needed(&page).as_deref(), Some("prove you are human"));
assert_eq!(human_needed(&["Flights from Delhi".to_owned()]), None);
```

This is what turns "the run got stuck for no obvious reason" into "the run
paused and told you exactly what it needs from you." See
[rescue](../../rescue.md) for what happens with that pause at the task
level, and [giving it a task](../../giving-it-a-task.md) for what a caller
sees when a task stops for one of these reasons.

## Where these checks are enforced

None of these functions click anything or block anything by themselves —
they only classify. The engine's flow runtime is what actually reads a
`Consequence` or a `PaymentEvidence` before a destructive click and decides
whether to proceed, hold for approval, or refuse. See
[`docs/technical/architecture.md`](../../technical/architecture.md)'s
"Safety, in one place" section for how that wiring works, and
[safety and privacy](../../safety-and-privacy.md) for what this guarantees
you as a caller.
