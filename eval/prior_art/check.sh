#!/bin/bash
# Wraps `backtest-audit check . --format json` with an explicit per-file
# confirmation line -- the raw JSON's empty `[]` per file requires
# explaining out loud otherwise ("empty array means no issues"). This just
# reformats the same real tool output, it doesn't change what was checked.
# Requires the venv from REPRODUCTION.md's prior-art step to be active
# (`source venv/bin/activate` from the repo root, or wherever it was
# created).
set -euo pipefail
cd "$(dirname "$0")"

backtest-audit check . --format json | jq -r '
  to_entries[] |
  "\(.key): \(if (.value | length) == 0 then "✓ checked, no issues found" else "✗ \(.value | length) issue(s) found" end)"
'
