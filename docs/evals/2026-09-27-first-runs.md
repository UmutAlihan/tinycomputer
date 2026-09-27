# 2026-09-27 — first lab runs

Machine: macOS, Apple Silicon, one iCloud Mail account. Jev: `jev-latest`
(`typesafe/jev-1.13-20260917`) through OpenRouter. Single trials; treat the
numbers as a first read, not a benchmark. The screen locked partway through the
session, which ended live runs (a locked screen exposes no window).

## Results

| scenario | flow (T1) | goal baseline (T0) | notes |
|---|---|---|---|
| textedit | **pass** — 3 actions, 4 Jev calls, 3.7 s | not comparable | the baseline ran on a document the flow had already filled; scenarios now reset first |
| mail-compose | **pass** — 5 actions, 3–4 Jev calls, 3.3 s; stops in front of Send | error | the baseline hit a Jev client bug (fixed, below) |
| calculator | **pass** — 128 × 37 by keypad: 9 actions, 30 Jev calls, 15.8 s | fail (low confidence on turn 1) | narrowing picked each key among ~30 buttons |
| notes | fail | fail | the note editor sits in a subtree the engine truncates; exploration added, not yet re-run |
| finder | fail | fail | "more than one window" while a new folder is being renamed; window disambiguation added, not yet re-run |
| settings-appearance | fail | — | the pane was reached; the mode is a selected state, not text; then the screen locked |
| mail-compose (T2, authored) | — | — | the LLM, never shown Mail, wrote a correct flow with the whole email; the run failed on launch (fixed, below) |

The flagship — a complete multi-paragraph email in Mail, recipient, subject,
and body verified, stopped in front of Send — runs end to end in about three
seconds. Its second run made no Jev calls for the fields: grounding memory
recalled them.

## What the runs found, and what changed

| Finding | Change |
|---|---|
| Jev's probabilities arrive rounded to two decimals; the client rejected valid Score answers and near-tied Choices | the Jev client moved into `tinyinference` (`tinyinference_decisions`); its Score tolerance had been fixed there, and the Choice tolerance fix is an upstream PR |
| `--headed` press timed out on TextEdit's open panel (no window to focus) | the lab defaults to headless |
| an app with no readable window failed the step | blank-screen looks with a note, for up to three turns |
| the paste fallback always failed: the engine's clipboard helper was not beside the module | `scripts/lab` and macOS release archives ship `agent-desktop-macos-helper` |
| Mail's body is a web area with no set-value | web areas count as fields; text is pasted at the caret |
| the To field reads back as U+FFFC tokens | tokenized read-back counts as delivered but unverified; `field_contents` shows the token text |
| verify on a list-like condition hedged at ~0.6 | negation calibration plus a coverage Score (0.54 → 0.79–0.83) |
| an existing draft satisfied "start a new email" — the flow wrote into a person's own draft | creation intents cannot be complete before acting |
| an empty Choice reached the client after a knockout of all-`none` | `decide` returns no element instead |
| launch of an app with several windows returned `AMBIGUOUS_TARGET` | treated as running |

## Next

- Re-run the ladder with resets on an unlocked screen, three trials per mode,
  including `authored` for `mail-compose` and `mail-reply`.
- Notes and Finder after the exploration and window fixes.
- System Settings: its window was reported as not exposed through
  accessibility; this is the case the optional screenshot check is for.
