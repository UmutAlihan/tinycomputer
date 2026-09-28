# Browser sight: reading a page the way a person looks at it

**Status:** Implemented; the browser surface's default (`Perception::Sight`),
with the accessibility tree as its fallback and as an opt-out
(`Perception::Tree`, `TINYCOMPUTER_BROWSER_PERCEPTION=tree` in the live
examples).
**Code:** `crates/tinycomputer-browser/src/surface/sight/`.
**Evidence:** [`../evals/2026-09-28-jev-call-audit.md`](../evals/2026-09-28-jev-call-audit.md).

## Problem

The browser surface built its screen from agent-browser's accessibility
snapshot, so it saw what a page *declares*: roles and names from ARIA and
label markup. Most sites apply that markup partly or wrongly, and every gap
reached Jev as an unnamed or mislabelled element:

- IndiGo's destination list is nine `div role="combobox"` rows whose
  `aria-labelledby` points nowhere: nine unnamed "text boxes" that take no
  text, which a flow tried to type into and then pressed, choosing Mumbai;
- a field labelled only by the words printed above it has no name;
- an icon button with no `aria-label` has no name;
- the tree lists what is off screen or covered by a dialog as if it could be
  pressed, and does not say which layer is in front.

A person never reads the markup. They see what is drawn and on top, read the
words on a control or beside a box, and know a box takes text because it has
a caret.

## Goals

- Build the browser's `Screen` from the rendered page, by what is drawn,
  where, and on top — independent of ARIA.
- Name each control by the words a person would read for it.
- Decide what takes text by what the element is, not the role it claims.
- Keep the `Screen` model, the flow runtime, and the contract unchanged:
  sight is a new way to fill the same structure.

## Non-goals

- Vision: nothing reads pixels. An icon with no words, no alternative text,
  and no telling class stays unnamed.
- Shadow roots and frames, which a CSS selector from the page cannot reach:
  the tree is read instead (below).
- Changing agent-browser. Sight runs through its existing `evaluate` command,
  and acts through its existing CSS-selector targets.

## Behavior

`BrowserSurface::observe` runs `sight.js` over the page (or the part under a
`root` ref) in one `evaluate`, and turns the reply into a `Screen`:

1. **Controls, by behaviour.** Native links, buttons, fields, checkboxes,
   radios, and selects; elements with a control's ARIA role; and elements a
   pointer cursor, a click handler, or a tab stop makes clickable (not the
   children that only inherit the cursor). A claimed `textbox`, `searchbox`,
   `combobox`, or `spinbutton` that takes no text is a `button`, or, when it
   wraps a real input, is read as that input. A label that stands in for a
   hidden checkbox or radio is that checkbox or radio. Disabled controls are
   left out, as in the tree.
2. **Only what is drawn.** Zero-size, `display: none`, invisible, and
   transparent elements are left out, and so are disabled controls: the
   `disabled` property, `aria-disabled`, or a class name ending in
   `disabled` (a calendar's past day, `rdrDay rdrDayDisabled`). A control outside the viewport carries
   the state `offscreen`; one whose middle is under another element carries
   `covered`, unless the cover is its own result card's content (the same
   rule the click-through uses).
3. **Names are the words a person reads.** A control's own words (without
   those of a list of controls nested in it); for a field, its tied
   `<label>`, then the page's `aria-label`, then the words inside its box,
   left of it on its line, or just above it (right of it for a checkbox),
   then its placeholder; for a word-less control, its `aria-label`, `title`,
   or pictures' alternative text, then the icon's class, id, or test-id words
   (`close`, `search`, `menu`, …) with the description "an icon", and for a
   link, where it leads ("leads to sightseeing"). A page label that adds to
   the words shown becomes the description — the control's own, or else the
   one labelled element inside it that carries its words: a calendar day
   drawn as "18" whose inner span is labelled "Sunday, 18 October 2026".
4. **One control per thing a person sees.** Two related elements drawn in
   nearly the same box (intersection over union 0.6), a wrapper with the same
   words as the control inside it, or two controls of one kind in the same
   label, are one control; a text entry wins over its wrapper.
5. **Containers.** Each element's path lists what a person sees it in:
   dialogs (`dialog`, `alertdialog`, `aria-modal`) and fixed layers (a
   `dialog` when it covers 30% of the viewport, else a `popover`; not a top
   bar with links), landmarks (`banner`, `navigation`, `main`, `form`, …),
   named sections and groups, lists (an unnamed one after the first of its
   kind numbered in page order, `list 2`, so two lists' first cards stay
   apart), and cards (`listitem #3`, `row #2`, `article #1`), in the tree's
   label format so card grouping and the digest
   work unchanged. The screen's surface is `sheet` (or `alert`) when a dialog
   is on top at the middle of the viewport.
6. **Text.** Visible words outside controls and fields, within a screen of
   the viewport, become text nodes in page order; the first 60 distinct lines
   become the context. Words inside a field are its value, private unless
   values are shared, and never context. A password's value is never read.
7. **Refs.** Each control is marked `data-tc-seen="N"` the first time it is
   seen and keeps the mark for its life. Its ref is `seen:N`; the surface
   addresses it with the selector `[data-tc-seen="N"]` for every action,
   bounding box, value read, and scoped observation. An element the page
   removes takes its mark with it, so a stale ref fails rather than reaching
   what replaced it.
8. **Fallback.** When the reading fails, or sees a control inside a shadow
   root or a frame of a fifth of the viewport in front, the surface reads the
   accessibility tree for that observation, as before.

A covered click on a `seen:` ref finds its target by its mark, not by its
name, so an unnamed card link is clicked through its own card too. A click
on a `seen:` tab, radio, or option that leaves it on screen unselected is pressed once more by
the element's own `click()`: a page can ignore a trusted click it has not yet
wired up (Emirates' trip tabs, freshly loaded), and selecting is idempotent.

## Limits

| Constant | Value | Why |
|---|---|---|
| `MAX_CONTROLS` | 800 | a long results page, in page order |
| `MAX_TEXTS` | 400 | text blocks within a screen of the viewport |
| `MAX_LABELS` | 3,000 | visible words weighed as a field's label |
| `MAX_NAME` / `MAX_TEXT` | 120 / 160 characters | as the tree's content names and context lines |
| nearby label | 200 px left, 40 px above, 40 px right of a checkbox | the distances at which a person still reads words as a field's label |

## Invariants

- Screen text stays data: everything sight returns reaches Jev only through
  the flow runtime's `untrusted_accessibility_data` wrapping and masking.
- Typing still passes the surface's editable check (`takes_text`); sight
  only stops offering `SetValue` for what cannot take it.
- A ref never silently moves to another element.
- Nothing is added to the page but the marks.

## Acceptance

- Unit tests: the reply's controls, text, context, states, actions, and
  bounds; the fallback when a reading fails or sees what it cannot reach;
  sight refs addressed by their marks in fill, focus, the covered click, and
  a scoped observation; `Perception::Tree` reading the tree alone.
- Live, read with a local headless Chrome: IndiGo's city rows named by their
  cities as buttons, its form's radios, date and passenger controls read
  once each, Google Flights' fields named "Where from? New Delhi DEL" and
  its result cards grouped, the travel fixture's fields named by their
  labels.
- Live runs: see the eval.
