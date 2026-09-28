# tinycomputer-skills

tinycomputer is a decision model (Jev) based harness for desktop and browser
automation: Jev answers small closed questions about what to press, and the
harness reads the screen, asks, checks answers, acts, and verifies. This
crate sits outside that loop. It is the leaflet that teaches an external
calling agent how to hand the harness a task in the first place, not part of
the Jev decision loop that runs once a task starts.

This crate is not code that does anything at runtime. It is text: the
instructions a calling agent needs to use tinycomputer's task API well, plus
the JSON Schema that describes the shape of that API's main request. A host
installs both into wherever it keeps skills for its own agent to read.

## Who needs this

- **Anyone embedding tinycomputer behind an agent** (a coding assistant, a
  chat product, an automation platform) that needs to teach that agent how
  to call `StartTask`, follow it to completion, and handle the pauses along
  the way.
- **Anyone writing or reviewing that guidance**, since the whole crate is
  effectively one Markdown file and one schema file, versioned together with
  the contract they describe.

If you are trying to understand what a task actually *does* once started,
this is the wrong place: that is
[Giving it a task](../../giving-it-a-task.md). This crate is the leaflet
handed to the caller, not the machine itself.

## Why this exists as its own crate

The module crate, `tinycomputer`, is the capability: it is the thing that
actually runs `StartTask`, `AwaitTask`, and the rest. `tinycomputer-skills`
holds no module, no bus connection, and no credentials, only text, so it can
be versioned and shipped independently of anything that talks to a real
desktop or browser. Keeping the guidance in its own crate also means it can
be checked against the real contract in `tinycomputer-bus`: a test in this
crate fails if the skill or the schema ever falls out of step with what the
module actually serves, so a stale instruction sheet cannot ship silently.

## A tour of the crate

| Path | What it holds |
|---|---|
| [`skills/tinycomputer/SKILL.md`](../../../crates/tinycomputer-skills/skills/tinycomputer/SKILL.md) | the loadable skill document itself: the loop, what each task status asks of the caller, how to write flows, and the payment rule. See [What a skill file is](what-a-skill-file-is.md). |
| [`schemas/start_task.schema.json`](../../../crates/tinycomputer-skills/schemas/start_task.schema.json) | `StartTask`'s input schema, and the full list of task members it is used alongside. See [Walking the schemas](walking-the-schemas.md). |
| [`src/lib.rs`](../../../crates/tinycomputer-skills/src/lib.rs) | the crate's whole public surface: two constants (`SKILL`, `START_TASK_SCHEMA`) and `skill_assets()`, which lists every file a host should install. |

## Installing it

A host installs `skill_assets()` into its own skill directory:

```rust
use tinycomputer_skills::skill_assets;

for asset in skill_assets() {
    // asset.path is relative to the host's skill root, e.g.
    // "tinycomputer/SKILL.md" and "tinycomputer/schemas/start_task.schema.json"
    // asset.contents is the file's full text.
}
```

That is the entire API. There is nothing to configure and nothing that can
fail at runtime; the only thing that can go wrong is a host writing the
files to the wrong place, which is on the host, not this crate.

## Pages in this folder

- [What a skill file is](what-a-skill-file-is.md): what `SKILL.md` actually
  is, why it exists as Markdown with front matter, and how an agent
  discovers and loads it.
- [Walking the schemas](walking-the-schemas.md): a short tour of
  `start_task.schema.json`, field by field.
- [Using it as an agent](using-it-as-an-agent.md): the practical loop a
  calling agent follows, with the statuses it needs to react to.

## See also

- [Giving it a task](../../giving-it-a-task.md): the same loop, explained
  for a person rather than for a calling agent.
- [`docs/technical/tasks.md`](../../technical/tasks.md): the task
  controller itself, in full technical detail.
- [Writing flows](../../writing-flows.md): how to write the `flow` a task
  needs when no planner is configured.
- [Safety and privacy](../../safety-and-privacy.md): how facts, secret
  facts, and the payment rule actually get enforced underneath this
  guidance.
- [Glossary](../../glossary.md): short definitions of terms used across
  these pages.
