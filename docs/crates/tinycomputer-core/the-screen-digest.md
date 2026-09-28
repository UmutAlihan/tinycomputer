# The screen digest

Source: [`crates/tinycomputer-core/src/surface/digest/`](../../../crates/tinycomputer-core/src/surface/digest/mod.rs).

An `observe` call can come back with hundreds of elements: a page's header,
its search form, thirty legal links in the footer, a cookie banner sitting
on top of everything, and a list of twenty near-identical flight cards. Handed
to Jev as a flat list, that is a lot of noise for a model that only needs to
answer one small question this turn. The digest is what turns that flat list
into something closer to how a person would actually read the same screen:
what is in the way right now, what the page is mostly about, and what is
safely background.

## Regions

`digest(screen)` groups a screen's candidates into `Region`s: sets of
elements that share the same ancestors, such as a toolbar, a form, a dialog,
or a result list. A region too large to take in at once (more than 24
elements, by default) is split one ancestor level deeper, recursively, up to
10 levels down. A list of repeated cards is always kept as one region rather
than being split apart, even if splitting it would otherwise shrink the
pieces below the size limit: a result list is one thing, not twenty.

Every region gets three properties:

- `kind`: `Front`, `Content`, or `Noise` (see below)
- `name`: the last two ancestor labels it sits under, such as
  `region "Cookie consent" > button "Accept all"`'s parent chain, or
  `"top level"` when it has no shared ancestor worth naming
- `members`: the indices into `Screen::candidates` it holds, in reading order

### What counts as "in front"

A region is `Front` when its elements sit inside a role that overlays the
page (`sheet`, `dialog`, `alertdialog`, `alert`, `popover`), or under a
container whose own label contains a word like `cookie`, `consent`, `gdpr`,
`newsletter`, `subscribe`, `popup`, `modal`, or `overlay`. The word check is
skipped at the very root of the tree: a page whose window happens to be
titled "Newsletter" is the page, not something floating above it.

Front regions are always shown first and never muted by budget or noise
rules, on the reasoning that whatever a person would have to deal with
first (a cookie banner, a confirmation dialog) is exactly what a decision
model should see first too.

### What counts as noise

A region is `Noise` when every one of its members shares an ancestor whose
label contains a word like `ads`, `advert`, `sponsored`, `promoted`,
`footer`, `contentinfo`, or `copyright`, again skipping the very root.
Unlike a front region, noise is collapsed by default: shown as one line
naming the region, how many elements it holds, and a few examples, unless a
relevance score explicitly rescues it (see below).

```rust
// digest/test.rs builds a results page with a search toolbar, a cookie
// banner, 30 footer links, and a list of flight cards, and asserts:
digest(&screen).regions.iter().find(|r| r.name.contains("Cookie consent")).kind == RegionKind::Front;
digest(&screen).regions.iter().find(|r| r.name.contains("contentinfo")).kind == RegionKind::Noise;
```

### Lists as cards

A region whose members repeat under two or more same-role numbered
containers (`listitem #1`, `listitem #2`, …) gets a `list` field instead of
being rendered element by element. `render()` then shows it as one line per
card, its combined text and its "open this" control, instead of every
button and label inside every card. See
[lists and result cards](lists-and-result-cards.md) for how a card's text and
its primary control are chosen; the digest just decides that a region *is* a
list and delegates the actual card-building to that module.

## Rendering within a budget

`Digest::render(screen, rendering)` is what actually produces the JSON a
request sends to Jev. `Rendering` carries:

- `include_values`: whether field values may be shown at all (gated the same
  way everywhere in this crate: only when a caller explicitly asked to see
  them)
- `budget`: the most bytes the rendered elements may take
- `relevance`: an optional score per region id, 0 to 1, from a survey pass
  earlier in the flow; a region with no score counts as 0.5 (ordinary
  content) or 0 (noise)
- `distractions`: region ids a survey judged to be distraction this turn

Regions are spent against the budget in rank order: in front first, then by
relevance (highest first), then in the order they originally appeared. A
region that fits is rendered in full; one that does not, or one that is
muted outright as noise or a distraction with no rescuing relevance score,
is collapsed to a single summary line instead, such as:

```
r6 contentinfo: 30 elements, likely noise, e.g. link "Legal 0", link "Legal 1", …
```

A relevance score of 0.5 or higher rescues a noisy or distracting region from
being muted, so a survey pass that decides "actually the promo banner matters
this turn" can still surface it. Collapsed summaries themselves still count
against the budget, but only up to a fixed amount of slack past it (twice the
longest summary's length), so a page split into hundreds of tiny regions
cannot make the digest balloon without limit; past that point `render` just
reports "and N more regions not shown".

The result is wrapped once more as `untrusted_accessibility_data`, with
`in_front` and `regions` arrays and, when anything was collapsed, a
`collapsed` array of summary lines.

## Other things a digest gives you for free

- `region_of(index)`: which region one candidate belongs to.
- `front()`: an iterator over just the front regions.
- `layout()`: a string built from every region's kind and name, ignoring its
  contents. It changes when a dialog opens or the page moves to a different
  screen, and does not change when someone types into a field, so a
  higher-level attention pass that is keyed on "has the shape of the page
  changed" only re-runs when it actually should.
- `ranked(rendering)`: every element index in the order the digest would show
  them (front first, then by relevance, then reading order, noise and
  distractions last), useful for anything that wants the digest's ordering
  without needing the rendered text.

## Why this lives in `tinycomputer-core` and not the engine

The digest depends on nothing but `Screen` and `Candidate`. It does not know
what Jev is, does not call it, and does not know what a "survey" or a
"relevance score" means beyond "a number between 0 and 1 keyed by region id"
that some caller happens to pass in. That keeps it testable with plain data
and reusable by both surfaces, and keeps the actual decision of *when* to run
a relevance pass, and what to do with its answer, in the engine crate where
the rest of the Jev loop lives. See
[how tinycomputer decides](../../how-it-works.md) and
[`docs/technical/decision-thresholds.md`](../../technical/decision-thresholds.md)
for where the relevance and distraction thresholds this module reads are
documented and changed.
