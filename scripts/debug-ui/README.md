# Jev Journal Inspector

A local Vite SPA for exploring the JSONL files produced by the Jev debug
journal. In development, its Vite server discovers run folders in the primary
checkout's `.jev-journal`, lists all runs, and loads the most recently modified
run at startup. Other runs load when selected. The API is read-only and only
serves `journal.jsonl` files below that directory. A built static preview has
no journal API, but still supports choosing files in the browser.

Journal files may contain screen text and other personal data. The dev server
reads them from the local checkout and serves them to the preview; it does not
upload them to a cloud service. See
[`docs/jev-journal.md`](../../../docs/jev-journal.md) for the event format and
journal setup.

From the repository root:

```sh
cd scripts/debug-ui
npm install
npm run dev
```

Open the local URL printed by Vite. Runs in `.jev-journal` load automatically.
You can also choose a different journal folder, choose individual
`journal.jsonl` files, or drop files into the page. The folder picker includes
nested run folders. Use **Add JSONL files** to add files to the current set.

The inspector shows a run list, searchable/filterable event timeline, request
and answer tabs for Jev exchanges, expandable JSON trees, summary timing, and
per-run/per-call input and output token counts. Malformed lines are skipped and
reported while valid events remain available. Journal changes do not trigger
Vite reloads; click the run-list refresh button to discover new runs. Refreshing
the currently selected run also picks up appended events while keeping the
selected event and filters in place. The run list, timeline, and event details
scroll independently; JSON trees have expand-all and collapse-all controls.

```sh
npm run build
npm run preview
```
