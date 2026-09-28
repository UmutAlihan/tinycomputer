# Unified agent on the travel fixture — 2026-09-27

The first end-to-end runs of the browser half of the unified agent
(`docs/technical/specs/unified-agent.md`), in the Docker lab on arm64 Linux with
Playwright's Chromium (build 1243) and agent-browser linked in-process.

## Without Jev: `browser_fixture`

`scripts/docker-lab -- crates/tinycomputer-examples/fixtures/run browser_fixture`

| Check | Result |
|---|---|
| the cookie-consent dialog is seen as the surface in front (`sheet`) | pass |
| the search form's From, To, and Departure date fields are offered | pass |
| the results page groups into exactly four cards | pass |
| exact ranking by lowest price picks IndiGo 6E-2135 at ₹6,840 | pass |
| the picked card opens with its "Select" link | pass |
| the payment page is recognised (card number, expiry date, CVV) | pass |

## With live Jev: `task_fixture`

`scripts/docker-lab -- crates/tinycomputer-examples/fixtures/run task_fixture`

A booking task through the task controller, given every fact but the phone
number:

1. `needs_input` for `phone` before anything ran; answered with
   `ContinueTask.inputs`.
2. The task ran every step:

   | Step | Kind | Outcome |
   |---|---|---|
   | 1 | browse | open (SkyFare · Search flights) |
   | 2 | enter | 3 values, with the cookie dialog in front |
   | 3 | do | "search for flights" (confidence 0.94) |
   | 4 | wait_for | results listed after 1 check |
   | 5 | pick | IndiGo 6E-2135, ₹6,840, ranked by lowest price out of 4 |
   | 6 | enter | 4 traveller values |
   | 7 | do | past the traveller details (0.79) |
   | 8 | do | skipped the seat upsell (0.97) |
   | 9 | stop_before | gated in front of paying |

3. Final status: `checkpoint` — "reached the payment step (Pay ₹6,840);
   payment is always left to you".

## Findings

- Playwright's arm64 Chromium lives under `chrome-linux-arm64`, which
  agent-browser's discovery does not search; the runs pass the executable
  explicitly (`TINYCOMPUTER_BROWSER_EXECUTABLE`, or the module's
  `browser.executable` configuration).
- agent-browser launches a browser before any command not on its skip list,
  so tests that must not launch one send an empty action.
