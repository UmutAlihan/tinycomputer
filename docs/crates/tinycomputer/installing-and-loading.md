# Installing and loading the module

This page is for whoever operates a TinyBus host and wants to give it
tinycomputer's desktop and browser members. If you are instead building this
workspace from source to develop it, see the root
[`README.md`](../../../README.md) and [`CLAUDE.md`](../../../CLAUDE.md).

## What a release archive contains

Each GitHub release publishes one archive per platform, named
`tinycomputer-<version>-<platform>.<extension>`, for example
`tinycomputer-0.2.1-ubuntu-24.04-x86_64.tar.gz`. Inside, flat, no
subdirectories:

- the compiled library itself: `libtinycomputer.so` on Linux,
  `libtinycomputer.dylib` on macOS, `tinycomputer.dll` on Windows;
- `modules.toml`, a one-line TOML file mapping that library's filename to its
  SHA-256 digest;
- `LICENSE` and `MODULE.md`, the module-facing readme quoted in the crate
  README;
- on macOS only, `agent-desktop-macos-helper`, a small helper binary the
  vendored engine's clipboard support only trusts when it sits right next to
  the loaded module. Without it, `ClipboardSet`, `ClipboardClear`, and the
  flow runtime's paste fallback fail on macOS specifically;
- on macOS and Windows, `tinycomputer-cursor-overlay` (or `.exe`), the window
  that draws the agent's on-screen cursor. Point the `cursor.overlay`
  configuration key at it, or leave it beside the module and it is found
  automatically. See [configuration.md](configuration.md#cursor).

Keep every file in the archive together when you copy it into wherever your
TinyBus module directory lives. The library on its own is missing the
checksum entry that lets TinyBus refuse a tampered or mismatched file, and on
macOS it is also missing the helper it needs for clipboard operations.

Every release also gets a separate `checksum.toml` asset at the release level
(not inside any one archive), mapping every archive filename in that release
to its own SHA-256 digest. This is what TinyBus's GitHub release loader checks
before it even downloads an archive, so a corrupted or replaced upload is
caught before extraction rather than after.

## Loading directly from a release

If your host uses TinyBus's own release loader, this is the whole thing:

```sh
tinybus modules load-github \
  https://github.com/tinyhumansai/tinycomputer/releases/tag/v0.2.1 \
  tinycomputer-0.2.1-ubuntu-24.04-x86_64.tar.gz \
  <archive-sha256>
```

You supply the release tag, the exact archive filename for your platform, and
the archive's SHA-256 digest (from `checksum.toml`, or from wherever you got
told to trust it). TinyBus downloads that specific archive, verifies it
against the digest you supplied and again against the SHA-256 baked into
`modules.toml`, extracts it, and loads the library. Two checks, not one,
because the digest you pass protects against a release that changed after you
looked at it, and the `modules.toml` digest protects against a library file
that was swapped inside an otherwise-intact archive.

## Loading a file you already have

If you built the module yourself, or copied an archive down by hand, point
your host's module loader at the extracted directory (library plus
`modules.toml`, and the macOS helper if relevant). The loader's job is the
same either way: check the library file's digest against `modules.toml`
before calling into it.

## Verifying a build before you trust it

`crates/tinycomputer-examples/src/bin/verify_module.rs` is the same check the
release pipeline runs on every platform before an archive is accepted. It
loads a compiled `cdylib` through TinyBus's real dynamic loader (not the
in-memory transport the unit tests use), waits for it to claim
`ai.tinyhumans.tinycomputer.Desktop`, and calls `Version` on it, chosen
because `Version` needs no granted permission and touches no other
application, so a failure here means the artifact itself is broken, not that
the machine running the check is unconfigured.

```sh
cargo build --release --lib --package tinycomputer
cargo run --package tinycomputer-examples --bin verify_module -- \
  target/release/libtinycomputer.dylib
```

A successful run prints the module name and how many members it serves. If
you are building your own distribution pipeline around this module, running
this check against your own build output before shipping it is the same
insurance the official release gets.

## What happens if the library or the digest is wrong

TinyBus's module loader is deliberately strict here: this is trusted,
in-process, native code that, once loaded, can read any window on the machine
and drive any application on it, so the loader would rather fail to start
than load something unverified. A missing `modules.toml` entry, a renamed
library file, or a digest mismatch all fail the load rather than falling back
to loading it anyway. Restart the host after replacing a loaded module;
nothing here supports hot-swapping the library underneath an already-running
process.

## Next

[configuration.md](configuration.md) covers what to put in the JSON blob you
hand the loader once the module is loaded, and
[calling-it.md](calling-it.md) covers what to do with it after that.
