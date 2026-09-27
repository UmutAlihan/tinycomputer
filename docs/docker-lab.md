# Docker lab

`scripts/docker-lab` runs a command in a Linux container that has Rust and
Playwright's Chromium with its system libraries (`docker/lab/Dockerfile`). Use it
for anything that launches a browser, or that writes and runs throwaway browser
scripts. That includes the agent-browser test suite, whose fixtures write fake
`chrome` shell scripts to temporary directories and execute them. Running those
on a Mac can trip macOS malware protection. Inside the container they touch
nothing on the host except the mounted source tree.

## Usage

```sh
# a shell in this repository
scripts/docker-lab

# this repository's contract commands on Linux
scripts/docker-lab -- cargo test --all-features

# another checkout, e.g. agent-browser's crate
scripts/docker-lab --src ../agent-browser --workdir cli -- cargo test --profile ci
```

| Option | Meaning |
|---|---|
| `--src DIR` | directory mounted read-write at `/src` (default: this repository) |
| `--workdir SUBDIR` | where under `/src` the command runs |
| `--rebuild` | rebuild the image even if it exists |

## Operational notes

- The image tag is derived from the Dockerfile's hash, so editing the
  Dockerfile builds a fresh image on the next run.
- Build output goes to `<workdir>/target/docker-lab`, beside the host's
  `target/`. Linux and macOS artifacts never share a directory, and
  `tune-box --clean-targets` still finds it.
- The cargo registry is cached in the `tinydesktop-lab-cargo` volume. Remove it
  with `docker volume rm tinydesktop-lab-cargo`.
- `PLAYWRIGHT_BROWSERS_PATH=/ms-playwright` is set. agent-browser searches that
  path for Chromium, and adds `--no-sandbox` itself inside a container.
- The container has no display. Headed browser runs, and anything that drives
  macOS applications, still need the host (`scripts/lab`).
