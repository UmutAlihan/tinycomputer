# Releases

## Versioning

The whole workspace releases as one version. Every member crate inherits it
with `version.workspace = true` from the root `[workspace.package]`, so
`tinycomputer` and `tinycomputer-bus` are always the same version number even
though only `tinycomputer` is packaged and shipped.

Separately, `CONTRACT_VERSION` in `tinycomputer-bus` (a `(major, minor)` pair,
distinct from the crate version above) tracks the wire contract itself:
adding a member or an optional field is a minor bump, and changing a wire
form, removing a member, or renaming one, including the interface name
itself, is a major bump. The interface was last renamed at contract 2.0, from
`ai.tinyhumans.tinydesktop.Desktop` to today's
`ai.tinyhumans.tinycomputer.Desktop`, when the repository itself was renamed.
[`../../technical/specs/desktop-module-contract.md`](../../technical/specs/desktop-module-contract.md)
is the authoritative history of every contract bump.

Nobody hand-edits the crate version in `Cargo.toml`. The release workflow
owns that field entirely.

## How a release actually happens

Releases run from `.github/workflows/release.yml`, triggered by hand
(`workflow_dispatch`) with one of four choices: `patch`, `minor`, `major`, or
`current`.

Dispatching `patch`, `minor`, or `major`:

1. Runs the full validation suite (`cargo fmt --all -- --check`,
   `cargo clippy --all-targets --all-features -- -D warnings`,
   `cargo build --all-targets --all-features`, `cargo test --all-features`),
   plus a check that every source file keeps 90% line coverage, plus a
   rustdoc build with warnings promoted to errors.
2. Computes the next version from the chosen bump.
3. Updates the root `[workspace.package]` version and `Cargo.lock`.
4. Opens a pull request with that version bump, so branch protection can run
   its required checks on it like any other change.

Nothing is tagged or published yet at this point. A human reviews and merges
that version pull request first.

Dispatching `current` after that merge:

1. Revalidates the now-checked version on `main`.
2. Creates the `vX.Y.Z` tag if it does not already exist (or reuses it, which
   is what makes `current` safe to redispatch if a release was interrupted
   partway through).
3. Builds `tinycomputer` as a native module for every platform in the matrix
   below.
4. Verifies each built module through `verify_module`, loading the actual
   compiled `cdylib` through TinyBus's real dynamic loader and calling
   `Version` on it, before that platform's archive is assembled at all.
5. Packages each platform's archive: the library, `modules.toml` with its
   SHA-256 digest, `LICENSE`, `MODULE.md`, and, on macOS, the clipboard helper
   and cursor overlay binary described in
   [installing-and-loading.md](installing-and-loading.md).
6. Uploads every archive as a build artifact, then a final job downloads all
   of them, generates one `checksum.toml` covering every archive in the
   release using TinyBus's own `modules checksum` tool, and creates an
   immutable GitHub release with `--verify-tag`.
7. As a last gate, downloads the published Ubuntu x86_64 archive back down
   through TinyBus's actual GitHub release loader and calls `Version` on it
   over a real in-memory bus, using
   `crates/tinycomputer-examples/src/bin/verify_github_release.rs`. This is
   deliberately the same call `verify_module` used locally, chosen again
   because `Version` needs no granted permission and touches no other
   application, so this step tests that the published, downloaded artifact
   works, not whether the runner happens to be configured a particular way.

If any of that fails, the release is not created (or, for the last step, is
created but flagged as broken by a failing workflow run), rather than an
archive going out that was never actually loaded and called.

## The platform matrix

Sixteen archives per release, checked at the point in `release.yml` that
insists exactly that many exist before generating `checksum.toml`:

Native runners, verified through `verify_module` on the real OS:

| Platform | Runner | Target |
| --- | --- | --- |
| Ubuntu 22.04 x86_64 | `ubuntu-22.04` | `x86_64-unknown-linux-gnu` |
| Ubuntu 22.04 arm64 | `ubuntu-22.04-arm` | `aarch64-unknown-linux-gnu` |
| Ubuntu 24.04 x86_64 | `ubuntu-24.04` | `x86_64-unknown-linux-gnu` |
| Ubuntu 24.04 arm64 | `ubuntu-24.04-arm` | `aarch64-unknown-linux-gnu` |
| macOS 15 x86_64 | `macos-15-intel` | `x86_64-apple-darwin` |
| macOS 15 arm64 | `macos-15` | `aarch64-apple-darwin` |
| macOS 26 x86_64 | `macos-26-intel` | `x86_64-apple-darwin` |
| macOS 26 arm64 | `macos-26` | `aarch64-apple-darwin` |
| Windows Server 2022 x86_64 | `windows-2022` | `x86_64-pc-windows-msvc` |
| Windows Server 2025 x86_64 | `windows-2025` | `x86_64-pc-windows-msvc` |
| Windows 11 arm64 | `windows-11-arm` | `aarch64-pc-windows-msvc` |

Distro containers, built on an Ubuntu runner but inside the named container
image, and still verified through `verify_module` for that container's own
`.so`:

| Platform | Container |
| --- | --- |
| Fedora 43 x86_64 | `fedora:43` |
| Fedora 43 arm64 | `fedora:43` |
| Fedora 44 x86_64 | `fedora:44` |
| Fedora 44 arm64 | `fedora:44` |
| Arch Linux rolling x86_64 | `archlinux:base-devel` |

Every runner also carries a check that the actual `rustc` host target matches
what the matrix entry claims before building anything, so a runner image
change upstream that quietly swaps architectures fails the build rather than
shipping a mislabeled archive.

TinyBus itself is a pinned submodule dependency, `vendor/tinybus`, used to
build and to run its own module-checksum and release-verification tools
during this workflow. It is never itself published as a release asset from
this repository; only `tinycomputer` is.

## Next

[installing-and-loading.md](installing-and-loading.md) covers what to do
with one of these archives once it exists, and
[`../../technical/specs/tinybus-module-release.md`](../../technical/specs/tinybus-module-release.md)
is the underlying spec this whole pipeline satisfies.
