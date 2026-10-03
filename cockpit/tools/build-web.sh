#!/usr/bin/env bash
# build-web.sh - build the cockpit to wasm32-unknown-unknown (WebGL2) with wasm-pack, and gate its size.
#
#   cockpit/tools/build-web.sh                  # release: the shipped payload (wasm-opt -Oz)
#   cockpit/tools/build-web.sh fast             # the iteration profile (no LTO, no wasm-opt)
#   cockpit/tools/build-web.sh release three-d  # the round-3 3D view (brings bevy_pbr back)
#   cockpit/tools/build-web.sh release --no-default-features --features fixture-engine
#
# Output: cockpit/pkg/{drafthouse_cockpit.js, drafthouse_cockpit_bg.wasm}. `cockpit/` is the web root - index.html,
# pkg/ and assets/ - so serving that one directory is the whole deployment (the asset server fetches
# assets/fixture.json from it).
#
# Issue #58, the gzip gate: the shipped payload must stay under 8 MB gzipped. The limit is the
# parameterised knob (`COCKPIT_GZIP_LIMIT_BYTES`, default 8 MiB) that the lane's RED/GREEN pair moves;
# the gate is stated for the release build, which is the one the CI job `cockpit` builds and the
# release workflow publishes. `fast` and `dev` builds print their size without gating (they are not
# the shipped bytes).
#
# Issue #82, the size budget: the same release gate also enforces the committed budget
# (`cockpit/wasm-budget.json`: raw and gzipped, the size the payload had when the budget was set),
# through `cockpit/tools/wasm-budget.mjs` - the one place the comparison lives. `COCKPIT_WASM_BUDGET`
# points the check at another budget file (the RED/GREEN knob); the CI jobs build the release payload
# with this script, so an over-budget payload fails them here.
#
# `COCKPIT_WASM_SKIP_BUILD=1` gates the `pkg/` that is already there instead of rebuilding - the
# RED/GREEN pair reruns the gate in a second, and a job that just built the payload can re-check it.
# CI never sets it, so CI always gates the bytes it just built.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
CRATE="$ROOT"
# A member of the engine workspace at `rust/` would otherwise share its target dir; the cockpit's
# build is its own cache (gitignored), so the engine's recorded artifact builds stay untouched.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
LIMIT="${COCKPIT_GZIP_LIMIT_BYTES:-8388608}"
SKIP_BUILD="${COCKPIT_WASM_SKIP_BUILD:-0}"

MODE="${1:-release}"
if [ "$#" -gt 0 ]; then shift; fi
CARGO_ARGS=("$@")

case "$MODE" in
  release) PROFILE_FLAG=(--release); GATE=1 ;;
  fast)    PROFILE_FLAG=(--profile fast); GATE=0 ;;
  dev)     PROFILE_FLAG=(--dev); GATE=0 ;;
  *) echo "usage: build-web.sh [release|fast|dev] [extra cargo build args...]" >&2; exit 2 ;;
esac

command -v wasm-pack >/dev/null || { echo "wasm-pack missing: cargo install wasm-pack" >&2; exit 1; }
rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown \
  || { echo "wasm32-unknown-unknown missing: rustup target add wasm32-unknown-unknown" >&2; exit 1; }

cd "$CRATE"
if [ "$SKIP_BUILD" = "1" ]; then
  echo "== wasm-pack build skipped (COCKPIT_WASM_SKIP_BUILD=1): gating the existing pkg =="
else
  echo "== wasm-pack build ($MODE, CARGO_TARGET_DIR=$TARGET_DIR) =="
  CARGO_TARGET_DIR="$TARGET_DIR" wasm-pack build --target web --out-dir pkg --no-typescript \
    "${PROFILE_FLAG[@]}" ${CARGO_ARGS[@]+"${CARGO_ARGS[@]}"}
fi

WASM="$CRATE/pkg/drafthouse_cockpit_bg.wasm"
[ -f "$WASM" ] || { echo "no wasm at $WASM" >&2; exit 1; }

RAW=$(wc -c < "$WASM" | tr -d ' ')
GZ=$(gzip -9 -c "$WASM" | wc -c | tr -d ' ')
JS=$(wc -c < "$CRATE/pkg/drafthouse_cockpit.js" | tr -d ' ')
echo "== payload =="
echo "  drafthouse_cockpit_bg.wasm  raw ${RAW} bytes / gzipped ${GZ} bytes"
echo "  drafthouse_cockpit.js       ${JS} bytes"
echo "  pkg:                 $CRATE/pkg"

if [ "$GATE" = "1" ]; then
  echo "  gzip gate:           ${GZ} bytes gzipped vs the ${LIMIT}-byte limit"
  if [ "$GZ" -gt "$LIMIT" ]; then
    echo "GZIP SIZE GATE: FAILED - ${GZ} bytes gzipped is over the ${LIMIT}-byte limit" >&2
    exit 1
  fi
  echo "GZIP SIZE GATE: OK - ${GZ} bytes gzipped, limit ${LIMIT}"

  # Issue #82: the committed budget, raw and gzipped, measured from the bytes above.
  node "$HERE/wasm-budget.mjs" --raw "$RAW" --gzip "$GZ" --label "$MODE"
fi
