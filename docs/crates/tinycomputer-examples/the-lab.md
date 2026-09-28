# The lab

The lab is how you find out whether a change to tinycomputer made it better
or worse at actually using an application, rather than just changing what a
unit test asserts. It runs a short list of real, everyday tasks (write an
email, do a sum, rename a folder) against the real applications on your
Mac, and then checks, by reading the application's actual state, whether the
task really happened. Not whether the run reported success: whether the
email really has the right subject, whether Calculator really shows the
right number.

Full detail lives in [`docs/technical/lab.md`](../../technical/lab.md); this
page is the friendlier tour.

## Setup

- macOS, with Accessibility granted to the terminal you run the lab from
  (and Screen Recording if you want screenshots).
- `OPENROUTER_API_KEY` in your environment or in `.env` at the repository
  root. Jev, and the optional LLM author for `--mode authored`, both go
  through OpenRouter.
- The scenarios drive Mail, Notes, TextEdit, Calculator, Finder, System
  Settings, and Spotify. Mail needs an account configured. Your screen has
  to be unlocked: a locked screen has no window for accessibility to see.

Always run the lab through `scripts/lab`, never `cargo run` directly on the
`lab` binary. The script builds `tinycomputer` in release mode into
`target/lab/`, writes a `modules.toml` beside it so TinyBus attests it the
way a real host would, and, on macOS, builds and places the clipboard
helper the engine needs for rich-text paste. Skipping the script means the
module is not attested and some paths behave differently than they would in
production.

## Commands

```sh
scripts/lab list                        # every scenario
scripts/lab run mail-compose             # one run, timeline, checker verdict
scripts/lab eval all --modes flow,goal --trials 3   # many runs, one scorecard
scripts/lab report target/lab-runs/mail-compose/flow-<timestamp>
scripts/lab call ListWindows '{"app": "TextEdit"}'  # probe any bus member
scripts/lab validate crates/tinycomputer-examples/scenarios/notes/flow.json
scripts/lab guide                        # print the flow authoring guide
```

`list`, `guide`, and `report` never load the module, so they work without
Accessibility granted or an API key set.

## Flags

| Flag | Effect |
|---|---|
| `--mode flow\|goal\|authored` | which of the three modes below to run |
| `--headed` | drive the application with real, physical mouse and keyboard input instead of accessibility actions, so you can watch the cursor move and the window actually receive focus |
| `--disable moves,undo` | turn off named decision loops, to measure what they were doing |
| `--no-memory` | ignore grounding hints saved from earlier runs |
| `--flow <file.json>` | run your own flow file against a scenario's checker, instead of the scenario's own flow |
| `--strategy narrow\|wide` | how a decision asks Jev: one focused question at a time (`narrow`, the default) or a single request over a digest of the whole screen (`wide`); see [`specs/jev-wide-turns.md`](../../technical/specs/jev-wide-turns.md) |
| `--deliberation off\|standard\|deep` | how much a decision double-checks itself before acting |
| `--send` | for the mail scenarios only: actually sends the draft, but only ever to `TINYCOMPUTER_LAB_SELF_EMAIL`, never anywhere the flow itself names |

`--send` is worth pausing on. The lab refuses to send unless the flow's only
recipient is exactly the `${to}` variable, and it overwrites that variable
with your own address before sending. A scenario or a hand-edited flow that
names a literal address, a different variable, or a reply recipient in
place of `${to}` is rejected. This is deliberate: it means the one flag that
actually does something in the world can never be pointed anywhere but back
at you.

## The three modes

- **`goal`**: the baseline. One call to `RunGoal` with the scenario's whole
  goal as one string, no flow at all. This is the simplest thing that could
  work, and the number every other mode is implicitly compared against.
- **`flow`**: the scenario's hand-written flow
  (`crates/tinycomputer-examples/scenarios/<name>/flow.json`), a short list
  of plain-language steps a person wrote with no idea which button does
  what. See [writing flows](../../writing-flows.md) for the language.
- **`authored`**: an LLM writes the flow itself, from the scenario's brief
  alone. It never sees the screen. It is given the brief and the same flow
  guide `scripts/lab guide` prints, writes a flow, and the lab validates and
  runs it. Afterward the author is shown a summary of what happened,
  including anything a `read` step captured, and gets up to three rounds to
  either declare it done or write a follow-up flow for what is left. The
  model is `TINYCOMPUTER_LAB_MODEL` (default `anthropic/claude-sonnet-5`,
  called through the vendored `tinyinference` client), and every LLM call
  this mode makes is counted separately from Jev calls in the scorecard.

## What a run leaves behind

Every run writes to `target/lab-runs/<scenario>/<mode>-<timestamp>/`:

| File | What's in it |
|---|---|
| `request.json`, `result.json` | the exact request sent and the result returned |
| `timeline.txt` | each step, its outcome, how many turns and Jev calls it took, which decision loops contributed, and every action with its target |
| `jev.jsonl` | one line per Jev decision: the state Jev saw, the question it was asked, the answer it gave. The file to read when a run picked the wrong thing |
| `verdict.json` | the checker's PASS/FAIL and why, read from the application's real state, never from the run's own report |
| `authored-N.json` | in `authored` mode, each flow the LLM wrote, one per round |

If you also set `TINYCOMPUTER_JEV_JOURNAL=1`, the module additionally writes
a full journal of every raw Jev call and every timing under
`.jev-journal/`; see [reading a journal](reading-a-journal.md). `jev.jsonl`
and the journal answer different questions: the trace file shows what a run
*decided*, the journal shows what actually went over the wire and how long
it took.

`target/lab-runs/memory.json` carries grounding hints between runs of the
same scenario, where things tended to be found last time, so a second run
of the same scenario usually needs fewer Jev calls than the first.
`--no-memory` turns that off, which is useful when you want to measure a
loop's cold-start cost specifically. See
[memory and saving](../../memory-and-saving.md).

## The debugging loop

1. `scripts/lab eval all --modes flow,goal --trials 3`: get a scorecard
   across every scenario and mode.
2. Pick a failure, re-run it with `scripts/lab report <its dir>`, and read
   its `jev.jsonl`.
3. Work out where the fault actually is: was Jev shown the wrong thing (an
   **observation** problem), asked the wrong thing (a **question**
   problem), given the wrong instructions (a **flow** problem), or did the
   engine click the right thing in the wrong place (an **engine** bug,
   which belongs upstream in `agent-desktop`)? See
   [how it decides](../../how-it-decides.md) and
   [catching mistakes](../../catching-mistakes.md).
4. Reproduce the behaviour as a failing test against the simulator in the
   flow runtime's test suite, fix it there, then re-run the scenario to
   confirm.
5. Worth recording, record it in
   [`docs/technical/evals/`](../../technical/evals/); see the existing
   files there for the format.

## The scenarios today

Run `scripts/lab list` for the live list; as of this writing they are
`calculator`, `finder`, `mail-compose`, `mail-reply`, `notes`,
`settings-appearance`, `spotify`, and `textedit`. Each one's folder under
[`scenarios/`](../../../crates/tinycomputer-examples/scenarios/) has a
`brief.md` (what the `authored` mode is given) and a `flow.json` (the
hand-written flow the `flow` mode runs); the goal string, the reset steps,
and the checker that verifies the outcome live in code, in
[`src/lab/scenario.rs`](../../../crates/tinycomputer-examples/src/lab/scenario.rs).
