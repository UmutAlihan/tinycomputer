# Sessions and their lifecycle

A session is one browser tab (technically one `DaemonState`) that
`tinycomputer-browser` drives from open to close. Everything else in the
crate, navigating, reading, clicking, screenshotting, happens on a session.
This page covers how one starts, what it costs, and how it ends.

Code: `crates/tinycomputer-browser/src/sessions/` (`mod.rs` opens and closes
sessions; `page.rs` and `artifacts.rs` hold the calls on them). The wire shape of the
options below lives in `crates/tinycomputer-bus/src/browser/session/types.rs`.

## Opening a session

`Browser::open_session` takes a `SessionOptions` and returns a `SessionInfo`.
Every field of `SessionOptions` has a default, so the common case is just
`SessionOptions::default()`: a headless Chrome at a 1280x800 viewport, no
origin restriction, a 30 second default timeout.

What opening a session actually does, in order:

1. Refuses if eight sessions are already open (see below).
2. Mints a session id (`s-1`, `s-2`, ...).
3. Asks the engine to open, which does not launch anything yet.
4. Sends an explicit `launch` command built from the options.
5. Sends an explicit `viewport` command.
6. Reads the page's URL and title once, so the returned `SessionInfo` already
   reflects where the browser landed (usually `about:blank`).

Step 4 matters more than it looks. Without an explicit launch, agent-browser
would auto-launch from whatever `AGENT_BROWSER_*` environment variables
happen to be set on the *host process* that loaded the module, not this
session's configuration. Sending `launch` explicitly, every time, is what
makes sessions in the same process independent of each other and of
whatever the host happens to have set.

## Launching versus attaching

`SessionOptions::endpoint` is the switch between the two:

- **Launching** (`endpoint: None`, the default): the module starts its own
  Chrome, Chromium, or Chrome for Testing (`SessionOptions::executable` picks
  a specific binary; otherwise the module finds the first one it can). This
  browser is this session's alone.
- **Attaching** (`endpoint: Some(...)`): the module connects to a browser
  that is already running, at a DevTools endpoint
  (`http://127.0.0.1:9222`, or a raw `ws://`/`wss://` browser socket),
  instead of starting a new one.

Attaching is how a host reuses a browser it manages itself, or how a sandbox
points the module at a Chrome running in a different container. It also
changes what closing the session does: `Browser::close_session` always sends
an engine `close`, but for an attached session that only disconnects; the
browser it did not launch keeps running. `SessionInfo::launched` tells you
which case you are in.

## Headed and headless

`SessionOptions::headless` defaults to `true`: a module loaded into a daemon
usually has no display to draw on. A headed session (`headless: false`, or
any attached session, since `SessionInfo` treats attaching as visible
regardless of the flag) is one with an actual window on screen. That matters
for one thing in particular: the on-screen cursor. `BrowserSurface` only
glides the shared agent cursor onto elements when the session has a window to
draw over (`BrowserSurface::shows_cursor`, see [surface.md](surface.md)).
Headless or headed, every action is performed identically either way; the
cursor is purely cosmetic.

## The viewport

`SessionOptions::viewport` is not a cosmetic setting. Accessibility snapshots,
sight's element scanning, and every coordinate-based interaction all depend
on layout, and layout depends on viewport size. A headless default of
800x600 (a common one for many tools) makes a responsive site serve its
mobile layout, and a flow driven against that tree looks for navigation items
that a person using the site's desktop layout would never see. The crate's
default (`Viewport::default`) is 1280x800 at 1x scale: wide enough that
mainstream sites serve their desktop layout, small enough that a full-page
screenshot of an ordinary article stays under the output size cap (see
[outputs-and-downloads.md](outputs-and-downloads.md)).

## The origin allow-list

`SessionOptions::allowed_origins`, when non-empty, is the only set of origins
this session may navigate to. An entry is either a full origin
(`https://example.com`) or a host with a leading dot to also admit its
subdomains (`.example.com`, which becomes agent-browser's `*.example.com`
pattern). A navigation outside the list is refused before the browser is
asked to make a request: the cheapest and most certain point to refuse it.

This is a guard rail, not a sandbox. The scheme is not enforced (the engine
filters by host only), and a page that is already loaded can still make its
own requests to other origins; the allow-list only stops *this session's*
navigations. See "Invariants" in
[`../../technical/specs/unified-agent.md`](../../technical/specs/unified-agent.md)
for how this fits into the wider safety picture.

## Serialization and concurrency

Calls on one session are serialized: each session is a
`tokio::sync::Mutex<Session>`, so two calls against the same session never
race, and a caller does not need to serialize its own calls. Different
sessions run independently of each other, with one exception. When the
crate is built with the `agent-browser` feature (the real, linked engine),
commands across *every* session in the process share one more lock
(`crates/tinycomputer-browser/src/linked/mod.rs`, the static `ENGINE`
mutex), because agent-browser keeps one process-wide piece of state: which
frame a selector resolves against. That lock is on top of, not instead of,
the per-session serialization.

## The eight-session limit

`MAX_SESSIONS` is 8. `open_session` checks the current count first and
refuses with `Error::LimitExceeded` rather than opening a ninth. This is a
guard against a module that is loaded into somebody else's long-running
process: nothing here assumes a caller will always remember to close what it
opens, so the limit exists as a backstop rather than a target to plan
around. If your use case genuinely needs to page through more than eight
sites at once, close sessions you are done with (or use tabs within a
session, outside the scope of this crate today) instead of pushing past the
limit.

## Scratch space

Each session gets a private directory under the crate's scratch root
(by default, a folder named for the module's process id under the system
temp directory) where agent-browser writes screenshots and downloads for the
crate to pick up. The directory is created lazily on first use and removed
when the session closes.

## Closing a session

`Browser::close_session` removes the session from the table, sends a `close`
command (best-effort: a session that is already dead has nothing left to
close), and removes its scratch directory. Closing a session that does not
exist succeeds silently: a caller retrying a close should never have to tell
"already gone" apart from "never existed".

## Listing sessions

`Browser::list_sessions` returns every open session's `SessionInfo`, sorted
by id, each carrying the page it was last seen on. This is a live inventory
of what the module is holding, not a history: once a session closes, it is
gone from this list.
