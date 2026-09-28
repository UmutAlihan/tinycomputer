# 2026-09-28 — deliberation, `off` against `deep`, live

What [`specs/jev-deliberation.md`](../specs/jev-deliberation.md) does on real
booking sites. Two tasks, each replaying one saved plan so that only the
deliberation level differs between runs: `task_live` with `FLOW_FILE`,
alternating `off` and `deep`, Jev `jev-latest` through OpenRouter, 7 votes,
the journal on. Every run stopped before payment.

**Verdict: no measurable accuracy gain yet, one real bug class fixed, and
five deliberation defects found live and fixed.** Every run was ended by the
site or by a perception gap that no deliberation level can reach: a
blocked search, cards that are not read as results, a calendar that does
not close. On the stretches both levels completed, deep made the same
clicks as off and spent about 12–20% more Jev calls.

## Kashmir: Delhi → Srinagar on IndiGo (23 steps)

| Setup | off (steps reached) | deep (steps reached) |
|---|---|---|
| Docker lab, headless | 9 | 7 → fixed, see 1 |
| Fresh local Chrome over CDP | 11, 11 | 11, 11 |
| The person's own Chrome (`:9222`) | 21, 16, 21, 14 | 15, 15 → fixed, see 2; then 21, 10, 8 → fixed, see 3 and 4; then 21, 12, 12 |

- Fresh profiles: all four runs filled Delhi → Srinagar, 18 Oct, one adult
  and pressed Search correctly; IndiGo answered "No Data Available" to every
  one. Deep made exactly the clicks off made, at 456 calls against 406.
- The person's own Chrome got past search until IndiGo began withholding
  results there too (the two 12s: "flight results" never showed).
- Off's failures: step 16 was the old Escape-only undo closing a passenger
  section it had just opened correctly (the case expectations now prevent);
  step 21 is a plan gap (it checks for paid extras before pressing Continue).

## Emirates: Mumbai → Dubai (26 steps)

| Run pair | off | deep | Stopped on |
|---|---|---|---|
| 1–2 | 13, 13 | 13, 13 | `pick` read four footer links as the results, not the flight cards |
| 3–4 (step 13 as a `do`) | 13, 10 | 10, 1 | a fare card press changed nothing; Economy not found; a timeout |
| 5–6 | 18, 18 | 18, 10 | `choose` "Female" for a gender the form never asks → see 5 |
| 7–12 | 10 ×4 | 10 ×6 | the date calendar stayed open over the form → see 6 |

## Defects found live, and what changed

1. **Views vetoed a done step** (Docker, "choose One Way"): the screen-only
   view read 0.2 — a selected radio shows no sign of who selected it — and
   the minimum rule failed the step. Views are now combined by their median.
2. **Views overruled a finish** (IndiGo passenger page): a judge at 0.64 with
   Jev choosing `finished` was pulled to 0.49 by views, under `LEANS_DONE`,
   and deep pressed the empty form's Next. Views now run only for a
   judgement that would pass.
3. **Abstaining stalled a `do` step** (IndiGo, confirming Delhi): a close call
   the duel and contrast could not settle pressed nothing, three turns in a
   row. A close call is now acted on at its best ranking, runners-up kept
   for a backtrack; only "nothing serves" abstains.
4. **Every-press "intended?" broke a repair** (IndiGo date): reopening the
   calendar to re-pick was read as unintended and undone. The question is
   now asked only after the screen contradicts the predicted effect.
5. **A failed `choose` left typed text behind** (Emirates, every run, both
   levels): with no gender list, "Female" was typed into the focused last
   name — `Raina` became `RainaFemale`. A deliberating `choose` now records
   every text field and retypes what it changed when it fails.
6. **Distractions** (the attention layer, added in response): false alarms
   on field-clear icons (fixed with `MAX_DISTRACTION_SIZE` and "not a form");
   a calendar covering the Class button is now found and, once the option
   named the covered control, Jev chose to clear it — but Escape does not
   close Emirates' calendar, and it was pressed three times in one step
   (fixed: once per step).

## What would measure accuracy

- A **replayable benchmark**: live sites change and block between runs, and
  each run spends most of its evidence on site behaviour. Saved pages for
  the post-search half of each task would let `off` and `deep` be compared
  many times on identical screens.
- The perception gaps that end every run before the decisions deliberation
  is for: Emirates' flight cards not read as `pick` records; a date `choose`
  that reports success when the calendar did not take the date (its
  reflection passes it); IndiGo's `choose` typing to filter with no target.
