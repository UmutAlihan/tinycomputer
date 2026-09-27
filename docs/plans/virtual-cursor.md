# Virtual cursor: implementation plan

Spec: [`../specs/virtual-cursor.md`](../specs/virtual-cursor.md).

## Tasks

1. **`crates/tinydesktop-cursor`.** Add the `error/`, `geometry/`, `rng/`,
   `pace/` and `glide/` modules (`aim`, `human_path`, `VirtualCursor`,
   `Glide`), with a `[workspace.dependencies]` entry. Write the tests first:
   - path endpoint and time;
   - overshoot and lateral bounds;
   - Fitts ordering;
   - aim containment;
   - glide continuity;
   - `off`.
2. **Core.** Add `tinydesktop_core::surface::uses_pointer`, tested over
   every operation.
3. **Browser.** Add `surface/cursor.rs`, containing the script, `script()`,
   `shows_cursor` and `show_cursor`, plus `with_cursor` and `cursor_pace` on
   `BrowserSurface`. Call `show_cursor` from `execute` for pointer
   operations and targeted typing. Write the tests first:
   - drawn before the click;
   - no input sent;
   - continuity;
   - headless, off, no box and non-pointer operations draw nothing;
   - a refused script.
4. **Module.** Parse `browser.cursor` in `dispatch.rs` (`BrowserConfig`),
   and apply it to each task's surface in `runner.rs`. Test that valid
   paces are accepted and anything else is refused.
5. **Docs.** Cover `browser.cursor` in `MODULE.md`, `README.md`,
   `docs/architecture.md` and `tinybus_module/README.md`, and add the
   crate to the listings.
6. **Follow-up.** Make the agent-desktop overlay's helper executable
   configurable upstream, then enable the overlay per session.

## Verification

```sh
cargo test -p tinydesktop-cursor
cargo test -p tinydesktop-browser surface
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
```

## Checklist

- [x] Tasks 1–5
- [ ] Task 6 (upstream agent-desktop)
