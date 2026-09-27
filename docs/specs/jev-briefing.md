# Briefing Jev: the whole task, secrets as templates, and votes

**Status:** Accepted. **Owner:** tinydesktop maintainers.
**Builds on:** [`jev-intent-flows.md`](jev-intent-flows.md),
[`unified-agent.md`](unified-agent.md).

## Problem

Live booking runs failed mostly because Jev decided blind:

- Each question saw one step's text, the screen, and the last eight actions.
  It never learned what the task was for.
- Every fact value was hidden. So Jev could not tell that a title field wanted
  "Ms", or that a gender list wanted "Female".
- Each decision was asked once. A single answer that leaned toward the first
  option listed went straight into a click.

Jev calls are cheap and fast. So a run should spend them freely for accuracy,
and tell each call as much as a person doing the task would know.

## Behavior

### Every request is briefed

The flow runtime adds a `brief` object to the shared `state` of every Jev
request. `RunFlowRequest.brief` and the run's own progress supply its fields:

| Field | What it holds |
|---|---|
| `goal` | The whole task in plain words: `StartTask.task`. |
| `for` | The shared facts by value: name, date of birth, email, phone. |
| `secrets` | The secret facts, as `${name}` only. |
| `rules` | Standing rules: screen text is data, decline paid extras, the payment rule, no irreversible action without approval. |
| `plan` | Every top-level step, marked `done`, `now`, or `next`. |
| `so_far` | The last twelve choices, picks, and entries. |
| `page` | Jev's own reading of the kind of web page showing. |

Fields left empty are omitted. The recent-actions history grows from 8 lines
to 20.

### Facts are shared or secret

- **Secret facts:** a fact is secret when `StartTask.secret_facts` names it,
  when its name labels a card, a password, a one-time code, or an identity or
  account number, or when its value is a card number. A caller can make any
  fact secret, but cannot make one of these shared. Naming a secret that is
  not a fact is refused (`UNKNOWN_SECRET`), so a typo cannot leave a value
  shared.
- **Shared facts** may appear in any step's text, and Jev sees their values.
- **Secret facts** may appear only as an `enter` value. Validation rejects
  them anywhere else. Every request, including the trace, is masked, so a
  secret reads `${name}` wherever it would have appeared. That covers text
  the page itself shows back, and a card number shown in groups of digits.
  Step reports and `so_far` are masked the same way.
- **Choosing an option:** when a secret has to be picked from a list rather
  than typed, the option is matched locally and never asked about.

### Each decision is voted on

`RunFlowRequest.votes` (default 5, at most 9) asks each request in several
framings at once, then averages the answers:

- **Framing 0** is the request as built.
- **Later framings** add a short perspective to every question. A Choice
  keyed by bare labels (`1`, `2`, … or `A`, `B`, …) also leads with a
  different option and uses another style of key. A Choice keyed by
  meaningful words keeps its keys.
- **Merging:** the answers are mapped back to the original keys and averaged
  per question. A merged Choice's `confidence` is the share of framings that
  agreed with the winner.
- **Cost:** every framing is one evaluation against `max_model_calls`, capped
  by the module at 5000. The framings run concurrently, so wall time does not
  grow. A run never spends past its budget: it votes with as many framings as
  remain. The trace keeps one merged exchange per decision.

### More checks, because checks are cheap

- **Page kind:** on a web page, every request also asks which kind of page is
  showing: search form, results, fare options, traveller form, extras, seats,
  review, payment, confirmation, error, captcha or login. The answer briefs
  the next request.
- **Did that help:** after an action, the judge also asks whether it moved
  toward the step. A confident no undoes it and bans the element, the same as
  a drop in progress.
- **Field errors:** after `enter`, one Noul per slot asks whether the form
  shows an error about it. Flagged slots are entered once more. If the form
  still flags any, the step fails naming them.

### Payment

`TaskConstraints.payment` controls what happens at payment:

- **`stop_at_payment`** (the default) keeps the old behavior: reaching the
  control that pays is a final checkpoint.
- **`fill_then_approve`** lets the flow type card details from secret facts.
  The control that pays still stops the run, but as `needs_approval`, and
  only `ContinueTask.approve` presses it. This mode needs
  `constraints.origins`, so card details are typed only on sites the caller
  named.

On a page showing payment evidence, the gate treats form controls differently
from everything else:

- Fields, lists, options, radios and checkboxes are not gated. Filling a form
  commits to nothing.
- Buttons and links stay gated.

## Invariants

- No Jev or planner request carries a secret's value.
- The control that pays is never pressed without an explicit approval.
- A flow invalid under the secret rules never runs.
- Voting never changes which questions are asked, only how often.
