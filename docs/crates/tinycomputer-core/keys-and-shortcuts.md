# Keys and shortcuts

Source: [`crates/tinycomputer-core/src/keymap/mod.rs`](../../../crates/tinycomputer-core/src/keymap/mod.rs).

A flow written for tinycomputer should never have to know that macOS uses
`cmd` and everyone else uses `ctrl`, or that agent-desktop's `Press` member
wants `cmd+a` while agent-browser's `press` command wants `Meta+a` for the
exact same "select all". `keymap` is where that translation lives, so a flow
step can just say `Key::SelectAll` and let the surface work out how to spell
it.

## `Platform`

Three variants: `MacOs`, `Windows`, `Linux`. `Platform::current()` reads the
build's target OS at compile time and treats anything that is not macOS or
Windows as Linux. Most callers never construct a `Platform` themselves; it
flows in from whatever `Platform::current()` reports, or from a browser
surface's own sense of what OS conventions a page should follow.

## `Key`

`Key` names fifteen shortcuts by what they do, not by which physical key they
press: `SelectAll`, `Copy`, `Cut`, `Paste`, `Undo`, `Redo`, `New`, `Find`,
`Back`, `Forward`, `Refresh`, `Settings`, `Confirm`, `Dismiss`, `NextField`.

Two methods spell a `Key` for the two engines:

- `Key::desktop(platform)` returns the combination for agent-desktop's
  `Press` member, such as `"cmd+a"` on macOS or `"ctrl+a"` elsewhere.
- `Key::browser(platform)` returns the key for agent-browser's `press`
  command, such as `"Meta+a"` on macOS or `"Control+a"` elsewhere.

Both return `Option<String>`, because not every shortcut makes sense on every
target:

| Key | Desktop | Browser |
|---|---|---|
| `Settings` | `cmd+,` on macOS, `None` on Windows/Linux (no standard shortcut) | always `None` (a web page has no settings menu) |
| `New` | always spelled | always `None` |
| everything else | spelled on every platform | spelled on every platform |

```rust
use tinycomputer_core::{Key, Platform};

assert_eq!(Key::Paste.desktop(Platform::Windows).as_deref(), Some("ctrl+v"));
assert_eq!(Key::Back.desktop(Platform::MacOs).as_deref(), Some("cmd+["));
assert_eq!(Key::Settings.desktop(Platform::Windows), None);
assert_eq!(Key::Settings.browser(Platform::MacOs), None);
```

A few shortcuts are spelled differently between the two engines even for the
same key and platform — `Redo` on macOS is `cmd+shift+z` for the desktop but
`Meta+Shift+z` for the browser (capitalization aside, both are "the same
shortcut"), and `Back`/`Forward` use bracket keys on desktop macOS
(`cmd+[`/`cmd+]`) but arrow keys everywhere in the browser
(`Alt+ArrowLeft`/`Alt+ArrowRight`), because that is what each underlying
engine's own convention actually expects.

## Why this is worth its own module

A flow author writing "select all the text in this field" should get the
same behavior whether that field lives in a native macOS app or in a text
box on a web page, without writing a platform check into every flow. Keeping
the mapping in one place also means that if agent-desktop or agent-browser
ever change how they expect a shortcut spelled, there is exactly one place
in this repository that needs to change (see this repository's rule that
`vendor/agent-desktop` and `vendor/agent-browser` are fixed elsewhere and
picked up here as a gitlink bump — nothing about *how a key is spelled* lives
upstream, only how the underlying press is carried out).
