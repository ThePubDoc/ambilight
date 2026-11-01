#!/bin/bash
# Renders the popover and settings pages with sample data (mock-tauri.js) in headless Brave or
# Chrome, light and dark, into docs/. Usage: tools/screenshots/capture.sh
set -euo pipefail
cd "$(dirname "$0")/../.."

BROWSER="${BROWSER:-}"
for candidate in "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser" \
                 "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"; do
    [ -z "$BROWSER" ] && [ -x "$candidate" ] && BROWSER="$candidate"
done
[ -z "$BROWSER" ] && { echo "Set BROWSER to a Chromium-based browser"; exit 1; }

WORK=$(mktemp -d)
trap 'kill $SERVER 2>/dev/null || true; rm -rf "$WORK"' EXIT
cp ui/* "$WORK/"
cp tools/screenshots/mock-tauri.js tools/screenshots/frame.css "$WORK/"
for page in popover settings; do
    sed -i '' 's#<link rel="stylesheet" href="style.css">#<link rel="stylesheet" href="style.css"><link rel="stylesheet" href="frame.css"><script src="mock-tauri.js"></script>#' "$WORK/$page.html"
done

PORT=8765
python3 -m http.server "$PORT" --directory "$WORK" >/dev/null 2>&1 &
disown
SERVER=$!
sleep 1

mkdir -p docs
shot() { # name url width height [dark]
    local scheme=1 # light
    [ "${5:-}" = dark ] && scheme=0
    local flags=(--headless=new --hide-scrollbars --force-device-scale-factor=2 --virtual-time-budget=3000
                 --default-background-color=00000000 --blink-settings=preferredColorScheme=$scheme
                 --window-size="$3,$4" --screenshot="docs/$1.png")
    "$BROWSER" "${flags[@]}" "http://localhost:$PORT/$2" 2>/dev/null
    echo "docs/$1.png"
}

shot popover-dark "popover.html?mode=screenSound" 330 286 dark
shot popover-light "popover.html?mode=screenSound" 330 286
shot popover-effect-dark "popover.html?mode=effect&effect=aurora" 330 236 dark
shot settings-dark "settings.html" 520 1200 dark
shot settings-light "settings.html" 520 1200

# Social preview card (upload under the repo's Settings → Social preview).
cp tools/screenshots/social.html icons/app-icon.png docs/popover-effect-dark.png "$WORK/"
"$BROWSER" --headless=new --hide-scrollbars --force-device-scale-factor=1 --virtual-time-budget=3000 \
    --window-size=1280,640 --screenshot=docs/social-preview.png "http://localhost:$PORT/social.html" 2>/dev/null
echo "docs/social-preview.png"
