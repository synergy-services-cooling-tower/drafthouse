#!/usr/bin/env bash
# serve.sh - static server for the cockpit. Serves this directory: index.html, pkg/, assets/.
#
#   cockpit/tools/serve.sh            # port 8177 (or the next free one up to 8199)
#   cockpit/tools/serve.sh 8188
#
# The lane's own server (the repository's product dev server is `npm start` / scripts/serve.mjs,
# which serves the same directory). Chrome-headless capture (`tools/capture.mjs`) expects this one.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WEBROOT="$(cd "$HERE/.." && pwd)"
PORT="${1:-8177}"

[ -f "$WEBROOT/index.html" ] || { echo "index.html missing in $WEBROOT" >&2; exit 1; }
[ -f "$WEBROOT/pkg/drafthouse_cockpit_bg.wasm" ] || echo "warning: pkg/drafthouse_cockpit_bg.wasm missing - run tools/build-web.sh first" >&2

while [ "$PORT" -lt 8200 ]; do
  if ! lsof -nP -iTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1; then break; fi
  echo "port $PORT busy, trying $((PORT + 1))" >&2
  PORT=$((PORT + 1))
done

echo "serving $WEBROOT"
echo "  http://127.0.0.1:$PORT/index.html"
echo "  http://127.0.0.1:$PORT/index.html?host=internal&frozen=1"
echo "PORT=$PORT"
cd "$WEBROOT"
exec python3 -m http.server "$PORT" --bind 127.0.0.1
