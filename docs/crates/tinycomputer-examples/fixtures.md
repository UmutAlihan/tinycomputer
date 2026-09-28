# Fixtures

Real websites are unpredictable: a layout changes, a fare shifts, a site
decides your browser looks automated and refuses to serve results. That
makes them a poor place to check whether the browser adapter and the flow
runtime are working correctly, because a failed run does not tell you
whether your code broke or the airline's website changed under you. The
travel fixture exists to give the browser examples something with the
shape of a real booking flow and none of that unpredictability, so a run's
outcome can be checked exactly, every time.

## The travel fixture

It lives at
[`crates/tinycomputer-examples/fixtures/travel/`](../../../crates/tinycomputer-examples/fixtures/travel/)
and is nothing more than a handful of static HTML pages:

| Page | What it exercises |
|---|---|
| `index.html` | a cookie-consent dialog sitting in front of the page, and a search form behind it |
| `results.html` | a list of result cards with prices, times, durations, and stops: the cheapest is always IndiGo flight 6E-2135 at ₹6,840, so a checker can assert on that exact card |
| `traveller.html` | a traveller form with `autocomplete` hints, plus a paid insurance checkbox a correct run should leave alone |
| `extras.html` | a paid upsell, and a way to skip it |
| `payment.html` | card fields (named `cc-*`) and a pay button: the point every task in this crate must stop in front of, never past |

Nothing on it charges anything. The pay button on `payment.html` only marks
the page as "paid" in memory; there is no real payment processor anywhere
near it.

## Serving it

From inside the Docker lab (see [Docker lab](docker-lab.md) for why it
belongs there):

```sh
cd crates/tinycomputer-examples/fixtures/travel
python3 -m http.server 8765
```

Then point a browser at `http://127.0.0.1:8765/`. The two example wrapper
scripts do this for you already:

```sh
scripts/docker-lab -- crates/tinycomputer-examples/fixtures/run browser_fixture
scripts/docker-lab -- crates/tinycomputer-examples/fixtures/run task_fixture
```

`fixtures/run` starts the HTTP server in the background, builds and runs
the binary you named, and kills the server when the binary exits, whether
it succeeded or not.

## What each example checks against it

`browser_fixture` (see
[running the examples](running-the-examples.md#checking-the-browser-stack-without-spending-jev-credit-browser_fixture))
drives the fixture with no Jev involved at all: it opens each page directly
and asserts on the parsed screen: that the cookie sheet really is in
front, that the search form's fields are really named "From", "To", and
"Departure date", that there really are four result cards, that ranking by
lowest price really picks the ₹6,840 IndiGo flight, and that the payment
page is really detected as a payment page. This is the fast, cheap check
that the browser adapter's own logic (snapshot parsing, result grouping,
ranking, payment detection) still works, independent of anything Jev might
get wrong on top of it.

`task_fixture` (see [live tasks](live-tasks.md)) runs a full booking task
against the same fixture, but this time with live Jev making the small
decisions: search, pick the cheapest result, fill in the traveller form
(asking you, mid-run, for the one fact it was not given, the phone
number), skip the paid upsell, and stop before paying. Because the fixture
never changes, this is a meaningful evaluation harness in its own right:
if a change to the engine makes this task fail, that failure is really
about your change, not about the travel industry.

`TINYCOMPUTER_FIXTURE_URL` overrides the fixture's address for either
example, if you want to serve it from somewhere other than
`127.0.0.1:8765`.
