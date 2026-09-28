# Prices, times, and dates

Source: [`crates/tinycomputer-core/src/records/mod.rs`](../../../crates/tinycomputer-core/src/records/mod.rs),
[`src/dates/mod.rs`](../../../crates/tinycomputer-core/src/dates/mod.rs).

Two related jobs live here: reading values off a results page well enough to
rank them ("book the cheapest one"), and reformatting a date the caller
already gave so it lands in a form the way that form's field expects it.
Both are pure text parsing, no model involved, for the same reason as
everything else in this crate: if a question can be answered by arithmetic,
it should be, and asking Jev is reserved for things that genuinely need
judgment.

## `Record`: one card as named fields

A `Record` is just a field name mapped to the text shown for it, for example
`{"airline": "IndiGo", "price": "₹6,840"}`. `Record::from_pairs` builds one
from `(name, text)` pairs directly, which is mostly how tests and examples
construct them; in a live run, the flow runtime builds these from the
`Group`s described in
[lists and result cards](lists-and-result-cards.md), giving each field a
name once it has decided what the field probably means.

## `parse_price`: reading a price out of arbitrary text

`parse_price(text)` reads the first price it finds, handling currency
symbols (`₹`, `$`, `€`, `£`, `¥`), three-letter codes (`INR`, `USD`, …),
abbreviations (`Rs.`, `Rs`), and even a currency spelled out in words, the
way a screen reader would read it aloud ("7339 Indian rupees"):

```rust
use tinycomputer_core::parse_price;

let price = parse_price("IndiGo · ₹6,840").unwrap();
assert_eq!((price.amount, price.currency), (6840.0, Some("INR")));
```

The tricky part is deciding which punctuation mark is the decimal point,
because that convention differs by locale (`$1,234.56` versus
`1.234,50 €`). The rule: when both `.` and `,` appear, whichever one comes
*later* in the number is the decimal mark, and the earlier one is just a
thousands separator. When only `,` appears, it is treated as a decimal mark
if it is followed by exactly two digits and nothing else (`6,50`), and as a
thousands separator otherwise, which correctly reads the Indian grouping
convention (`1,23,456`) as a whole number rather than mangling it. A named
currency is always found by scanning for the *earliest* amount-before-a-name
in the text, not the first currency name in the module's own list, so
`parse_price` reports the first price the text actually shows rather than
whichever currency happens to be checked first internally.

Prices found without any currency marker at all (a bare "6840" with no
symbol, code, or name anywhere near it) are still read as a number, but with
`currency: None`, which matters for ranking, described below.

## `parse_clock`, `parse_duration`, `parse_stops`

Three smaller, more literal parsers:

```rust
use tinycomputer_core::{parse_clock, parse_duration, parse_stops};

assert_eq!(parse_clock("Departs 6:45 PM"), Some(18 * 60 + 45));
assert_eq!(parse_duration("2h 15m"), Some(135));
assert_eq!(parse_stops("Non-stop"), Some(0));
assert_eq!(parse_stops("1 stop · DEL"), Some(1));
```

`parse_clock` returns minutes after midnight, so times compare with plain
integer arithmetic rather than needing a real calendar. `parse_duration`
adds up every `Nh`/`Nm`-shaped chunk it finds ("2 hr 15 min", "135 min", "1h"
all work). `parse_stops` treats "Nonstop" and "Direct" as zero stops, and
otherwise reads the number immediately before the word "stop".

## `Criterion` and `rank`: picking "the best" deterministically

`Criterion::parse(text)` reads plain words like "cheapest" or "fewest stops"
into one of six criteria (`LowestPrice`, `HighestPrice`, `Earliest`,
`Latest`, `FewestStops`, `Shortest`), and returns `None` when the wording
does not map cleanly onto one of them, the deliberate cue for a caller to
fall back to asking a decision model instead of guessing.

`rank(records, criterion)` sorts a slice of `Record`s best-first for a given
criterion:

```rust
use tinycomputer_core::{Criterion, Record, rank};

let flights = [
    Record::from_pairs([("airline", "Vistara"), ("price", "₹7,210")]),
    Record::from_pairs([("airline", "IndiGo"), ("price", "₹6,840")]),
];
assert_eq!(rank(&flights, Criterion::LowestPrice), Some(vec![1, 0]));
```

Each criterion looks for a field whose *name* hints at what it needs
(`price`/`fare`/`cost`/`total` for price criteria, `depart`/`time`/`start`
for time criteria, and so on), and falls back to scanning every field's text
if no name matches. For price specifically, the fallback scan only accepts a
value that actually showed a currency, so a flight number like `6E-2135`
never gets mistaken for a price just because it contains digits. A record
whose value cannot be read at all is not dropped from the ranking; it is
placed after every readable one, in its original order, so a caller always
gets back a full, stable ordering rather than a filtered list.

`rank` returns `None`, not an empty ranking, when *no* record in the whole
set could be read for that criterion, the same "hand this to a model
instead" signal `Criterion::parse` gives for wording it does not recognize.
That is the whole point of the module: ranking by an unambiguous criterion
never needs a model, but the moment the data genuinely cannot support it,
the caller finds out rather than getting a plausible-looking but meaningless
order back.

## Dates: writing a caller's date the way a form asks for it

A caller supplies a date of birth once, typically as something unambiguous
like `2000-01-31` or `31 January 2000`. A booking form's masked date field,
though, wants it typed in whatever layout that field's own hint describes
("enter date of birth in DD-MM-YYYY format"). `dates/mod.rs` bridges the two.

`parse_date(text)` reads a date only when it is unambiguous: numeric year
first or last with a numeric month (`2000-01-31`, `2000/1/1`), or any order
once the month is spelled out, even abbreviated (`31 Jan 2000`,
`January 31, 2000`). A form like `01/02/2000`, which genuinely means
different days depending on the country reading it, is deliberately never
read, guessing wrong there is worse than not guessing.

```rust
use tinycomputer_core::{Date, parse_date};

let day = Date { year: 2000, month: 1, day: 31 };
assert_eq!(parse_date("2000-01-31"), Some(day));
assert_eq!(parse_date("31 Jan 2000"), Some(day));
assert_eq!(parse_date("January 31, 2000"), Some(day));
assert_eq!(parse_date("01/02/2000"), None);
```

`date_pattern(hints)` finds which layout (`DD-MM-YYYY`, `MM/DD/YY`, and
eight others, most specific first) the first matching hint actually names,
and `reformat_date(value, hints)` puts the two together: parse the caller's
value, find the layout the field's hints ask for, and retype it in that
layout.

```rust
use tinycomputer_core::reformat_date;

let hints = ["Date of Birth", "Please enter date of birth in (DD-MM-YYYY) format"];
assert_eq!(reformat_date("2000-01-31", hints).as_deref(), Some("31-01-2000"));
assert_eq!(reformat_date("Asha", hints), None);
```

`reformat_date` returns `None`, leaving the original value untouched,
whenever either half fails: the value is not a date it can parse
unambiguously, or none of the hints name a layout it recognizes. A caller
that gets `None` back should type the value exactly as given rather than
guess at a transformation.

## Why arithmetic first

Every function on this page exists to remove a small piece of judgment from
Jev's plate. A model asked "which of these five fares is cheapest" is slower
and, being a model, is not guaranteed to get simple arithmetic exactly right
every single time. A parser either reads the value or honestly says it
could not, and either way the caller knows exactly where it stands. See
[how tinycomputer decides](../../how-it-works.md) for how this fits into
the wider split between what code decides and what Jev is asked.
