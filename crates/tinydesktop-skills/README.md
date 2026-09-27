# tinydesktop-skills

The guidance a model needs to drive tinydesktop's task members, packaged
with the contract it describes:

| Asset | Holds |
|---|---|
| `skills/tinydesktop/SKILL.md` | the loop (Describe, StartTask, AwaitTask, ContinueTask), what each status asks of the caller, how to write flows, and the payment rule |
| `schemas/start_task.schema.json` | `StartTask`'s input schema and the task members |

A host installs `skill_assets()` into its skill directory. Tests fail if the
skill or schema falls out of step with `tinydesktop-bus`.
