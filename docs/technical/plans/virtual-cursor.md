# Virtual cursor: implementation plan

Spec: [`../specs/virtual-cursor.md`](../specs/virtual-cursor.md).

## Tasks

1. **`tinycomputer-cursor` crate.** Add `geometry/`, `rng/`, `pace/`,
   `glide/`, `protocol/`, `animate/`, `sprite/` (with `png`), and `screen/`
   (with `ProcessOverlay`). Write every module's tests first.
2. **The `tinycomputer-cursor-overlay` binary** in the same crate, under
   `src/bin/` and behind the `overlay` feature:
   - `driver/`, testable without a display;
   - `platform/macos.rs` (AppKit);
   - `platform/windows.rs` (Win32 layered window);
   - `platform/other.rs`.

   The crate carries its own lint table with `unsafe_code = "deny"`, and the
   library itself sets `#![forbid(unsafe_code)]`.
3. **Core.** Add `uses_pointer`.
4. **Desktop.** Add `Desktop::with_cursor`, and make `execute_desktop` glide
   onto boxed pointer targets.
5. **Browser.** Add `BrowserSurface::with_cursor` and `surface/cursor.rs`
   (viewport-to-screen mapping and `show_cursor`).
6. **Module.** Add `cursor_config` in `dispatch.rs`. Build one `ScreenCursor`
   and share it between the desktop and the runner.
7. **Release.** Package the helper beside the module on macOS and Windows.
8. **Examples.** Add `cursor_demo`, which tours the cursor.
9. **Docs.** Update `MODULE.md`, `README.md`, `docs/architecture.md`,
   `AGENTS.md`, and `tinybus_module/README.md`.

## Verification

```sh
cargo test -p tinycomputer-cursor --all-features
cargo clippy -p tinycomputer-cursor --features overlay --target x86_64-pc-windows-msvc --all-targets -- -D warnings
cargo build -p tinycomputer-cursor --features overlay
cargo run -p tinycomputer-examples --bin cursor_demo -- calm 2
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
```

## Checklist

- [x] Tasks 1–9
- [ ] A Linux overlay (follow-up)
