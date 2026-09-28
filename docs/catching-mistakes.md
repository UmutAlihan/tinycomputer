# Catching mistakes

Any agent that clicks things will sometimes click the wrong thing. What
matters is noticing and putting it right before the mistake spreads.
tinycomputer checks its own work after every action, and it has several ways
to back out.

## After every action: what changed?

tinycomputer takes a fingerprint of the screen before and after each action
and compares them. The fingerprint leaves out the temporary ids that change
with every look, so it only notices real changes.

- **Nothing changed.** The control that was pressed is banned for the rest of
  the step, so it isn't pressed again. After three actions in a row that
  change nothing, the step fails with "the last three actions changed nothing
  on screen".
- **Something changed.** A short note goes into the history Jev sees, like
  "the window is now New Message; appeared: textfield To:". That's how Jev
  learns what its last choice did.
- **Waiting that changes nothing** is fine once or twice (the page may have
  finished loading). After two idle waits in a row, Jev isn't allowed to wait
  again that step.

## Did it help?

After an action, the next judgement also asks "did the last action move
toward the step?" and "how far along is the step now?". Two answers count as
the action making things worse:

- progress dropped by a quarter of the scale or more;
- Jev is confident the action didn't help.

Either one triggers an undo.

## Predicting what should happen

Before pressing something, tinycomputer predicts what the press should do,
from the kind of control alone:

| Control | Should |
|---|---|
| a dropdown, or anything collapsed | open |
| a tab, radio button, or option | become selected |
| a checkbox or switch | change state |
| a link | go somewhere |
| a Close, Cancel, or Done button | close something |

After the press, it checks the screen against the prediction. Only a clear
contradiction counts, like a checkbox that didn't change, or a press that left
the page when it should have opened a menu. That's a **miss**, and a miss
makes the next judgement ask whether the result was intended. If it wasn't,
tinycomputer treats it as a mistake.

## Undo that checks itself

Before an important press, tinycomputer saves a **checkpoint**: a snapshot of
what the screen showed and the page's web address. Each kind of action has
its own way back:

| What happened | How it's undone |
|---|---|
| something opened, or it scrolled | Escape |
| a toggle changed | press it again |
| the page navigated | go back, then load the saved address if needed |
| a `choose` typed text into the wrong field | retype each changed field's old text |
| sending, paying, deleting | can't be undone, so never done by accident |

The undo is checked too. Afterward, the screen has to match the checkpoint
closely, at the same address. If an undo that claims to restore things (go
back, press again) doesn't, the step stops instead of carrying on from an
unknown place.

The typed-text case comes from a real run. On one airline's form, a `choose`
step couldn't find the gender option. It fell back to typing "Female" to
filter a list, but the cursor was in the last-name field, and "Raina" became
"RainaFemale" on every run. Now a failed `choose` puts every field it changed
back the way it was.

## Backtracking: trying the runner-up

When deliberation can't fully settle which element to press, tinycomputer
keeps the runners-up. If the first pick turns out wrong and gets undone, the
next press tries the best runner-up that isn't banned, after one quick
confirmation. With `deep` deliberation a step can backtrack three times; with
`standard`, once.

About 40% of successful runs in published research on web agents needed at
least one backtrack. It's normal for the first guess on a messy page to be
wrong.

## Looking back at a choice

A press can succeed and still leave the wrong thing on screen. On one site,
"choose 1 Adult" pressed a button whose label mentioned "1 Adult" but
actually added a passenger. The search went ahead for two people.

So after a `choose` step presses something, tinycomputer looks again and asks
two questions: "does the screen show the choice exactly as asked?" and "does
the screen show a different choice, or a change nobody asked for?". Sometimes
it doesn't need to ask. If the requested tab isn't selected but another tab
is, the screen already answers the question.

If the choice didn't take, it gets one repair attempt: a short set of turns
told to "correct the previous step so the screen shows ...; undo anything it
changed that was not asked for". Then it checks again. If it still isn't
right, the step fails and says why.

This is called **reflection**.

## Typing that checks itself

Every value typed into a field is read back:

1. set the field's value directly;
2. read it back; if it matches, done;
3. if not, wait a moment and read again;
4. still wrong: paste it instead, and read back once more;
5. still wrong: report `TEXT_NOT_DELIVERED`.

After an `enter` step, tinycomputer also asks whether the form shows an error
next to each field it filled. Flagged fields are filled once more. If the form
still complains, the step fails and names the fields.

## When all of this isn't enough

Sometimes a step fails anyway: the page is laid out in a way the plan didn't
expect, or an earlier step quietly didn't take. In a task, that failure goes
to the rescuer before anything is reported. See [Rescues](rescue.md).

## Where to find out more

- [`technical/specs/jev-deliberation.md`](technical/specs/jev-deliberation.md):
  expectations, checkpoints, undo, and backtracking in full.
- [`technical/specs/flow-reflection.md`](technical/specs/flow-reflection.md):
  reflection.
- [`technical/decision-loops.md`](technical/decision-loops.md): the `do` loop
  and text delivery.
