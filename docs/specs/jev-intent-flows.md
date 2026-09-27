# Jev intent flows

**Status:** Implemented. **Owner:** tinydesktop maintainers.
**Plan:** [`../plans/jev-intent-flows.md`](../plans/jev-intent-flows.md).

## Problem

`RunGoal` hands Jev one goal ("write an email to Sam about Friday") and asks it,
turn after turn, for one operation and one target out of up to 254 actionable
elements. Jev is a decision model: it chooses well among options it is given,
but it does not plan, and it never writes text. In production that loop looked
unintelligent, for reasons that were mostly ours:

- stall detection compared ref IDs, which every snapshot re-mints, so it never
  fired;
- Jev saw a flat, silently truncated list with no labels, status text, or
  field contents;
- a failed action or one low-confidence turn ended the run;
- `send` in the goal made every click need confirmation;
- a whole multi-step task rode on one long goal string, which asks a chooser to
  plan.

Meanwhile the callers that want desktop automation are usually language models
that can plan and write, but do not know any particular application's
interface and should not have to read it.

## Goals

- A caller writes a short, **UI-agnostic** script of what to accomplish — no
  element names, shortcuts, menus, or coordinates — and the module grounds it.
- Planning and text stay with the caller; choosing stays with Jev; structure,
  verification, and recovery are deterministic Rust composing many small Jev
  questions.
- Read as little of the screen as the decision needs.
- Every run is measurable on real applications, with a checker that reads real
  state and a trace that explains every decision.

## Non-goals

- A text-generating model inside the shipped module. The optional LLM author is
  lab-only (see [Tiers](#tiers)).
- Vision in the shipped module. Screenshots stay a member a host may call.
- Pixel or coordinate actions. Every action goes through the accessibility tree.

## Behavior

### The flow

A flow is JSON with an `app`, optional `vars`, and `steps`. A bare string step
is a `do` intent; an object has exactly one key naming its kind:

| Step | Meaning |
|---|---|
| `"<intent>"`, `do` | reach the described state |
| `open` | launch the app or bring it forward |
| `enter {slot: text}` | put each text into the field its slot describes |
| `choose {what, option}` | pick an option in a list, menu, or popup |
| `read {what, into}` | store visible text in a variable |
| `verify` | fail the flow unless a condition holds |
| `wait_for` | wait until a condition holds |
| `stop_before` | find an irreversible action and stop in front of it |
| `repeat_until {condition, steps, max}`, `if {condition, then, else}` | control flow |

`${name}` substitutes a variable. The authoring guide, with worked examples, is
`crates/tinydesktop-bus/src/flow/guide.md`, served verbatim by `FlowGuide`.

### Members

Contract 1.2 adds:

- `RunFlow(RunFlowRequest) -> FlowRunResult` — confidential, like `RunGoal`.
- `ValidateFlow(ValidateFlowRequest) -> FlowValidation` — takes raw JSON so a
  malformed flow comes back as readable, per-step errors.
- `FlowGuide() -> {guide, step_kinds}`.

`FlowRunResult` reports why the run stopped, one `StepReport` per step (kind,
outcome, turns, Jev calls, actions, the decision loops that contributed, a
note), the variables, the irreversible target a `stop_before` stopped at,
grounding hints learned, and — when `trace` is set — every Jev exchange.

### Decision loops

A step is run by composing small Jev questions (each Choice at most 20
options). Each loop can be disabled per run to measure it.

| Loop | Question | Effect |
|---|---|---|
| completion | Noul "is the step accomplished?", plus its negation | skip done steps; end a step without Jev self-reporting |
| progress | Score over five levels | a drop triggers undo; its top level corroborates completion |
| moves | Choice over app-agnostic moves and standard shortcuts | planning by choice, with no app knowledge |
| narrowing | Choice over screen regions, then elements; knockout groups | no silent truncation, small questions |
| corroboration | Noul "is this the element for the purpose?" | confirm a low-confidence target |
| consistency | the same Choice with relabelled, reversed options | act only when both agree |
| slots | one Choice per slot over the same fields | fill a form in one request, one field per slot |
| obstacles | Noul "is something unrelated in the way?", then a safe dismiss | handle sheets and prompts |
| undo | — | escape after a regression and ban the element |
| memory | Noul corroborating a remembered element | reads less on the next run |

Conditions (`verify`, `wait_for`, `if`, `repeat_until`) ask the condition, its
negation, and a five-level coverage Score in one request and combine them, so a
condition listing several things is judged crisply.

### Deterministic rules

- A step whose intent creates something new (`new`, `create`) cannot be
  complete before it acts: an existing draft or folder on screen is someone
  else's.
- An irreversible control (send, delete, purchase, submit, …) is never pressed
  by an ordinary step; only `stop_before` reaches one, and only with
  `allow_destructive`.
- Return is refused while a sheet or alert is showing.
- Text is delivered by set-value, read back (again after a short settle), and
  pasted when it did not arrive. A field without set-value (a rich-text body)
  is pasted at the caret, so a reply keeps what it quotes. A token field's
  U+FFFC value is accepted as delivered but unverified.
- An unreadable screen is shown to Jev as blank, with a note, for up to three
  looks, so a shortcut can still make progress.

## Invariants

- The shipped module links no text-generating model and no vision model.
- `RunFlow` requires confidential delivery; field values leave the machine only
  with `include_values`.
- Nothing irreversible happens without `allow_destructive`.
- Every Jev request the flow runtime builds validates against the client's
  request rules.

## Tiers

- **T0 `goal`** — `RunGoal` on the whole brief (the baseline).
- **T1 `flow`** — a hand-written, UI-agnostic flow run by the decision loops.
- **T2 `authored`** — an LLM (lab only, through the vendored `tinyinference`)
  writes the flow from the brief and the guide, never seeing the screen; after
  each run it sees a summary, including anything `read` captured, and either
  finishes or writes a flow for what remains.

## Acceptance criteria

- The four contract commands pass and every source file keeps 90% line
  coverage.
- `scripts/lab run mail-compose` fills a new Mail message's recipient, subject,
  and body, verifies it, and stops in front of Send, with the checker reading
  the compose window.
- `scripts/lab eval` produces a scorecard comparing tiers on the scenario
  ladder; results are recorded under [`../evals/`](../evals/).
