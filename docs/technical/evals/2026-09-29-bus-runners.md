# 2026-09-29: every demo through the bus

Until this date the task demos built the task controller in-process:
`task_live` and `task_fixture` never touched TinyBus. From here they load
the built, attested module with the real TinyBus loader
(`tinycomputer_examples::host`) and use nothing but its members: `PlanTask`,
`StartTask`, `AwaitTask`, `ContinueTask`, `TaskReport`, and the `Browser…`
members for the final screenshot. This records the first runs of every demo
that way. Each replayed its saved `plan.json` (`FLOW_FILE`), deliberation
`deep`, five rescues.

## Results

| Demo | Where | Result | Steps | Jev exchanges | Rescues |
|---|---|---|---|---|---|
| travel fixture (`task_fixture`) | Docker lab, launched Chromium | **pass**: payment checkpoint (Pay ₹6,840), phone number supplied through `ContinueTask` | — | — | — |
| WhatsApp (`tasks/whatsapp`, desktop) | this Mac, accessibility | **pass**: `done`, five chats read, `done.result` in the `output.json` shape | 17 | 31 | 1 recovered |
| Kashmir (`tasks/kashmir`) | the person's Chrome over CDP (`:9222`) | **pass**: IndiGo payment page, ₹7,671, Pay not pressed | 32 | 164 | 2 recovered, 1 failed again |
| Emirates (`tasks/emirates`) | the person's Chrome over CDP (`:9222`) | **pass**: stopped before "Continue to Payment" | 27 | 140 | 2 recovered, 2 failed again |
| Kashmir | Docker lab, launched headless Chromium | fail: IndiGo's destination picker never became editable | — | — | 5 failed again |
| Emirates | Docker lab, launched headless Chromium | fail: typing into the arrival-airport picker never filtered it | — | — | 2 recovered, 3 failed again |
| lab `calculator` (`scripts/lab`) | this Mac | fail: Calculator "shows no readable window" — the shell that ran it had no Accessibility permission; the module loaded, launched the app, and asked Jev 14 times | — | 14 | — |

Every recorded pass of the two booking sites
([`2026-09-28-rescue.md`](2026-09-28-rescue.md)) ran on the person's own
Chrome, and both headless failures stop at the sites' pickers, where a
fresh headless browser has never got past. They are the sites, not the bus.

## What only the bus showed

- **`TaskReport` could not be called.** It is confidential, and it took a
  `TaskRef`, `{"id": "t-1"}`. A TinyBus client refuses to put an object whose
  only field is a string `id` in a confidential body, because that is a stream
  handle's shape, whose bytes would travel unprotected. Every in-process
  caller had missed it. Contract 2.7 gives it a `TaskReportRequest`,
  `{"id", "trace"}`, with `trace` always serialized; `trace: false` also keeps
  a report small (the traced reports above are 0.5 to 3.3 MB).
- **The loader refuses a module under a directory another user can write.**
  It checks every ancestor, which rules out the Docker lab's bind mount and
  its volumes, so the Docker runners install the module under `$HOME`
  (`TINYCOMPUTER_MODULE_DIR`). The headless demos had never loaded the module
  in a container before.
- **Browser launch settings belonged to the runner, not the module.** A
  host could not set the user agent, launch arguments, or perception, which
  the booking sites need; they are now the module's `browser` configuration.

## Setup notes

- `scripts/build-module` builds and attests the module and prints its path;
  `scripts/lab`, `tasks/run`, and `fixtures/run` all call it.
- Building under the Docker lab's bind-mounted `target/docker-lab` twice lost
  dependency artifacts mid-build (`E0460`, then `E0463`); building with
  `CARGO_TARGET_DIR` inside the lab's cargo volume was reliable.
- Journals, traces, and the WhatsApp results hold personal data and stayed
  in git-ignored `target/`.
