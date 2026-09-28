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
| 4 | Module wiring, `Describe`, examples, docs | `a_planner_is_configured_from_private_configuration_only_with_a_key`, `describe_documents_every_member_and_its_examples_really_work` | `tinybus_module/dispatch.rs`, `task/describe.rs`, `task_live` (`TASK_RESCUES`, `TINYCOMPUTER_RESCUE_MODEL`), `docs/tasks.md`, SKILL.md, the `start_task` schema |

## Remaining

- A live run on the Emirates plan, where the date calendar stays open and
  flight cards are not read, to see whether a rescue gets past either.
