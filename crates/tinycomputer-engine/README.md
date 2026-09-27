# tinycomputer-engine

The agent runtime behind tinycomputer's agentic members. It composes the surface
adapters with Jev, TypeSafe's decision model:

| Entry point | Member | What it does |
|---|---|---|
| `resolve_intent` | `ResolveIntent` | grounds one described element, optionally acting on it |
| `run_goal` | `RunGoal` | a bounded, scoped goal with visible success predicates |
| `run_flow` | `RunFlow` | a UI-agnostic intent flow grounded by small Jev decision loops |
| `validate_flow`, `flow_guide` | `ValidateFlow`, `FlowGuide` | check and document flows offline |
| `Tasks` | `StartTask`, `AwaitTask`, `ContinueTask`, … | the task controller: runs a flow in the background, pauses for missing values and irreversible actions, always stops at payment |
| `Planner` | `PlanTask`, plain-language `StartTask` | turns a task into a flow with an LLM (`planner` feature); sees fact names, never values |
| `Workspace` | — | the desktop and the browser as one surface, so `browse` and `open` steps move a flow between them |

`JevRuntime::configure` builds the Jev client from the module's private
configuration. The crate holds no bus; `tinycomputer` serves these functions over
TinyBus.

- `docs/jev-harness.md` maps every layer between a request and a Jev call,
  and `docs/jev-journal.md` the debug journal (`TINYCOMPUTER_JEV_JOURNAL`)
  that records each call and timing to disk.
- `src/agentic/README.md` covers `RunGoal` and `ResolveIntent`.
- `src/agentic/flow/README.md` maps the flow runtime's files, and
  `docs/decision-loops.md` explains how it grounds each step.
- `docs/tasks.md` explains the task controller and the planner, and
  `docs/architecture.md` how the engine sits between the adapters and the
  module.
