# Decision thresholds

Every constant a flow decision is thresholded on, with its value and where it
lives. [`decision-loops.md`](decision-loops.md) explains each loop;
[`specs/jev-wide-turns.md`](specs/jev-wide-turns.md) the wide strategy's.
Change a constant and its row together.

| Constant | Value | Where | Meaning |
|---|---|---|---|
| `DONE` | 0.75 | `act.rs` | completion that ends a step after acting; also the bar for `verify`, `wait_for`, `if`, `repeat_until` |
| `ALREADY_DONE` | 0.85 | `act.rs` | completion that skips a step before acting |
| `BLOCKED` | 0.70 | `act.rs` | obstacle probability that triggers dismissal |
| `LEANS_DONE` | 0.50 | `act.rs` | completion under which a `finished` move is overruled after acting |
| `REGRESSION` | 0.25 | `act.rs` | progress drop that triggers undo |
| `UNHELPFUL` | 0.20 | `act.rs` | `helped` probability that triggers undo |
| `SHORTCUT_FLOOR` | 0.50 | `act.rs` | least probability for pressing a shortcut |
| `ACT` | 0.70 | `view/mod.rs` | element choice used without re-asking |
| `NAMED_FLOOR` | 0.45 | `ground.rs` | element choice used when its name is in the purpose |
| `CORROBORATED` | 0.80 | `ground.rs` | corroboration that accepts a target alone |
| `AGREED` | 0.50 | `ground.rs` | corroboration that accepts a target the re-ask agreed on |
| `SLOT_FLOOR` | 0.40 | `enter.rs` | least probability for a slot assignment |
| `LOCATE_FLOOR` | 0.50 | `steps.rs` | least probability for a `read`, `pick`, or `stop_before` target |
| `CAP` | 20 | `ask.rs` | most options in one Choice |
| `DO_TURNS` | 8 | `steps.rs` | turns a `do` step may spend |
| `REFLECT_FLOOR` | 0.50 | `reflect.rs` | belief that a pressed `choose` left its choice, below which it is repaired, and failed if the repair does not take |
| `REPAIR_TURNS` | 4 | `reflect.rs` | turns one reflection repair may spend |
| `MAX_ACTIONS` / `MAX_CALLS` | 120 / 10000 | `mod.rs` | per-run caps on actions and Jev calls |
| `MAX_VOTES` | 9 | `vote.rs` | most framings one decision is asked in; a deliberated decision is widened up to it |
| `STALL_TURNS` / `MAX_IDLE_WAITS` | 3 / 2 | `act.rs` | unchanged turns before a step fails; idle waits before Jev may not wait again |
| `MAX_OBSTACLES` / `MAX_UNDOS` | 2 / 2 | `act.rs` | obstacles dismissed and undos run per step at most |
| `FIELD_ERROR` | 0.70 | `enter.rs` | field-error probability that makes a slot be entered again |
| `NOT_ASKED` | 0.35 | `enter.rs` | "the form asks for it" probability under which a slot with no field is taken as not asked for |

## Deliberation

The gates of [`specs/jev-deliberation.md`](specs/jev-deliberation.md), which
replace the single-number bars above at every site they apply to unless
`deliberation` is `off`.

| Constant | Value | Where | Meaning |
|---|---|---|---|
| `MAX_DISTRACTIONS` / `MAX_CLEARED` | 4 / 3 | `attention/` | distractions one attention question offers; distractions cleared per step |
| `ATTENTION_FLOOR` | 0.50 | `attention/` | least probability a distraction must win the attention Choice with, beside the gate's margin and agreement |
| `ACCEPT_MARGIN` | 0.25 | `evidence/` | least lead of a Choice's winner over the runner-up to act on it as read |
| `ACCEPT_AGREEMENT` | 0.80 | `evidence/` | least share of framings that picked the winner, or put a judgement on the same side of its threshold, to act on it as read |
| `ABSTAIN_FLOOR` / `ABSTAIN_AGREEMENT` | 0.20 / 0.40 | `evidence/` | a winner under both is abstained from: nothing serves |
| `UNDECIDED_BAND` | 0.12 | `evidence/` | half-width of the band around a judgement's threshold inside which it is deliberated |
| `MAX_FINALISTS` / `FINALIST_FLOOR` | 4 / 0.05 | `duel/` | most finalists a duel compares, and the least probability to be one |
| `DUEL_WIN` | 0.60 | `duel/` | least share of every pairing the champion must take, both orders averaged |
| `CONTRAST_ACCEPT` / `CONTRAST_LEAD` | 0.65 / 0.20 | `escalate/` | with no duel champion, the belief and lead a contrasted leader needs to be taken over the duel's ranking |
| `BRANCH_MARGIN` | 0.30 | `ground.rs` | lead of the chosen region under which narrowing keeps the runner-up region too |
| `MISTAKE` | 0.50 | `act.rs` | "did what it was meant to" belief under which a press whose effect was missed is undone |
| `CLEAR_MISTAKE` | 0.25 | `act.rs` | that belief under which any press is undone |
| `MAX_BRANCHES` | 3 deep / 1 standard | `act.rs` | next-best candidates a `do` step backtracks into |
| `RESTORED` | 0.80 | `checkpoint/` | share of a checkpoint's marks a screen must show again for an undo to count as verified |
| `IRREVERSIBLE_FLOOR` | 0.85 | `steps.rs` | belief a deep run needs before a `stop_before` presses its control |
| `CROWDED` | 40 | `survey.rs` | actionable elements above which a wide turn surveys the screen first |
| `DISTRACTION` | 0.70 | `survey.rs` | distraction probability that collapses a region and ranks it last |
| `DIGEST_BUDGET` | 24,000 bytes | `wide.rs` | screen a wide request shows before regions are collapsed (about 9,000 tokens of dense page text) |
| `WIDE_POOL` | 40 | `wide.rs` | candidates one move is offered in a wide turn: two Choices of `CAP` |
| `NEW_TENTHS` | 3 | `survey.rs` | tenths of a page's regions that must be new before a step surveys it again |
| `REGION_SIZE` | 24 | `tinycomputer-core` `surface/digest/` | elements a region holds before it is split one level deeper |
| `MAX_DEPTH` | 10 | `tinycomputer-core` `surface/digest/` | deepest ancestor level regions are split on |
| `LIST_CARDS` | 12 | `tinycomputer-core` `surface/digest/` | cards of a list shown one line each before the rest are counted |
| `CARD_CHARS` | 160 | `tinycomputer-core` `surface/digest/` | longest a card's line is let to run |
| `EXAMPLES` | 5 | `tinycomputer-core` `surface/digest/` | example labels a collapsed region names |
| `SUMMARY_CHARS` | 200 | `tinycomputer-core` `surface/digest/` | longest a collapsed region's one-line summary is let to run |
| `COLLAPSED_SLACK` | 400 bytes | `tinycomputer-core` `surface/digest/` | how far collapsed summaries push `spent` past the render budget before the rest are reported as one count instead |
| `MAX_FINISHED` | 40 | `ledger.rs` | finished steps the ledger keeps before the oldest is dropped |
| `MAX_RECENT` | 24 | `ledger.rs` | history lines shown as `recent_actions` |
| `MAX_TRIED` | 12 | `ledger.rs` | `tried_and_failed` notes kept before the oldest is dropped |
| `MAX_LINE` | 240 | `ledger.rs` | longest a ledger line is let to run |
