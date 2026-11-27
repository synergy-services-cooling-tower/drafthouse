#!/bin/sh
# Renders each cockpit state at 1440x900 and 390x844 with Playwright's cached headless Chromium (WebGL by SwiftShader).
set -eu
cd "$(dirname "$0")"
shell=$(ls -d "$HOME"/Library/Caches/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-*/chrome-headless-shell | tail -1)
shot() {
  w=$2; h=900; [ "$w" = 390 ] && h=844
  "$shell" --headless --allow-file-access-from-files --force-prefers-reduced-motion --use-angle=swiftshader --enable-unsafe-swiftshader \
    --force-device-scale-factor=1 --virtual-time-budget=6000 --hide-scrollbars --window-size="$w,$h" \
    --screenshot="$PWD/$3" "file://$PWD/cockpit.html$1" >/dev/null 2>&1
  echo "$3 ${w}x$h"
}
both() { shot "$1" 1440 "$2-1440.png"; shot "$1" 390 "$2-390.png"; }
only=${1:-}
run() { [ -z "$only" ] || [ "$only" = "$1" ]; }
run base && { both "" cockpit; for s in inputs ranking calc; do shot "?sheet=$s" 390 "cockpit-$s-390.png"; done; }
run ga && both "?ga" cockpit-ga
run refused && { both "?refused" cockpit-refused; shot "?refused&sheet=inputs" 390 cockpit-refused-inputs-390.png; }
run light && both "?light" cockpit-light
run poster && both "?poster" cockpit-poster
run layers && { shot "?s=layers" 1440 cockpit-layers-1440.png; shot "?s=layers&sheet=build" 390 cockpit-layers-390.png; }
run cells && { both "?s=cells" cockpit-cells; both "?s=cells&ga" cockpit-cells-ga; }
run structure && { shot "?s=structure" 1440 cockpit-structure-1440.png; shot "?s=structure&sheet=calc" 390 cockpit-structure-390.png; }
run parts && { shot "?s=parts" 1440 cockpit-parts-1440.png; shot "?s=parts&sheet=parts" 390 cockpit-parts-390.png; }
run compare && { shot "?s=compare" 1440 cockpit-compare-1440.png; shot "?s=compare&sheet=compare" 390 cockpit-compare-390.png; }
exit 0
