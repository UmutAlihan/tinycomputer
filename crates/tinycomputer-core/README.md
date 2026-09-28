# tinycomputer-core

Part of [tinycomputer](../../README.md), a decision model (Jev) based harness for
desktop and browser automation, written in Rust. This crate is the shared ground the harness stands on: the screen model, keys, safety rules, and facts. Its user guide is
[`docs/crates/tinycomputer-core/`](../../docs/crates/tinycomputer-core/README.md).

The shared, engine-free domain of tinycomputer's surfaces. Everything here is
deterministic and behaves the same for a desktop application and a web page.

| Module | Holds |
|---|---|
| `keymap/` | `Key` and `Platform`: shortcuts named by purpose, spelled per OS for agent-desktop and agent-browser |
| `safety/` | `consequence` (reversible, irreversible, payment) and `payment_evidence` (card fields, checkout pages) |
| `records/` | `Record`, price, clock, duration and stop parsers, and deterministic `rank` |
| `surface/` | the `Surface` trait decision loops run against, the `Screen` and `Candidate` it observes, fingerprints, change notes, verified text delivery, and `digest`: a screen as regions, what is in front, lists as cards, noise collapsed, rendered within a byte budget |
| `facts/` | `Facts`: the caller's values, shared or secret, redaction, and masking secrets back to `${name}` |

The safety checks are what stop a run before anything irreversible or paid
happens, whatever a model decided (`docs/technical/specs/unified-agent.md`).
