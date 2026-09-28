# Writing flows

A flow is a short script of plain-language steps. It says what to get done,
never how. You write "start a new email message", and tinycomputer finds the
button, the menu item, or the keyboard shortcut on the live screen.

Tasks write flows for you when a planner is configured. You'll want to write
one yourself when you have no planner, when you want to reuse the same steps
every time, or when you want exact control over where it stops.

## A first flow

```json
{
  "app": "Mail",
  "vars": { "to": "sam@example.com" },
  "steps": [
    { "open": "Mail" },
    "start a new email message",
    { "enter": {
        "recipient": "${to}",
        "subject": "Moving Thursday's sync",
        "message body": "Hi Sam,\n\nCould we move Thursday's sync to Friday at 3pm?\n\nThanks,\nAlex"
    } },
    { "verify": "the draft shows the recipient, the subject, and the message body" },
    { "stop_before": "sending the email" }
  ]
}
```

Read it from top to bottom the way you'd brief a colleague who has never used
Mail:

1. Open Mail.
2. Start a new message.
3. Put the address in the recipient field, the subject in the subject field,
   and the text in the body.
4. Check that the draft shows all three.
5. Find the Send button, but don't press it.

Nowhere does it say "click the pencil icon" or "press Cmd+N". That's the
point. The same flow keeps working when Mail moves a button.

- `app` is the application's name as the operating system shows it, or
  `browser` for a flow that starts on the web.
- `vars` are named values. Any step can use them as `${name}`.
- `steps` run in order. A step is either a plain sentence or an object with
  one key.

## The step kinds

| Step | Example | What it does |
|---|---|---|
| a sentence, or `do` | `"open the Liked Songs list"` | reach the described state |
| `open` | `{"open": "Mail"}` | launch an app or bring it to the front |
| `browse` | `{"browse": "https://www.google.com/travel/flights"}` | open a web address; later steps act on that page |
| `enter` | `{"enter": {"subject": "Hi"}}` | type each value into the field its name describes |
| `choose` | `{"choose": {"what": "the font list", "option": "Helvetica"}}` | pick an option from a list, menu, tab set, or calendar |
| `read` | `{"read": {"what": "the newest message's subject", "into": "subject"}}` | save visible text in a variable |
| `extract` | `{"extract": {"what": "the flight results", "into": "flights"}}` | save every row of a list, as data |
| `pick` | `{"pick": {"from": "the flight results", "by": "lowest price", "into": "flight"}}` | choose the best item of a list and open it |
| `verify` | `{"verify": "the draft shows a recipient"}` | fail unless this is true on screen |
| `wait_for` | `{"wait_for": "the search results are showing"}` | wait until this is true |
| `stop_before` | `{"stop_before": "sending the email"}` | find something irreversible and stop in front of it |
| `if` | see below | do one thing or another depending on the screen |
| `repeat_until` | see below | repeat steps until something is true |

### Branching and repeating

```json
{ "if": {
    "condition": "a folder named tinycomputer-lab is visible",
    "then": [ "open the tinycomputer-lab folder" ],
    "else": [ "create a new folder", { "enter": { "folder name": "tinycomputer-lab" } } ]
} }
```

```json
{ "repeat_until": {
    "condition": "the display shows 4736",
    "steps": [ "calculate 128 times 37" ],
    "max": 2
} }
```

### Picking from a list

`pick` is how you say "the cheapest" or "the earliest". When the rule is
something tinycomputer can measure (lowest or highest price, earliest or
latest time, fewest stops, shortest duration) it reads the numbers off each
result and compares them exactly. It doesn't ask a model at all. For a
fuzzier rule, like "a morning flight with at most one stop", it asks Jev to
pick.

## Writing good steps

**Describe outcomes, not clicks.** Write "start a new email message", not
"click the compose button".

**One idea per step.** "Open the Sent mailbox" and "open the newest message"
are two steps. A step that tries to do three things is hard to judge as done.

**Name fields by what they hold.** In `enter`, use `recipient`, `subject`,
`message body`. Say what the text is, not where you think it goes.

**Put every word in the flow.** tinycomputer never writes prose. Anything that
should appear on screen belongs in an `enter` value.

**Finish with a check.** End anything that matters with `verify`.

**Guard anything you can't take back.** Put a `stop_before` in front of
sending, deleting, buying, and submitting. Whether it may actually press that
control is decided separately. See [Safety and privacy](safety-and-privacy.md).

**Don't guess the interface.** If you aren't sure a panel is open, say what
you need ("show the formatting options") and let tinycomputer find it.

**Use the label the page shows in `choose`.** Write `"option": "Saver"`, not
`"the cheapest fare"`. Choosing by a rule is what `pick` is for.

**Checks must be visible now.** A `verify` has to be checkable on the current
screen alone. "Matches what the other site showed" can't be checked.

## Your details in a flow

Details like your name or email are called facts. They come in two kinds.

- **Shared** facts, like a name or date of birth, can appear anywhere in a
  flow. "choose ${title} in the title field" is fine, and it helps
  tinycomputer pick the right option.
- **Secret** facts, like a passport number, a password, or a card number, may
  only appear as a value in an `enter` step. Anywhere else, the flow is
  rejected before it runs. Secrets are typed into fields and never shown to
  any model.

More in [Safety and privacy](safety-and-privacy.md).

## Checking a flow before running it

- `ValidateFlow` checks a flow without touching anything. It catches unknown
  step kinds, secrets in the wrong place, variables nobody sets, and similar
  mistakes.
- `FlowGuide` returns the full authoring guide as text, ready to hand to a
  language model that writes flows.
- `RunFlow` runs a flow directly on the desktop. Tasks run flows too, and add
  pausing, approvals, rescues, and the browser.

## Where to find out more

- [`crates/tinycomputer-bus/src/flow/guide.md`](../crates/tinycomputer-bus/src/flow/guide.md):
  the authoring guide, as given to language models.
- [`technical/decision-loops.md`](technical/decision-loops.md): exactly how each
  step kind runs.
- [`technical/flow-examples.md`](technical/flow-examples.md): real flows traced
  decision by decision.
