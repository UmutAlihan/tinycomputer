# What a skill file is

A "skill" here is a specific, small idea: a Markdown document, with a bit of
front matter at the top, written for a language model to read before it
tries to use a particular tool. It is not code, and it is not a prompt
template with slots to fill in. It is closer to a page out of a manual,
sized to fit in an agent's context and aimed at exactly one job.

## The front matter

`SKILL.md` starts like this:

```markdown
---
name: tinycomputer
description: Hand a task to the tinycomputer module and follow it to completion across web pages and desktop applications. Use when a person asks you to do something on their computer or on the web for them, such as finding and booking travel up to payment, filling a form, or drafting an email in Mail.
---
```

The `name` and `description` are what a host's agent framework typically
uses to decide, out of potentially many installed skills, which one to load
for a given request. The `description` is written to answer two questions
at once: what does this skill let the agent do, and when should the agent
reach for it. A test in this crate checks that the file starts with exactly
this front matter shape (`SKILL.starts_with("---\nname: tinycomputer\n")`),
so an edit cannot accidentally drop or mangle it.

## The body

After the front matter, the document reads like a short procedure, in this
order:

1. **The loop.** `Describe`, then `StartTask`, then `AwaitTask` in a cycle,
   then `TaskReport`. This is the same five-step loop covered for a human
   reader in [Giving it a task](../../giving-it-a-task.md), just written
   at the level of detail an agent making the actual calls needs: which
   member to call, and what its reply looks like (`{ok, data}` or
   `{ok: false, error: {code, message, hint, recoverable}}`).
2. **Starting a task.** How to fill in `task` versus `flow`, how `facts`
   and `secret_facts` work, and the payment rule: tinycomputer never pays on
   a person's behalf unless the caller explicitly asks for
   `fill_then_approve` and lists the sites it applies to.
3. **A table of every status** a task can return, and exactly what the
   agent should do for each one: ask the person, approve or decline, wait
   for a person, or read the final report.
4. **How to write flows that work**, for the case where no planner is
   configured and the calling agent has to describe the steps itself: one
   idea per step, and the specific step kinds (`browse`, `open`, `enter`,
   `pick`, `extract`, `read`, `stop_before`).
5. **One worked example**, a complete flight-booking flow, that a test in
   this crate actually parses and validates against the real
   `StartTaskRequest` type, so the example in the document is guaranteed to
   be a request the module would accept.

## Why Markdown, and why bundled with a schema

An agent reading a skill benefits from prose it can reason over directly:
"ask the person, then `ContinueTask` with `approve: true` or `false`" is
something a language model can follow without any parsing step. But prose
alone cannot pin down the exact shape of a request. That is what
`start_task.schema.json` is for, covered in
[Walking the schemas](walking-the-schemas.md): a machine-checkable
description of the same `StartTask` call the prose describes in words.
Shipping both together, and testing that they agree with each other and
with the real contract, is what keeps an agent from being taught a call
shape that the module has since changed underneath it.

## How a host actually uses it

The crate does not install anything on its own. A host reads
`skill_assets()` (see the [crate README](README.md)) and writes each
`SkillAsset`'s `contents` to `SkillAsset`'s `path`, relative to wherever
that host keeps its own skill files. From that point on it is the host's
agent framework, not this crate, that decides when to load `SKILL.md` into
a model's context. This crate's only job is to make sure the text and the
schema it ships are correct and in step with the contract, at build and
test time.
