# Lists and result cards

Source: [`crates/tinycomputer-core/src/surface/groups.rs`](../../../crates/tinycomputer-core/src/surface/groups.rs).

Search results, hotel listings, flight fares, an inbox: a huge share of the
pages tinycomputer needs to act on are really one thing repeated many times.
This module turns that repetition into structure, so a flow can say "pick
the third card" or "rank these by price" instead of reasoning about forty
individual buttons and labels.

## How a repeated card is recognized

Nothing here is told in advance that a page has a list on it. Instead, a
surface labels each item in a repeated container with an ordinal, like
`listitem #3`, and every node under that item carries the same label
somewhere in its `path`. `result_groups(screen)` (and the more general
`result_families(screen)`) look for the ancestor path under which two or
more same-role ordinal containers repeat (the most containers first, the
deeper one winning any tie) and treat that as *the* list. A page can have
more than one repeating thing on it (a strip of selectable dates above a list
of flights, say); `result_families` returns every one of them, longest
first, while `result_groups` just gives you the biggest.

Each card becomes a `Group`:

```rust
pub struct Group {
    pub label: String,             // e.g. "listitem #3"
    pub fields: Vec<String>,       // the card's visible text, in reading order, no repeats
    pub primary: Option<Candidate>, // the control that opens or selects the card
}
```

## Building a card's fields

A card's `fields` are built from every node under its container, actionable
or not, in document order:

- An actionable element (something you could click or type into) contributes
  its name or description unconditionally, and its held value only when the
  caller asked to see values (`include_values`), the same rule
  [`element_line`](surfaces-and-screens.md) applies everywhere else in this
  crate.
- A ref-less text node (something with no actions of its own, like a plain
  price label) contributes its name or description unconditionally too, since
  most of a card's price, airline name, and departure time text arrives this
  way and would otherwise never be visible at all. Its held *value*, though,
  is gated the same way as an actionable element's, unless the text node
  sits inside a rich-text area or a token field (`webarea`, `document`,
  `textbox`, `searchbox`, `combobox`, `textarea`), in which case its value
  mirrors that field's actual contents rather than naming anything of its
  own, and stays private on the same terms as any other field content.

Duplicate text within one card is dropped, and whitespace is collapsed, so a
label that wraps across two lines in the accessibility tree still reads as
one clean line of text.

## Picking the card's primary control

Not every button inside a card should count as "the" way to open it. A
result card might have a "Share" icon, a "Save" star, and a "Select" button;
only the last one is what a person would actually click to act on that
result. `prefers()` chooses whichever actionable element's name contains one
of a short list of opener words, `select`, `book`, `choose`, `view`,
`details`, `continue`, `reserve`, `deal`, `see`, over one that does not. If
nothing in the card matches an opener word, the group is left with whatever
actionable element came last, which still gives a caller *something* to act
on rather than nothing.

## Example

```rust
// a results page with three flight cards, each a "listitem #N" holding a
// price text node and a "Select" button
let cards = result_groups(&screen);
assert_eq!(cards.len(), 3);
assert_eq!(cards[1].fields, vec!["IndiGo".to_owned(), "₹6,840".to_owned()]);
assert_eq!(cards[1].primary.as_ref().unwrap().name.as_deref(), Some("Select"));
```

## Where this feeds into the rest of the system

The digest ([the screen digest](the-screen-digest.md)) uses this same
card-building logic to render a whole list region as one line per card
instead of one line per element, so a decision model sees "card 2: IndiGo ·
₹6,840 → Select" rather than every element inside every card. `Group` is
deliberately name-free (its `fields` are just a list of text, in order): it
is the flow runtime, in `tinycomputer-engine`, that turns a page's `Group`s
into named `Record`s once it knows what the fields probably mean. From
there, [ranking by price, time, or stops](prices-times-and-dates.md) becomes
plain arithmetic, see that page for how "book the cheapest one" turns into
a deterministic sort instead of a question sent to Jev.
