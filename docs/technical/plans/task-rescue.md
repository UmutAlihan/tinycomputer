# Plan: task rescue

Spec: [`../specs/task-rescue.md`](../specs/task-rescue.md). Status: done.
Each phase started with a failing test and ended with the four contract
commands green.

| # | Phase | Test first | Code |
|---|---|---|---|
| 0 | Contract 2.4: `max_rescues`, `Rescue`, `RescueOutcome`, `rescue_configured` | `rescues_pin_their_wire_form`, the version and envelope pins | `tinycomputer-bus` `agent/types.rs`, `version/`, `envelope/` |
| 1 | The rescuer: briefing, protocol, validation, repairs | `rescue/test.rs` | `tinycomputer-engine` `rescue/`, `planner::json_object` |
| 2 | `OpenRouter` adapter: `rescue_model`, reasoning effort | `the_open_router_planner_needs_a_key_and_never_prints_it` | `planner/openrouter.rs` (`open_router_rescuer`, `RESCUE_MODEL`) |
| 3 | The task controller: rescue before failing, resume, limits, record | `task/test/rescue.rs` | `task/mod.rs` (`rescued`, `rescue`, `record`, `rescue_outcome`, `drive` as a queue) |
| 4 | Module wiring, `Describe`, examples, docs | `a_planner_is_configured_from_private_configuration_only_with_a_key`, `describe_documents_every_member_and_its_examples_really_work` | `tinybus_module/dispatch.rs`, `task/describe.rs`, `task_live` (`TASK_RESCUES`, `TINYCOMPUTER_RESCUE_MODEL`), `docs/technical/tasks.md`, SKILL.md, the `start_task` schema |
| 5 | Guidance covering the steps after the failed one | `guidance_may_cover_the_steps_after_the_failed_one_but_never_a_stop_before`, `steps_the_guidance_covers_are_dropped_and_the_guard_is_kept` | `rescue/mod.rs` (`covered`, `guards`, `resumed`), `Rescue.covers` |

## Remaining

- The live evaluation is in
  [`../evals/2026-09-28-rescue.md`](../evals/2026-09-28-rescue.md). Both runs
  got past steps that had stalled every earlier run, and the furthest step
  reached moved from 18 to 20 of 26.
- Guidance now says how many following steps it covers (phase 5). Run 3
  used it (`covers: 2`) and reached step 25 of 26.
- Run 5 reached the payment checkpoint: the first complete Emirates run.
  Guidance for a failed `stop_before` must now keep one.
- Tasks now run with `include_values` on, secrets masked, so a `verify` of
  typed values can pass. The rescuer's screen text shows what fields hold,
  redacted (`Workspace::visible_text`).
