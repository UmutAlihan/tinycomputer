# Writing a desktop flow

A flow is a short JSON script that says **what** to accomplish in one desktop
application. It never says **how**: no button names, no menu paths, no keyboard
shortcuts, no coordinates. The desktop module works out the how on the live
screen, one small decision at a time.

Write a flow the way you would brief a capable person who has never used the
app: a handful of plain steps, each describing a state to reach or a thing to
do.

## Shape

```json
{
  "app": "TextEdit",
  "vars": { "greeting": "Hello from a flow." },
  "steps": [
    { "open": "TextEdit" },
    "start a new blank document",
    { "enter": { "document text": "${greeting}" } },
    { "verify": "the document shows the greeting text" }
  ]
}
```

- `app` is the application's name as the operating system shows it, or
  `browser` for a flow that starts on the web.
- `vars` are optional named values; any step text may use `${name}`.
- `steps` run in order. A step is a plain string or an object with one key.

## Steps

| Step | Example | Meaning |
|---|---|---|
| string | `"open the Liked Songs list"` | Reach the described state. |
| `open` | `{"open": "Mail"}` | Launch the app or bring it forward. |
| `browse` | `{"browse": "https://www.google.com/travel/flights"}` | Open a web address in the browser; later steps act on the page until an `open` switches back to an app. |
| `do` | `{"do": "start a new note"}` | Same as a plain string. |
| `enter` | `{"enter": {"subject": "Hi"}}` | Put each text into the field its key describes. |
| `choose` | `{"choose": {"what": "the font list", "option": "Helvetica"}}` | Pick an option in a list, menu, or popup. |
| `read` | `{"read": {"what": "the newest message's subject", "into": "subject"}}` | Store visible text in a variable. |
| `extract` | `{"extract": {"what": "the flight results", "into": "flights"}}` | Store every item of a list, as JSON rows of their text, in a variable. |
| `pick` | `{"pick": {"from": "the flight results", "by": "lowest price", "into": "flight"}}` | Choose the best of a list of results and open it; `into` stores its text. Prices, times, durations, and stops are compared exactly. |
| `verify` | `{"verify": "the draft shows a recipient"}` | Fail the flow unless this holds. |
| `wait_for` | `{"wait_for": "the search results are showing"}` | Wait until this holds. |
| `stop_before` | `{"stop_before": "sending the email"}` | Find an irreversible action and stop in front of it. |
| `repeat_until` | see below | Repeat steps until a condition holds. |
| `if` | see below | Branch on a condition. |

```json
{
  "app": "Finder",
  "steps": [
    { "open": "Finder" },
    "show the Desktop folder",
    { "if": {
        "condition": "a folder named tinydesktop-lab is visible",
        "then": [ "open the tinydesktop-lab folder" ],
        "else": [ "create a new folder",
                  { "enter": { "folder name": "tinydesktop-lab" } } ]
    } }
  ]
}
```

```json
{
  "app": "Calculator",
  "steps": [
    { "open": "Calculator" },
    "clear the calculator",
    { "repeat_until": {
        "condition": "the display shows 4736",
        "steps": [ "calculate 128 times 37" ],
        "max": 2
    } }
  ]
}
```

## Writing good steps

1. **Describe outcomes, not clicks.** Write "start a new email message", not
   "click the compose button". The module finds the button, the menu item, or
   the shortcut.
2. **One idea per step.** "Open the Sent mailbox" and "open the newest message"
   are two steps.
3. **Name fields by purpose in `enter`.** Use `recipient`, `subject`,
   `message body`: what the text *is*, not where you guess it goes.
4. **Put all text in the flow.** The module chooses and acts; it never writes
   prose. Every word that should end up on screen belongs in an `enter` value.
5. **End with `verify`** for anything that matters, and **guard irreversible
   actions with `stop_before`** (sending, deleting, buying, submitting). The
   caller decides separately whether those may run.
6. **Do not guess the interface.** If you are unsure whether a panel is open,
   say what you need ("show the formatting options"); do not script how to get
   there.

## A full example

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

## Across the web and an application

A flow may move between surfaces: `browse` switches to the browser, `open`
switches back to an application. Values captured with `read` on one surface
can be typed on the other.

```json
{
  "app": "browser",
  "vars": { "from": "Delhi", "to": "Srinagar", "date": "14 October" },
  "steps": [
    { "browse": "https://www.google.com/travel/flights" },
    { "enter": { "where from": "${from}", "where to": "${to}", "departure date": "${date}" } },
    "search for flights",
    { "wait_for": "flight results are listed" },
    { "pick": { "from": "the flight results", "by": "lowest price", "into": "cheapest" } },
    { "open": "Mail" },
    "start a new email message",
    { "enter": { "subject": "Flight to ${to}", "message body": "Cheapest option: ${cheapest}" } },
    { "stop_before": "sending the email" }
  ]
}
```
