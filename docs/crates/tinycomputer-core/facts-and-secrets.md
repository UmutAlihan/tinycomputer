# Facts and secrets

Source: [`crates/tinycomputer-core/src/facts/mod.rs`](../../../crates/tinycomputer-core/src/facts/mod.rs).

A task usually needs to know things about you: your name, your date of
birth, your email, and sometimes a card number or a passport number. `Facts`
is where those values live for the length of a run, and the rule it enforces
is simple to say and easy to get wrong without help: a decision model may be
briefed with your name, but it must never see your card number, not even for
a moment.

## Shared versus secret

Every fact is either **shared** or **secret**.

- A **shared** fact, name, date of birth, email, phone, is part of the
  brief Jev works from. Jev needs to be able to tell "Ms" from "Mr" or pick
  "Female" from a dropdown, and that requires actually seeing the value.
- A **secret** fact, a card number, a passport number, a password, a
  one-time code, is never shown to a model at all. A model only ever sees
  the fact's *name*, written as `${name}`; the actual value is looked up by
  a surface at the exact moment it types it, through `Facts::get`, and never
  passed through anything that talks to a language model.

A fact becomes secret in one of three ways, and only one of them is up to
the caller:

1. the caller explicitly marks it secret (`Facts::with_secrets`)
2. its *name* labels something sensitive, see `is_sensitive_name` below
3. its *value* looks like a real card number (passes the Luhn checksum)

A caller can make any fact secret. A caller cannot make a fact that matches
rule 2 or 3 shared, even by trying, `Facts::new` computes secrecy from the
name and value themselves, and there is no method that removes an entry from
that set once it is in.

```rust
use tinycomputer_core::Facts;

let facts = Facts::new([
    ("email", "sam@example.com"),
    ("card number", "4111 1111 1111 1111"),
]);
assert_eq!(facts.get("email"), Some("sam@example.com"));
assert!(facts.is_secret("card number") && !facts.is_secret("email"));
assert_eq!(facts.mask("paying with 4111111111111111"), "paying with ${card number}");
```

## `is_sensitive_name`

A short, fixed list of terms marks a fact secret purely by its name, no
matter what value it holds: card numbers, CVVs, expiry dates, PINs, OTPs,
passwords, passport numbers, SSNs, Aadhaar, PAN, IBAN, and bank account or
routing numbers. Matching is whole-word, so "date of birth" does not
accidentally match "pin" the way a naive substring search might:

```rust
use tinycomputer_core::is_sensitive_name;

assert!(is_sensitive_name("Passport Number"));
assert!(is_sensitive_name("card_cvv"));
assert!(!is_sensitive_name("date of birth"));
```

## `mask`: turning a value back into `${name}`

Once a value has left the model's view (typed into a field, say), it can
still show up in text coming back the other way, a screen reading back
what was typed, or a page confirming the last four digits. `Facts::mask`
replaces every secret value it finds, longest value first (so a shorter
value that happens to be a substring of a longer one is not replaced
prematurely and left with a dangling fragment), with `${name}`.

Card numbers get one more layer of protection: `mask` also mounts a second
pass that looks for the secret's *digits alone*, ignoring spaces and dashes,
whenever a secret has at least six digits. That is what lets a card number
you supplied as one unbroken string still get masked when a page happens to
show it back to you grouped in fours (`4111 1111 1111 1111`), and it is
deliberately gated at six digits so an ordinary three-digit price is never
mistaken for a fragment of a secret and masked by accident.

`Facts::redact` is the gentler sibling used for logs and traces rather than
anything shown to a model: it replaces *every* fact's value, shared or
secret, with `‹name›`, so a debug trace never has to carry someone's actual
email address or date of birth even when that value was never a secret in
the first place.

## The rest of the surface

- `Facts::merged(other)` combines two sets of facts, keeping a name secret if
  either side marked it so, merging never demotes a secret back to shared.
- `Facts::names()`, `Facts::shared()`, `Facts::secret_names()`,
  `Facts::missing(wanted)` give you the bookkeeping views a caller needs: what
  is known, what a model may be briefed with, which names are secret (so even
  their *existence* can be surfaced without their values), and which of a
  wanted list is not supplied at all.
- `Facts` implements `Debug` by hand, printing only the fact *names*, never
  values, so an accidental `{:?}` in a log line cannot leak a secret the way
  a derived `Debug` would.

## Why this crate, and not the engine

Masking has to happen at the boundary of *every* channel that could carry a
value back out: the brief sent to Jev, a step report, a trace, a journal
entry. Putting the rule in a shared, model-free crate means every one of
those callers gets the same guarantee by construction, rather than each
needing to remember to redact on its own. See
[memory and saving](../../memory-and-saving.md) for how facts persist across
a run, and [safety and privacy](../../safety-and-privacy.md) for the
plain-language version of the guarantee this module gives you.
