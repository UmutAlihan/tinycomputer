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
| `STALL_TURNS` | 3 | `act.rs` | unchanged turns before a step fails |
| `ACT` | 0.70 | `view/mod.rs` | element choice used without re-asking |
| `NAMED_FLOOR` | 0.45 | `ground.rs` | element choice used when its name is in the purpose |
| `CORROBORATED` | 0.80 | `ground.rs` | corroboration that accepts a target alone |
| `AGREED` | 0.50 | `ground.rs` | corroboration that accepts a target the re-ask agreed on |
| `SLOT_FLOOR` | 0.40 | `enter.rs` | least probability for a slot assignment |
| `LOCATE_FLOOR` | 0.50 | `steps.rs` | least probability for a `read`, `pick`, or `stop_before` target |
| `CAP` | 20 | `ask.rs` | most options in one Choice |
| `DO_TURNS` | 8 | `steps.rs` | turns a `do` step may spend |
| `MAX_ACTIONS` / `MAX_CALLS` | 120 / 5000 | `mod.rs` | per-run caps on actions and Jev calls |
| `MAX_VOTES` | 9 | `vote.rs` | most framings one decision is asked in |
| `CROWDED` | 40 | `survey.rs` | actionable elements above which a wide turn surveys the screen first |
| `DISTRACTION` | 0.70 | `survey.rs` | distraction probability that collapses a region and ranks it last |
| `DIGEST_BUDGET` | 40,000 bytes | `wide.rs` | screen a wide request shows before regions are collapsed |
| `WIDE_POOL` | 80 | `wide.rs` | candidates one move is offered in a wide turn: four Choices of `CAP` |
| `REGION_SIZE` | 24 | `tinycomputer-core` `surface/digest/` | elements a region holds before it is split one level deeper |
| `LIST_CARDS` | 12 | `tinycomputer-core` `surface/digest/` | cards of a list shown one line each |
