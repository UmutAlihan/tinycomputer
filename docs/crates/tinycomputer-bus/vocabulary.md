# The shared vocabulary

Source: [`crates/tinycomputer-bus/src/vocabulary/types.rs`](../../../crates/tinycomputer-bus/src/vocabulary/types.rs)

A handful of small enums show up across many different payloads: which region
of the screen a command addresses, which mouse button to click, which
modifier key to hold. Rather than let each payload family invent its own copy
(and its own subtly different spelling), they live in one place here.

## Why these enums mirror the engine instead of importing it

Each of these types corresponds to a type inside the vendored `agent-desktop`
engine. It would be simpler to just reuse the engine's own enum. This crate
deliberately does not, and the reason is in the module's own doc comment:

> The module crate converts each one into its `agent-desktop-core` counterpart
> with an exhaustive `match`, so a variant added here without a conversion
> fails that build.

If `tinycomputer-bus` imported the engine's enum directly, a caller naming a
surface would not need the engine at all until suddenly, transitively, it
did — pulling a platform accessibility backend into every host that only
wanted to spell `Surface::Window`. Keeping a separate, hand-mirrored enum
here means the contract crate stays down to its two dependencies (`serde`,
`serde_json`), and it means a variant the engine adds cannot silently reach a
caller without a human deciding how to convert it: the `match` in
`tinycomputer-desktop` that does the conversion is exhaustive and has no
wildcard arm, so adding a variant on either side without the other is a
compile error, not a runtime surprise.

## `Surface`

The region of an application's accessibility tree a command addresses.
`Surface::Window` — the tree rooted at an application window — is the
ordinary case and the default. The rest name transient or system-owned
regions that are not children of any window and so cannot be reached by
descending from one: `Focused`, `Menu`, `Menubar`, `Sheet`, `Popover`,
`Alert`, `Desktop`, `Taskbar`, `SystemTray`, `QuickSettings`,
`NotificationCenter`, `Toolbar`, `Dock`, `Spotlight`, `MenuBarExtras`,
`SystemTrayOverflow`, `StartMenu`, `ActionCenter`.

Not every platform implements every surface. Asking for one it does not have
fails with `PLATFORM_NOT_SUPPORTED` and lists the surfaces that are actually
supported in the error's `details`. `ListSurfaces` (see
[Members and names](members-and-names.md)) reports which ones are currently
reachable for a given application before you ask for one.

On the wire, `Surface` is snake_case: `"window"`, `"system_tray"`,
`"menu_bar_extras"`, `"notification_center"`.

## `Direction` and `MouseButton`

`Direction` (`Up`, `Down`, `Left`, `Right`) is used by `Scroll`. `MouseButton`
(`Left` the default, `Right`, `Middle`) is used by `MouseClick`,
`HoldMouseRequest`, and `MouseWheelRequest`.

These two are the exception to the snake_case rule above: they carry no
`#[serde(rename_all = ...)]`, so the wire form is the plain Rust variant
name, PascalCase: `"Down"`, `"Middle"`. This is one of the small,
individually-tested facts a host has to get right (see
`crates/tinycomputer-bus/src/vocabulary/test.rs`), so a caller building
these by hand should double check rather than assume every enum here follows
the same casing.

## `Modifier`

`Meta` (Command on macOS, the Windows key elsewhere), `Ctrl`, `Alt` (Option on
macOS), `Shift`. `Meta` also accepts the alias `"Cmd"` on decode, matching an
alias the engine itself recognizes, so a caller who thinks in macOS terms can
write `"Cmd"` and land on the same variant a caller who wrote `"Meta"` would.
It always re-encodes as the canonical `"Meta"`, never as the alias it was
decoded from.

## `ClipboardFormat`

The pasteboard flavor a `ClipboardGet` asks for: `Auto` (whatever the
pasteboard currently holds), `Text` (the default), `Image` (written to a file
and reported by path, never inlined), `FileUrls`. Snake_case on the wire:
`"file_urls"`.

## `ElementProperty` and `ElementStateProperty`

`ElementProperty` is what `Get` can read: `Text`, `Value`, `Title`, `Bounds`,
`Role`, `States`. `ElementStateProperty` is what `Is` can test: `Visible`,
`Enabled`, `Checked`, `Focused`, `Expanded`, `Selected`. Both are snake_case
on the wire. See [Members and names](members-and-names.md) for how `Get` and
`Is` use them, and note the distinction the crate draws for `Is`: a state
that does not apply to an element's role (asking whether a button is
"checked") is reported as inapplicable, not folded into `false`.

## `StatePredicate`

Used by `Find` to require a state token, optionally negated:

```json
{ "token": "enabled" }
```

means "must be enabled" (`expected` absent defaults to `true`), while

```json
{ "token": "enabled", "expected": false }
```

means "must not be enabled." The two convenience constructors mirror this:
`StatePredicate::set("enabled")` for the first, `StatePredicate::expect("enabled",
false)` for the second.
