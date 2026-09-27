# tinydesktop-engine

The agent runtime behind tinydesktop's agentic members. It composes the surface
adapters with Jev, TypeSafe's decision model:

| Entry point | Member | What it does |
|---|---|---|
| `resolve_intent` | `ResolveIntent` | grounds one described element, optionally acting on it |
| `run_goal` | `RunGoal` | a bounded, scoped goal with visible success predicates |
| `run_flow` | `RunFlow` | a UI-agnostic intent flow grounded by small Jev decision loops |
| `validate_flow`, `flow_guide` | `ValidateFlow`, `FlowGuide` | check and document flows offline |

`JevRuntime::configure` builds the Jev client from the module's private
configuration. The crate holds no bus; `tinydesktop` serves these functions over
TinyBus.

- `src/agentic/README.md` covers `RunGoal` and `ResolveIntent`.
- `src/agentic/flow/README.md` covers intent flows.
- `docs/specs/unified-agent.md` describes the browser surface and the task
  controller that are joining this crate.
