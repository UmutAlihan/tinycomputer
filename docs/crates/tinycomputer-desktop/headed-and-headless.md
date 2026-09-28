# Headed and headless input

## The default: headless

A `Desktop` built with `Desktop::new()` runs headless. A ref action, such as
`click` or `type_text`, goes through the platform's accessibility API rather
than through a synthesized mouse click or keystroke. That has a useful
consequence: it does not steal focus, does not move the visible cursor, and
does not touch the pasteboard as a side effect, unless the member is one
that explicitly uses the pasteboard (see the clipboard family in
[Members by family](members-by-family.md)).

Practically, this means an automated run can proceed on a machine while
someone else is using it. Their mouse does not jump around, their currently
focused window does not change out from under them, and their own clipboard
is not clobbered mid-task. This is the configuration a background agent
wants, and it is why it is the default rather than something that has to be
turned on.

```rust
use tinycomputer_desktop::Desktop;

let desktop = Desktop::new();
assert!(!desktop.is_headed());
```

## Headed mode: when the real cursor has to move

Some interactions genuinely cannot be done through the accessibility API
alone. A handful of applications and controls only respond correctly to a
real, focused, on-screen interaction. `Desktop::with_headed(true)` relaxes
the headless guarantee for exactly the ref actions that need it, letting
them take focus and move the real cursor when the platform requires it to
make the action work.

```rust
use tinycomputer_desktop::Desktop;

let desktop = Desktop::new().with_headed(true);
assert!(desktop.is_headed());
```

This is the exception, not the rule, and it is opt-in for a reason: it
disturbs whoever else might be at the machine. Reach for it only when a ref
action is not working headless and the target genuinely needs the real
cursor, not as a default setting for every run.

## The input family bypasses headlessness by design

The members in the `input` family
([`input.rs`](../../../crates/tinycomputer-desktop/src/desktop/input.rs):
`press`, `hover`, `drag`, `mouse_move`, `mouse_click`, `mouse_wheel`, and the
always-failing `key_down`/`key_up`/`mouse_down`/`mouse_up`) are the other
planned exception. They synthesize real keyboard and mouse input on
purpose, regardless of headed or headless mode, because that is their whole
job: reaching a canvas, a game, or any custom-drawn control the
accessibility tree cannot describe at all. `mouse_click`, in particular, is
documented in the code as "the last resort for an element the accessibility
tree cannot describe": it clicks whatever is actually at a given screen
coordinate, which is a different and weaker guarantee than clicking a named
ref, and should be reached for only when a ref genuinely does not exist for
the target.

## Choosing between them

| Situation | Use |
|---|---|
| The element has an accessibility role and a ref | An interaction member (`click`, `type_text`, and so on), headless, the default |
| The above fails because the app needs a real focused interaction | The same interaction member, with `Desktop::with_headed(true)` |
| There is no accessibility element at all: a canvas, custom control, or raw screen coordinate | An input member (`mouse_click`, `press`, `drag`) |

## Where this lives in the code

`headed` is a field on `Desktop` itself
(see [`desktop/mod.rs`](../../../crates/tinycomputer-desktop/src/desktop/mod.rs)),
threaded into the `CommandContext` every command runs under. `Desktop`'s
own rustdoc documents this same distinction under "Headless by default." See
[Members by family](members-by-family.md) for the full input family and
[Desktop as a surface](desktop-as-a-surface.md) for how a decision loop
picks between a ref-based action and a coordinate-based one when it acts.
