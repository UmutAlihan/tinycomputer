# Screenshots, held outputs, and downloads

Code: `crates/tinycomputer-browser/src/outputs/mod.rs`,
`crates/tinycomputer-browser/src/sessions/mod.rs` (the `screenshot`,
`read_output`, `release_output`, `sweep_outputs`, `list_downloads`, and
`wait_download` methods on `Browser`).

## Why screenshots are held, not just returned

A screenshot is an image sitting in the module's own memory, and the module
is loaded into somebody else's process. Returning the raw bytes straight
back on the same call would work for a small viewport capture, but a
full-page capture of a long article is easily several megabytes, and a bus
frame has a limit. So screenshots are captured, held, and handed back as a
small reference (`OutputRef`) that a caller then reads in chunks: the same
handle-and-chunk protocol tinybrowser used before it, kept because hosts
written against it keep working unchanged.

Three things can go wrong with anything held in memory this way, and none
of them require anyone to be doing anything malicious: a caller takes a
screenshot and never reads it, a caller takes hundreds of them in a loop, or
one screenshot of an unusually long page is bigger than anyone expected. The
`OutputStore` (`outputs/mod.rs`) exists to defend against exactly those
three, and only those three.

## Taking a screenshot

`Browser::screenshot` writes the capture to a scratch file (agent-browser
writes screenshots to disk, not straight back over the command channel),
reads the bytes back, deletes the scratch file immediately, checks the size
against the cap, reads the image's own width and height out of its file
header, and stores the bytes under a fresh, unguessable id.

`ScreenshotRequest` covers the viewport by default; set `full_page: true`
to capture the whole scrollable document, or give a `target` to capture one
element. The format defaults to PNG, lossless and correct when a model is
going to read text out of the image, with JPEG and WebP available for a
long full-page capture where layout matters more than legibility, plus an
optional `quality` (1 to 100, checked at conversion time) for the lossy
formats.

Sizing is checked *before* decoding: `outputs::within_cap` estimates the
decoded size from the base64 length agent-browser reports and refuses with
`Error::LimitExceeded` before the bytes are ever held twice in memory.

## The bounds

| Bound | Value | Enforced by |
|---|---|---|
| Max held outputs | `MAX_OUTPUTS` = 16 | `OutputStore::insert`, evicting the oldest first once full |
| Max size of one output | `MAX_OUTPUT_BYTES` = 64 MiB | checked both before and after decoding |
| Time to live | `TTL` = 5 minutes | `OutputStore::expire` |
| Sweep interval | `SWEEP_INTERVAL` = 60 seconds | a host is expected to call `Browser::sweep_outputs` on this cadence |
| Max bytes per read | `MAX_CHUNK` = 4 MiB | `OutputStore::read` |

Eviction, when the store is full, always drops the *oldest* held output,
never the newest. That is the right direction: the screenshot a caller just
took is the one it is about to ask for.

Expiry runs both reactively (every store operation calls `expire` first)
and, expected from a host, on a fixed timer via `Browser::sweep_outputs`
every `SWEEP_INTERVAL`. Without the timer, an output taken and then never
collected again would sit in memory, in somebody else's process, until the
next unrelated call happened to trigger a cleanup, which might never come.

## Reading a held output

`Browser::read_output(id, offset, len)` returns up to `MAX_CHUNK` bytes,
base64-encoded, plus whether this chunk reaches the end (`eof`). A caller
reads a large screenshot by repeating this call with an advancing offset
until `eof` is true. Reading with an `offset` past the end of the output is
refused with `Error::InvalidInput`; reading an id that never existed, or has
since expired, is `Error::NoSuchOutput`.

## Releasing early

`Browser::release_output` drops a held output before its TTL, and, like
closing a session, succeeds whether or not the output was still there. A
caller cleaning up after itself should never have to check first.

## Downloads

Downloads are a different shape of "wait for something to finish, then hand
back where it landed." `Browser::wait_download` sends agent-browser's
`waitfordownload` command with a scratch path and a timeout (the request's
own `timeout_ms`, or the session's default), and turns the result into a
`DownloadInfo`: a sequence number, a synthetic id, the file's size on disk,
and the path it was written to. `Error::Timeout` comes back if nothing
finishes before the deadline.

Every completed download is remembered on the session (`Session::downloads`)
and returned by `Browser::list_downloads`, a simple in-memory list, not a
subscription; a caller that wants to know about the *next* download calls
`wait_download` again.

Where downloads actually land is controlled by
`SessionOptions::download_dir`: an absolute directory the module creates if
it does not exist and never removes when the session closes (unlike a
session's screenshot scratch space, which is cleaned up on close). Relative
paths are refused, because they would resolve against whatever working
directory the module's host process happens to have, which nothing here can
predict or control.
