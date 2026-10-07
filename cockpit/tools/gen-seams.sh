#!/usr/bin/env bash
# gen-seams.sh - regenerate docs/COCKPIT_SEAMS.md from the seam registry (imported, issue #58).
#
#   cockpit/tools/gen-seams.sh          # write the file
#   cockpit/tools/gen-seams.sh --check  # fail if the file is stale
#
# The document is a pure function of `cockpit/seams/src/lib.rs` (the `gen-seams` binary in that
# crate), so a diff here means the code changed - that is the point: the document and the running
# app's "Data seams" panel cannot drift from the bindings.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="$ROOT/docs/COCKPIT_SEAMS.md"

cd "$ROOT/cockpit/seams"
cargo run --quiet --bin gen-seams > /tmp/cockpit-seams.md
if [ "${1:-}" = "--check" ]; then
  if ! diff -u "$OUT" /tmp/cockpit-seams.md; then
    echo "docs/COCKPIT_SEAMS.md is stale - run cockpit/tools/gen-seams.sh" >&2
    exit 1
  fi
  echo "docs/COCKPIT_SEAMS.md is current"
else
  cp /tmp/cockpit-seams.md "$OUT"
  echo "wrote $OUT ($(wc -l < "$OUT" | tr -d ' ') lines)"
fi
