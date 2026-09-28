# Docker lab

Anything in this crate that opens a browser should run inside a Linux
container, not directly on your Mac. `scripts/docker-lab` gives you that
container: Rust plus Playwright's Chromium and the system libraries it
needs, mounted against this repository's source.

## Why bother

Two separate reasons, and both are real:

1. **Repeatability.** A browser test that writes throwaway shell scripts and
   executes them (which the agent-browser test suite does, for its fake
   `chrome` fixtures) behaves differently on every developer's Mac. Inside
   a container it always behaves the same way, and it touches nothing on
   your machine except the source tree you mounted in.
2. **Safety.** Those same throwaway `chrome` shell scripts have tripped
   macOS malware protection when run directly on a Mac. Inside the
   container, none of that protection is in the way, and nothing escapes
   the container to trigger it on the host.

If a browser example needs a display you can actually see (a headed run,
or anything driving a macOS application alongside the browser), the
container cannot help, because it has no display. Those still run on the
host with `scripts/lab` or `task_live` directly.

## Usage

```sh
# a plain shell in the container, repository mounted at /src
scripts/docker-lab

# this repository's contract commands, but on Linux
scripts/docker-lab -- cargo test --all-features

# the browser stack against the travel fixture, no Jev
scripts/docker-lab -- crates/tinycomputer-examples/fixtures/run browser_fixture

# a full booking task against the fixture, with live Jev
scripts/docker-lab -- crates/tinycomputer-examples/fixtures/run task_fixture

# a saved live task against a real airline site
scripts/docker-lab -- crates/tinycomputer-examples/tasks/run kashmir

# a different checkout, e.g. agent-browser's own crate
scripts/docker-lab --src ../agent-browser --workdir cli -- cargo test --profile ci
```

| Option | Meaning |
|---|---|
| `--src DIR` | the directory mounted read-write at `/src` (default: this repository) |
| `--workdir SUBDIR` | where under `/src` the command actually runs |
| `--rebuild` | rebuild the container image even if one already exists |

## What to know before you rely on it

- The image is rebuilt automatically whenever `docker/lab/Dockerfile`
  changes; its tag is derived from the file's hash.
- Build output goes to `<workdir>/target/docker-lab`, next to the host's own
  `target/`. Linux and macOS artifacts never share a directory, so a
  container run never pollutes or invalidates your host build, and
  `tune-box --clean-targets` still finds and cleans it.
- The cargo registry is cached across runs in the `tinycomputer-lab-cargo`
  Docker volume. If it ever gets into a bad state, `docker volume rm
  tinycomputer-lab-cargo` clears it.
- `PLAYWRIGHT_BROWSERS_PATH=/ms-playwright` is set inside the container, and
  agent-browser searches that path for a Chromium binary. The container adds
  `--no-sandbox` itself, since a sandboxed Chromium needs privileges a
  container does not have by default.
- `.env` at the repository root is loaded the same way `scripts/lab` loads
  it, but only `OPENROUTER_API_KEY` and `TINYCOMPUTER_LAB_MODEL` are
  forwarded into the container, by name, and neither is ever printed. If a
  live task or fixture example needs another variable from
  [`.env.example`](../../../.env.example), export it yourself before
  invoking `scripts/docker-lab`, or check whether the script needs updating
  to forward it.

See [`docs/technical/docker-lab.md`](../../technical/docker-lab.md) for the
exact list of forwarded variables and any detail this page leaves out.
