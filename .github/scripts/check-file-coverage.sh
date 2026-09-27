#!/usr/bin/env bash
set -euo pipefail

minimum="${1:-90}"
report="${2:-coverage.json}"
workspace_root="$(pwd -P)/"
# Every crate lives under `crates/<package>/src/`, so one prefix covers the
# whole workspace. Vendored submodules and `worktrees/` sit outside it and are
# excluded by the same test.
source_root="${workspace_root}crates/"
# macOS deliberately does not exercise `clipboard-clear`: doing so would erase
# the developer's real pasteboard. Linux CI has no pasteboard and covers that
# branch, so omit only that file from a local Darwin report.
coverage_exclude=""
if [[ "$(uname -s)" == "Darwin" ]]; then
  coverage_exclude="${source_root}tinycomputer-desktop/src/desktop/clipboard.rs"
fi

cargo llvm-cov \
  --locked \
  --workspace \
  --exclude tinycomputer-examples \
  --all-targets \
  --all-features \
  --json \
  --output-path "$report"

summary="$(jq -r --arg workspace_root "$workspace_root" --arg source_root "$source_root" --arg coverage_exclude "$coverage_exclude" '
  .data[].files[]
  | select(.filename | startswith($source_root))
  | select(.filename != $coverage_exclude)
  # LLVM may emit the same source line from more than one monomorphization.
  # Normalize its duplicated line denominator to unique source locations while
  # retaining the LLVM covered-line count, capped at that normalized total.
  | ([.segments[] | select(.[3] == true) | .[0]] | unique | length) as $count
  | select($count > 0)
  | ([.summary.lines.covered, $count] | min) as $covered
  | [
      (.filename | ltrimstr($workspace_root)),
      (($covered * 100 / $count) | tostring),
      ($covered | tostring),
      ($count | tostring)
    ]
  | @tsv
' "$report")"

covered_files="$(printf '%s\n' "$summary" | awk 'NF { count += 1 } END { print count + 0 }')"
if [[ "$covered_files" -eq 0 ]]; then
  echo "coverage report contains no files with executable lines under crates/" >&2
  exit 1
fi

printf 'File\tLine coverage\tCovered lines\tCoverable lines\n'
while IFS=$'\t' read -r file percent covered count; do
  printf '%s\t%.2f%%\t%s\t%s\n' "$file" "$percent" "$covered" "$count"
done <<< "$summary"

if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  {
    printf '### Per-file line coverage\n\n'
    printf '| File | Coverage | Lines |\n'
    printf '| --- | ---: | ---: |\n'
    while IFS=$'\t' read -r file percent covered count; do
      printf '| %s | %.2f%% | %s/%s |\n' "$file" "$percent" "$covered" "$count"
    done <<< "$summary"
  } >> "$GITHUB_STEP_SUMMARY"
fi

failures="$(awk -F $'\t' -v minimum="$minimum" '$2 + 0 < minimum { printf "%s: %s%%\n", $1, $2 }' <<< "$summary")"

if [[ -n "$failures" ]]; then
  printf '\nFiles below %s%% line coverage:\n%s\n' "$minimum" "$failures" >&2
  exit 1
fi
