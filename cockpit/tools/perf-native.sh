#!/usr/bin/env bash
# perf-native.sh - the issue #82 measurement pass, on this host's own GPU.
#
#   cockpit/tools/perf-native.sh before <out.json>     # -> <out.json> (+ its .log beside it)
#                                                      #    the caller names where the report lands:
#                                                      #    the lane archive is the lane's business
#
# Builds the native binary with the release profile - the same profile the wasm payload ships with
# (`cockpit/Cargo.toml`, `[profile.release]`) - and runs the instrument under `DRAFTHOUSE_PERF` (the
# harness in `cockpit/src/bootstrap.rs`). The plan: warm up, idle with nothing moving the draft, a
# 6 s rpm drag, a settle window, and a held pose. The JSON report carries p50/p95 frame time, engine
# runs per second per phase, the frame count and the post-release exactness check; the app exits by
# itself. Both streams go to the log; the report is at the path below (and on stdout).
#
# The same command on the same machine is what makes the before/after pair comparable: run it once
# on the tree before the fix and once after, with the label naming which is which.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
LABEL="${1:?usage: perf-native.sh <label> <out-json>}"
OUT="${2:?usage: perf-native.sh <label> <out-json>}"
LOG="${PERF_LOG:-$OUT.log}"
mkdir -p "$(dirname "$OUT")" "$(dirname "$LOG")"

cd "$ROOT/cockpit"
PROFILE="${CARGO_PROFILE_RELEASE_OPT_LEVEL:-$(sed -n 's/^opt-level *= *//p' Cargo.toml | head -1 | tr -d '"')}"
echo "== building the release binary (profile.release opt-level=${PROFILE}) =="
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}" cargo build --bin drafthouse --release

echo "== measurement pass (label=$LABEL) -> $OUT =="
echo "== full log: $LOG =="
DRAFTHOUSE_PERF="out=$OUT;label=$LABEL" "./target/release/drafthouse" 2>&1 | tee "$LOG"
