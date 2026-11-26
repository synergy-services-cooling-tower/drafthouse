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
both "" cockpit
both "?ga" cockpit-ga
both "?refused" cockpit-refused
both "?light" cockpit-light
both "?poster" cockpit-poster
for s in inputs ranking calc; do shot "?sheet=$s" 390 "cockpit-$s-390.png"; done
shot "?refused&sheet=inputs" 390 cockpit-refused-inputs-390.png
