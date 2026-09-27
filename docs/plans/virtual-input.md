# Virtual input: implementation plan

Spec: [`../specs/virtual-input.md`](../specs/virtual-input.md).

## Assumptions

- Jev's contract is unchanged: operations target candidates, not pixels.
- agent-browser (tinyhumansai fork) already exposes `mousemove` with
  `inputMode`, `mousedown`/`mouseup` with tracked button state, `keyboard`,
  and `boundingbox`.
- agent-desktop's `mouse_move` posts real events in headed mode on macOS.

## Tasks

1. **Crate skeleton.** Create `crates/tinydesktop-input/` with the
   `[workspace.dependencies]` entry, `error/`, `geometry/` and `rng/`.
   *Test first:* the seed reproduces its sequence, and draws stay in range.
2. **Profiles.** Add `profile/` with names, parsing, and serde.
   *Test first:* round trip, unknown name refused, lowercase wire form.
3. **Plans.** Add `plan/`: `Step`, `Plan`, `InputSink`, `play` and `Pacer`.
   *Test first:*
   - pauses merge;
   - play order;
   - play stops at the first failure;
   - the pacer subtracts time already spent.
4. **Mouse.** Add `mouse/`: `aim`, `human_path` and `VirtualMouse`.
   *Test first:*
   - the path ends on target;
   - the overshoot bound holds;
   - Fitts ordering;
   - aim containment;
   - click, drag and hover plans.
5. **Keyboard.** Add `keyboard/`: cadence and `VirtualKeyboard`.
   *Test first:* exact text, Enter/Tab mapping, gap band, fast bigrams.
6. **Core.** Add `tinydesktop_core::surface::uses_pointer`.
   *Test first:* the pointer and non-pointer operation sets.
7. **Browser.** Add `surface/input.rs` (`PageInput`), plus `with_motion`,
   `approach`, `click_at`, `type_keys` and `type_into` in `surface/mod.rs`.
   Pin the old mapping under `Instant`.
   *Test first:*
   - glide before click;
   - pointer continuity;
   - no box means no glide;
   - typed keys;
   - fill fallback;
   - covered-card presses.
8. **Desktop.** Add `motion` and the shared `Pointer` in `desktop/mod.rs`,
   the config key, and `surface/pointer.rs` (`RealPointer`, `glide_plan`).
   *Test first:*
   - config parsing and refusals;
   - bounds parsing;
   - a glide only when headed and moving;
   - the sink refuses non-move primitives.
9. **Module.** Make `runner.rs` pass `desktop.motion()` to each task's
   browser surface. Document `motion` in `MODULE.md`, `README.md`,
   `docs/architecture.md` and `tinybus_module/README.md`.
10. **Upstream (follow-up PRs).** In agent-desktop: path-streamed moves, the
    overlay `with_path`, `helper_path`, a pointer-position query, and
    Windows `SendInput` plus overlay. Then bump the gitlink, ship a
    `tinydesktop-cursor` helper, and enable the overlay from `motion`
    config.

## Verification

```sh
cargo test -p tinydesktop-input
cargo test -p tinydesktop-browser surface
cargo test -p tinydesktop-desktop surface
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
```

For the live check, run `scripts/lab` headed against the browser fixture and
watch a hover menu open as the pointer glides in.

## Checklist

- [x] Tasks 1–9
- [ ] Task 10 (upstream agent-desktop)
