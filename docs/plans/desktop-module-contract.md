# Implement The Desktop Module Contract

Linked specification: [`../specs/desktop-module-contract.md`](../specs/desktop-module-contract.md)

1. Vendor `agent-desktop` as a pinned submodule and take `agent-desktop-core`
   plus one platform adapter crate per target in `crates/tinycomputer`.
2. Define the contract in `crates/tinycomputer-bus`: the member names, the
   response envelope, the shared enumerations, and one payload family per group
   of members. Pin each payload's serde representation in a test first.
3. Convert the contract onto the engine's argument types in
   `desktop/convert.rs`, with an exhaustive `match` per enumeration and a test
   asserting each variant against the engine's own spelling.
4. Build `Desktop`: configuration parsing, the shared run path, the permission
   preflight, and one method per member split by family across sibling files.
   Cover the configuration parser and the envelope mapping before the members.
5. Serve the members in `tinybus_module/dispatch.rs`, in the order of
   `names::METHODS`, each handing its work to `spawn_blocking`. Assert the
   dispatch table and the embedded manifest against that list.
6. Exercise the interface over the in-memory bus, then load the compiled
   `cdylib` through the real dynamic loader with
   `cargo run -p tinycomputer-examples --bin verify_module`.
7. Run the four validation commands and the coverage gate, and update
   `README.md`, `MODULE.md`, and `ROADMAP.md` in the same change.
