# tinydesktop-core

The shared, engine-free domain of tinydesktop's surfaces. Everything here is
deterministic and behaves the same for a desktop application and a web page.

| Module | Holds |
|---|---|
| `keymap/` | `Key` and `Platform`: shortcuts named by purpose, spelled per OS for agent-desktop and agent-browser |
| `safety/` | `consequence` (reversible, irreversible, payment) and `payment_evidence` (card fields, checkout pages) |
| `records/` | `Record`, price, clock, duration and stop parsers, and deterministic `rank` |
| `surface/` | the `Surface` trait decision loops run against, the `Screen` and `Candidate` it observes, fingerprints, change notes, verified text delivery |
| `facts/` | `Facts`: the caller's values, shared or secret, redaction, and masking secrets back to `${name}` |

The safety checks are what stop a run before anything irreversible or paid
happens, whatever a model decided (`docs/specs/unified-agent.md`).
