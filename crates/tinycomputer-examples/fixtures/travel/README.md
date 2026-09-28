# Travel fixture

A small static booking site for end-to-end browser runs in the Docker lab
(`docs/technical/docker-lab.md`). It has the shape of a real booking flow with none of a
real site's unpredictability, so a run's outcome can be checked exactly:

| Page | What it exercises |
|---|---|
| `index.html` | a cookie-consent dialog in front of the page; a search form |
| `results.html` | a list of result cards with prices, times, durations, and stops; `pick` by lowest price chooses IndiGo 6E-2135 at ₹6,840 |
| `traveller.html` | a traveller form with `autocomplete` hints, and a paid insurance checkbox to leave alone |
| `extras.html` | a paid upsell and a way to skip it |
| `payment.html` | card fields (`cc-*`) and a pay button: the task must stop here |

Serve it with `python3 -m http.server 8765` from this directory inside the
lab and browse to `http://127.0.0.1:8765/`. Nothing on it charges anything:
the pay button only marks the page.
