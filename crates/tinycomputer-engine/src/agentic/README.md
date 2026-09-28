# Native Jev task loop

`RunGoal` keeps a bounded desktop task inside the module. The caller supplies
the app, optional exact window ID and title, prepared values, allowed actions and targets,
and visible success conditions. `policy.rs` builds Jev's closed choices and
gates the selected operation and target probabilities independently. `screen.rs`
collects bounded accessibility observations. `verify.rs` checks the caller's
conditions without asking Jev to judge its own completion.
Verification inspects visible snapshot nodes even when the accessibility
engine gives them no actionable ref (for example Calculator's result text).
Only ref-bearing actionable nodes are offered as Jev action targets.
The screen-change fingerprint excludes snapshot-qualified refs and includes
visible text, so a new snapshot ID alone cannot masquerade as progress. When
an action reports `delivered_unverified`, the loop gives the UI up to two
seconds to expose the requested success state before asking Jev to decide
again. It never repeats the delivered action during this settling window.
`name_contains` verifies a name fragment only under an exactly named ancestor
container, which permits checking a message in the active chat without
accepting a matching chat-list row.
Transient `APP_NOT_FOUND` and `WINDOW_NOT_FOUND` observations are retried for
at most three seconds with the same app and exact window ID. Permission errors
are returned immediately. A premature Jev `DONE` adds one corrective note and
gets one more bounded decision; only the accessibility predicates can complete
the task.
An unlabeled element can use the engine's exact `native_id.value` as its target
label; this identifier is retained in fresh-target comparison.

Each step observes, asks Jev to choose, reobserves the chosen target, acts once,
and observes again. A changed or ambiguous target stops before mutation. A
specified window ID is sent with every snapshot, including reobservation; a
missing or different window stops the task rather than selecting another one.
A possibly delivered failed or timed-out action stops as `action_uncertain`; the
module does not retry it. OS permission errors come from the underlying desktop
command preflight. With `require_confirmations: true` (the default), a
consequential action returns a one-use continuation handle. The host may set
`false` when it has already authorized continuous desktop execution within the
request's action scope.

The result carries executed turns, provider metrics, a stop reason, and compact
predicate evidence. The only value echoed for a value predicate is its
caller-supplied expected string or fragment after a match; other field content
is not returned. An absence predicate is deliberately unavailable because a
bounded accessibility snapshot cannot prove an element is absent.

Intent flows (`RunFlow`, `ValidateFlow`, `FlowGuide`) live in `flow/`; see
`flow/README.md`. They share only the Jev runtime and its error mapping with
the task loop above.

`sage/` lets Levanto Sage take the decisions in place of Jev, for measuring
it behind the same loops (`JevRuntime::sage`). It sends each Jev request to
Sage as one batch group and maps the answers back:

- a yes/no gets Sage's probability of yes;
- a choice gets Sage's per-option probabilities, normalised;
- a score gets a Sage five-level scale, spread back over the request's own
  levels.

Every call still goes through `JevRuntime::evaluate`. It is not reachable over
the bus, and since the examples drive the module over the bus, none of them
selects it any more; the recorded comparison is
`docs/technical/evals/2026-09-29-sage.md`.
