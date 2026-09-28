# Plan: Jev deliberation

Spec: [`../specs/jev-deliberation.md`](../specs/jev-deliberation.md). Status:
done. Each phase started with a failing test and ended with the four contract
commands green.

| # | Phase | Test first | Code |
|---|---|---|---|
| 0 | Contract: `Deliberation`, eight loops, `TaskBudget.deliberation`, version 2.3, defaults | `a_deliberation_level_pins_its_wire_spelling_and_round_trips`, the version and default pins | `tinycomputer-bus` `flow/types.rs`, `agent/types.rs`, `version/` |
| 1 | Ballots and the evidence gate | `evidence/test.rs` | `vote.rs` (`ballots`, `tally`, `framings_between`), `evidence/` |
| 2 | Escalation: more framings, duel, contrast, views | `duel/test.rs`; `lookalike_buttons_are_resolved_by_a_duel`, `a_duel_split_by_position_bias_is_settled_by_contrast`, `a_close_call_nothing_settles_is_not_pressed`, `standard_deliberation_takes_the_duel_champion_without_contrast`, `a_condition_split_across_views_does_not_pass` | `escalate/`, `duel/`, `ground.rs::settle`, `act.rs::settle_done`, `steps.rs::holds` |
| 3 | Tree beam and the wider cross-check | `the_tree_keeps_a_close_second_region`, `a_region_cut_that_lost_the_target_is_caught_by_the_wider_choice` | `ground.rs` (`kept_regions`, `decide`) |
| 4 | Denoising | `denoise/test.rs`; `disabled_elements_never_reach_jev`, `an_oscillation_bans_the_pair`; the browser's `sight/test.rs` | `denoise/`, `ground.rs::opening`, `act.rs::note_oscillation`; `sight.js` |
| 5 | Expectations | `expect/test.rs` | `expect/`, `act.rs` (`expect`, `check_expectation`, `judge_turn`) |
| 6 | Checkpoints, `Surface::back`, verified undo, the irreversible bar | `checkpoint/test.rs`; `a_wrong_navigation_is_undone_by_going_back_and_verified`, `an_unverifiable_undo_fails_the_step_closed`, `an_irreversible_press_needs_a_deep_accept`; the core, workspace, and browser `back` tests | `checkpoint/`, `act.rs::undo`, `steps.rs::stop_before`, `tinycomputer-core` `surface/mod.rs`, `workspace/mod.rs`, `tinycomputer-browser` `surface/mod.rs` |
| 7 | Backtracking | `a_wrong_toggle_is_pressed_again_and_the_runner_up_is_tried` | `act.rs` (`plan_branch`, `try_branch`), `reflect.rs` |
| 8 | Budget, calibration view, lab knobs, docs | `escalation_degrades_gracefully_when_the_budget_is_short`, `clear_evidence_costs_nothing_more_than_the_legacy_gates`, `calibration_tallies_verdicts_against_how_steps_ended` | `tinycomputer-examples` `journal/`, `jev_journal`, `lab --deliberation`, `TINYCOMPUTER_FLOW_DELIBERATION` |

The simulator's scenarios are in `flow/test/deliberation.rs`. The shop pages
it gained (`EXTRAS`, `TERMS`, `REVIEW`) give it an address, `back`, and two
toggles.

## Remaining

- Live evaluation, `off` against `deep`, on the Docker lab scenarios, with the
  journal on. Record it in `docs/evals/` and tune the constants from
  `--calibration`.
