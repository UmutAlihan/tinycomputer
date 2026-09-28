# Undo and backtracking

A completion judge only ever sees the screen after a press has already
happened. That is enough to notice a step went wrong eventually, but not
enough to catch a press that succeeded and still did the wrong thing: a
checkbox that was already ticked gets cleared instead of set, or a link
opens a whole new page where a dropdown was actually wanted. Left alone,
that kind of mistake tends to surface much later, as a wrong price on a
review page, or not at all.

This page covers the three pieces that catch and fix this: predicting
what a press should do (`expect/`), recording where the screen was before
it happened and undoing back to it when needed (`checkpoint/`), and
retrying a different candidate afterward (backtracking, in `act/recover.rs`).

## Expectations: predicting an effect before it happens

Before a deliberating `do` move presses an element, `expect/` predicts
what that press should do, from nothing but the move and the element's
role and current state:

| Prediction | When |
|---|---|
| opens | the move is `expand`, or the element is a combobox or a collapsed control |
| selects or clears | the element is a tab, radio, or option; or a checkbox or switch |
| navigates | the element is a link |
| closes | the element's label is Close, Cancel, or Done |
| scrolls | the move is `scroll` |

After the press, the screens from before and after are compared against
that prediction. Only an outright contradiction counts as a **miss**: a
checkbox that does not show the state it was pressed toward, or a press
that left the page entirely when it should have just opened a menu. An
effect that the screen neither confirms nor rules out is `unclear`, and
changes nothing; a page that opens a menu in some way the prediction did
not anticipate is not mistaken for a wrong click just because the
prediction did not cover it.

After a miss, the next judgement carries one extra pair of questions,
`intended` against `unintended`, calibrated together the way every yes/no
pair is. This is asked only after a miss, not after every deliberated
press: it was tried on every deep press at first, but live runs showed
Jev sometimes reading a repair's own legitimate reopening of a calendar as
`unintended`, which then undid the very repair that was fixing things.
Restricting the question to genuine contradictions fixed that.

Two situations count as a **mistake**, which triggers an undo:

- a miss with belief under 0.50 (`MISTAKE`);
- any press at all, miss or not, with belief under 0.25
  (`CLEAR_MISTAKE`).

A mistake joins the older triggers that already existed in the `do` loop:
a progress drop of a quarter of the scale (`REGRESSION`), and a `helped`
answer under 0.20 (`UNHELPFUL`). See [The do loop](the-do-loop.md).

## Checkpoints and a verified undo

Before a deliberating `do` move presses anything, `checkpoint/` records
where the screen was: a ref-free fingerprint of what is on it, and the
surface's address, when the surface has one (a URL, on the browser). Each
action is then classified by how it could, in principle, be undone:

| Class | Actions | Undone by |
|---|---|---|
| reversible | opening something, scrolling | Escape |
| restorable | flipping a toggle | pressing it again |
| restorable | a navigation | `Surface::back`, then loading the recorded address |
| restorable | text a failed `choose` typed | retyping each changed field's previous text |
| irreversible | send, pay, delete, a `stop_before` control | nothing |

The last row is worth dwelling on. A `choose` step that cannot find its
option sometimes falls back to typing the option's text to filter a list
(see [Step kinds](step-kinds.md#choose)), and that typed text lands
wherever the focus happens to be. Live on Emirates, a gender field the
form never actually asked for got typed into the last name field instead,
turning "Raina" into "RainaFemale," in every single run of that scenario.
A deliberating `choose` now records every text field's contents first,
and, when the step ends up failing, retypes each field it actually
changed back to what it held before, matched by the field's kind alone
(never a field that refused text in the first place).

Every undo is then **verified**, not just attempted. The screen must show
0.80 (`RESTORED`) of the checkpoint's original marks again, at the
recorded address. If a rung that claims to restore something (going back,
navigating, pressing again) is tried and the screen does not come back to
match, the step **fails closed** rather than pressing on with a screen
nobody actually understands anymore. An Escape that misses is treated more
gently: it is noted, and the loop continues, the way it always has.

`Surface::back` is what makes the navigation row possible. The browser
implements it: going back and reporting the address it landed on. A
desktop application refuses it by default, since "back" is not a universal
concept on the desktop the way it is in a browser.

An irreversible press is never treated as one branch among several
options to explore; there is no undoing it to try something else. A
`stop_before` step that is allowed to actually press its control
(`allow_destructive`) needs belief at 0.85 (`IRREVERSIBLE_FLOOR`) at the
deep deliberation level before it will. A pick that falls short of that
gets vouched for once more, "is it?" against "is it only similar to it?",
widened across more framings, and the press is refused unless that belief
reaches the floor.

## Backtracking

After an undo, the step's **frontier**, the ranked runners-up kept from
grounding (see [Grounding](grounding.md)), offers the best candidate that
has not already been banned this step. On the next `activate` move, that
candidate is confirmed with one `confirm` question and pressed if it
reaches 0.50 (`AGREED`), before the runtime tries grounding an element
completely from scratch again.

Each step gets a limited number of backtracks: 3 at the deep deliberation
level, 1 at standard, 0 when deliberation is off (`MAX_BRANCHES`).

A `choose` step whose reflection fails (see [Reflection](reflection.md))
first returns to wherever the step began, verified, if it has since
navigated away, and only then attempts its repair.

## Reading a backtrack in the journal

The journal events to look at, in order, for a story like "pressed the
wrong thing, undid it, tried the next best guess": `expect` for the
prediction and the miss, `checkpoint` for what was recorded, `restore` for
the undo and whether it verified, and `backtrack` for what was tried next.
See [`docs/technical/flow-examples.md`](../../../technical/flow-examples.md#5-when-things-go-wrong)
for a table mapping report notes to exactly these events.

See [`docs/technical/decision-thresholds.md`](../../../technical/decision-thresholds.md)
for `MISTAKE`, `CLEAR_MISTAKE`, `RESTORED`, `IRREVERSIBLE_FLOOR`,
`MAX_BRANCHES`, and `AGREED`.
