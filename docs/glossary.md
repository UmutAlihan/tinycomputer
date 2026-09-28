# Glossary

Terms used across the tinycomputer docs, in alphabetical order.

**Accessibility tree.** The list of controls an operating system exposes for
screen readers. tinycomputer reads desktop apps through it. See
[How it sees the screen](seeing-the-screen.md).

**Approval.** Your go-ahead for one irreversible action. A task asks with
`needs_approval`, and you answer with `ContinueTask` and `approve: true` or
`false`.

**Attention.** The check, before each step, for anything in the way (cookie
banners, pop-ups, a calendar left open) and clearing it first.

**Backtrack.** Undoing a wrong press and trying the next best candidate. See
[Catching mistakes](catching-mistakes.md).

**Brief.** What choosing questions are told about the whole task: the goal,
shared details, rules, the plan, recent choices. See
[How it decides](how-it-decides.md).

**Budget.** Limits on actions, questions to Jev, time, and rescues for a
task. Pausing never refills it.

**Checkpoint.** Two meanings. A task status: the task stopped where a person
must take over, usually at payment. Or, inside a step: a saved snapshot of
the screen, used to undo a mistake.

**Choice.** A Jev question that picks one of several labelled options.

**Constraints.** Limits on where a task may go: which surfaces, which
websites, how it treats payment.

**Deliberation.** Looking at how much the evidence supports an answer, and
asking more when it doesn't. Levels: `deep` (default), `standard`, `off`.

**Digest.** A compact summary of a busy screen, used by the wide strategy.

**Duel.** Comparing the top candidates two at a time, in both orders, to
settle a close pick.

**Facts.** Your details, handed to a task to type into forms. Shared facts
may be shown to Jev; secret facts never are. See
[Safety and privacy](safety-and-privacy.md).

**Flow.** A list of plain-language steps. See [Writing flows](writing-flows.md).

**Framing.** One of the several copies of a question sent to Jev, each worded
or ordered a little differently. See **Voting**.

**Grounding.** Finding the one element on screen that a step means.

**Grounding memory.** Hints about which control worked for a step last time.
See [Memory and saving](memory-and-saving.md).

**Harness.** The Rust code around Jev that reads the screen, decides which
questions to ask, acts, and checks. It holds all the safety rules. The flow
runtime is the part of the harness that runs flows.

**Jev.** The decision model. It answers yes/no, scale, and pick questions,
never writes text, and never plans.

**Journal.** An optional, detailed log of every Jev call and its timing. See
[Watching a run](watching-a-run.md).

**Noul.** A yes/no Jev question, answered as a probability.

**Origins.** The websites a task's browser may load.

**Planner.** The optional language model that turns a plain-language task
into a flow.

**Primitives.** The low-level members (`Snapshot`, `Click`, `SetValue`, and
so on) where the caller decides everything.

**Ref.** A short id for one element in one read of the screen, like
`@s8f3k2p9:e1`. A stale ref fails safely instead of clicking the wrong thing.

**Reflection.** Checking after a `choose` step that the screen shows exactly
the choice asked for, and repairing it once if not.

**Rescue.** Asking a reasoning model for new steps after a step fails. See
[Rescues](rescue.md).

**Saved values.** Text a flow read, extracted, or picked. The run remembers
them and returns them at the end.

**Score.** A Jev question answered on a scale, like "how far along is the
step, on five levels".

**Sight.** How tinycomputer reads a web page by what's drawn rather than by
its markup.

**Stop before.** A step that finds an irreversible control and stops in front
of it.

**Strategy.** How questions are asked: `narrow` (many small ones, the
default) or `wide` (one bigger one per turn).

**Surface.** Something tinycomputer can read and act on: the desktop or a
browser tab. A workspace joins them.

**Task.** A whole job handed over in one call, run in the background. See
[Giving it a task](giving-it-a-task.md).

**TinyBus.** The message bus a host uses to load tinycomputer and call its
members. See the [module crate docs](crates/tinycomputer/README.md).

**Voting.** Asking a question in several framings at once (seven by default)
and averaging the answers, to cancel out bias.

**Workspace.** The desktop and a browser session joined into one surface for
a task.
