#!/bin/sh
# Renders each page at 1440 and 390 wide, full height, light, reduced motion, with Playwright's cached headless Chromium (WebGL by SwiftShader).
set -eu
cd "$(dirname "$0")"
shell=$(ls -d "$HOME"/Library/Caches/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-*/chrome-headless-shell | tail -1)
shot() {
  set -- "$1" "$2" "$3" --headless --allow-file-access-from-files --force-prefers-reduced-motion --blink-settings=preferredColorScheme=1 \
    --use-angle=swiftshader --enable-unsafe-swiftshader --force-device-scale-factor=1 --virtual-time-budget=4000
  page=$1 out=$2 w=$3; shift 3
  h=$("$shell" "$@" --window-size="$w",900 --dump-dom "file://$PWD/$page" 2>/dev/null | sed -n 's/.*data-h="\([0-9]*\)".*/\1/p' | head -1)
  "$shell" "$@" --hide-scrollbars --window-size="$w,${h:-2000}" --screenshot="$PWD/$out" "file://$PWD/$page" >/dev/null 2>&1
  echo "$out ${w}x${h:-2000}"
}
pages=${*:-landing wizard tower}
for p in $pages; do
  for w in 1440 390; do shot "$p.html" "$p-$w.png" "$w"; done
done
[ -z "$*" ] && shot "landing.html?poster" landing-poster-390.png 390
true
