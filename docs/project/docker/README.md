# docker/

One Dockerfile, used to build a Linux environment for running the browser
stack in CI-like conditions on any machine, including a Mac.

| Path | What it is |
|---|---|
| `docker/lab/Dockerfile` | image for `scripts/docker-lab` |

## docker/lab/Dockerfile

Built from `mcr.microsoft.com/playwright:v1.63.0-noble`, which already
carries Playwright's Chromium and the system libraries it needs. On top of
that base image the Dockerfile adds:

- build essentials, `ca-certificates`, `curl`, `git`, and `pkg-config`
- a Rust toolchain installed with `rustup`, minimal profile, `clippy` and
  `rustfmt` components, defaulting to `stable` (overridable with the
  `RUST_TOOLCHAIN` build argument)
- `PLAYWRIGHT_BROWSERS_PATH=/ms-playwright`, which is how both agent-browser
  and the lab find Chromium inside the container

The working directory is `/src`, which is where `scripts/docker-lab` mounts
whatever source tree you point it at (this repository by default).

This image exists specifically for anything that launches a browser, or that
writes and runs throwaway browser automation scripts as part of a test. The
agent-browser test suite's fixtures do exactly that: they write fake `chrome`
shell scripts to a temporary directory and execute them, which can trip
macOS's own malware protection when run directly on a Mac. Running the same
tests inside this container avoids that, and keeps Linux-only build output
away from a macOS `target/` directory.

You never build or run this image directly; `scripts/docker-lab` does both,
tagging the image by a hash of the Dockerfile so an edit here produces a
fresh image on the next run. See
[`docs/project/scripts/README.md`](../scripts/README.md#scriptsdocker-lab)
for the script's usage, and
[`docs/technical/docker-lab.md`](../../technical/docker-lab.md) for the full
guide, including what runs well here versus on the native lab.
