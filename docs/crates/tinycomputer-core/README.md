# tinycomputer-core

tinycomputer is a Jev-based harness: Jev, the decision model, decides what to
press, and the Rust harness does everything else. `tinycomputer-core` is the
small, boring crate at the bottom of that harness. It holds the ideas that a
desktop application and a web page share: what a screen looks like once you
have observed it, what counts as a dangerous button, how to type a value the
way a form wants it typed, and how to keep a card number away from Jev. None
of it knows how to actually click anything. It has no window system, no
browser, no network calls, and no model in it. Everything in this crate is a
plain function you could run in a unit test with no computer attached, which
is exactly the point: rules that decide "is this a payment button" or "is
this the cheapest flight" should not depend on luck, timing, or asking a
model twice and hoping for the same answer.

If you are new to the project, read
[how tinycomputer works](../../how-it-works.md) first. This page assumes you
already know, roughly, that a "surface" is a thing tinycomputer can look at
and act on (a desktop app or a browser tab), and that "Jev" is the decision
model that answers small yes/no/pick-one questions about what is on the
screen.

## Who reaches for this crate

Mostly nobody reaches for it directly. `tinycomputer-desktop` and
`tinycomputer-browser` each implement the `Surface` trait defined here, and
`tinycomputer-engine`'s flow runtime is written against that trait rather than
against either adapter. So this crate is where you look when you want to
understand a rule that both adapters follow, or when you are adding a new
surface and need to know what it is required to hand back.

You will care about this crate specifically if you are:

- writing or debugging the digest a decision model sees on each turn
- adding a new "this button is dangerous" or "this page wants a card" rule
- figuring out why a typed value did not stick, or why a card number ended up
  in a log
- ranking search results ("book the cheapest one") and wondering how the
  arithmetic works
- deciding what a caller's date of birth should look like once it lands in a
  masked form field

## A tour of the folders

| Folder | What lives there | Read next |
|---|---|---|
| `src/surface/` | The `Surface` trait, the `Screen` and `Candidate` types it hands back, fingerprints and change notes, verified text delivery, result cards, and the screen digest | [Surfaces and screens](surfaces-and-screens.md), [The screen digest](the-screen-digest.md), [Typing text reliably](typing-text-reliably.md), [Lists and result cards](lists-and-result-cards.md) |
| `src/safety/` | `Consequence` (reversible, irreversible, payment) and the payment and human-gate detectors | [Safety and payment detection](safety-and-payment-detection.md) |
| `src/facts/` | `Facts`: a caller's shared and secret values, and masking secrets back to `${name}` | [Facts and secrets](facts-and-secrets.md) |
| `src/keymap/` | `Key` and `Platform`: a shortcut named by what it does, spelled per operating system | [Keys and shortcuts](keys-and-shortcuts.md) |
| `src/records/`, `src/dates/` | `Record`, price/time/duration/stop parsers, deterministic ranking, and calendar dates typed the way a form asks for them | [Prices, times, and dates](prices-times-and-dates.md) |
| `src/error/` | The crate's one `Error` enum and `Result` alias | mentioned where it matters, see [Facts and secrets](facts-and-secrets.md) |

## Why this crate exists as its own thing

Two reasons, both load-bearing.

First, determinism. A run that asks a model "is ₹6,840 cheaper than ₹7,210"
is slower, costs money, and can occasionally get simple arithmetic wrong.
Parsing the price and comparing two floats never does. The rule in this crate
is: if a question can be answered by a parser, it is, and Jev is asked only
about the things that genuinely need judgment. See
[how tinycomputer decides](../../how-it-works.md) for where that split sits in
the bigger picture.

Second, safety. The checks that stop a run before money moves or before an
irreversible action fires live here, in code with no model in it, so that a
model's opinion about a button can never be the only thing standing between a
task and a real purchase. `docs/technical/architecture.md`'s "Safety, in one
place" section describes how the rest of the system leans on this. For the
plain-language version, see
[safety and privacy](../../safety-and-privacy.md) and this crate's own
[safety and payment detection](safety-and-payment-detection.md).

## Where to go for the engineering detail

This crate's own [rustdoc](../../technical/README.md) and
[`crates/tinycomputer-core/README.md`](../../../crates/tinycomputer-core/README.md)
are the source of truth for exact behavior; the pages here explain the same
ideas in plainer language, with examples, and link back to source files
instead of repeating them. For the architecture these pieces sit inside, see
[`docs/technical/architecture.md`](../../technical/architecture.md).
