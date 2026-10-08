#!/bin/zsh
# Double-click to open TravelSnapMap (builds it first if needed). Lives in scripts/; works from the project root.
cd "$(dirname "$0")/.."
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"

APP="src-tauri/target/release/bundle/macos/TravelSnapMap.app"
BIN="$APP/Contents/MacOS/travelsnapmap"

needs_build=0
if [ ! -x "$BIN" ]; then
  needs_build=1
elif [ -n "$(find src src-tauri/src native index.html package.json src-tauri/tauri.conf.json -newer "$BIN" -type f 2>/dev/null | head -1)" ]; then
  needs_build=1
fi

if [ $needs_build = 1 ]; then
  echo "Building TravelSnapMap (first time takes a few minutes)…"
  [ -d node_modules ] || npm install || { echo "npm install failed"; read -k1 "?Press any key to close"; exit 1; }
  npm run tauri build || { echo "Build failed (see above)"; read -k1 "?Press any key to close"; exit 1; }
fi

open "$APP"
echo "TravelSnapMap is open. You can close this window."
