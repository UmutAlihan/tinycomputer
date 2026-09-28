# Linux lab for browser automation: Rust plus Playwright's Chromium and its
# system libraries. Tests that launch a browser, or that write and run fake
# browser scripts, run here instead of on the host. See docs/technical/docker-lab.md.
FROM mcr.microsoft.com/playwright:v1.63.0-noble

ARG RUST_TOOLCHAIN=stable

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential ca-certificates curl git pkg-config \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN curl -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal \
            --default-toolchain "$RUST_TOOLCHAIN" --component clippy,rustfmt \
    && chmod -R a+w "$RUSTUP_HOME" "$CARGO_HOME"

# agent-browser and the lab find Chromium through this path.
ENV PLAYWRIGHT_BROWSERS_PATH=/ms-playwright
WORKDIR /src
