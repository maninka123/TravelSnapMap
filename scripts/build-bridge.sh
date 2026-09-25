#!/bin/sh
# Builds the Swift photos-bridge sidecar with the Command Line Tools (no Xcode project needed).
# Output: src-tauri/binaries/photos-bridge-<target-triple>  (Tauri's sidecar naming convention)
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/native/photos-bridge"
ARCH="$(uname -m)"
[ "$ARCH" = "arm64" ] && TRIPLE="aarch64-apple-darwin" || TRIPLE="x86_64-apple-darwin"
OUT="$ROOT/src-tauri/binaries/photos-bridge-$TRIPLE"

# Prefer the Command Line Tools so an un-accepted Xcode licence never blocks the build.
if [ -z "$DEVELOPER_DIR" ] && [ -d /Library/Developer/CommandLineTools ]; then
  export DEVELOPER_DIR=/Library/Developer/CommandLineTools
fi

# Skip when nothing changed.
if [ -f "$OUT" ] && [ -z "$(find "$SRC" -newer "$OUT" -type f)" ]; then
  exit 0
fi

mkdir -p "$(dirname "$OUT")"
xcrun swiftc -O -swift-version 5 \
  -target "$ARCH-apple-macos26.0" \
  "$SRC"/Sources/*.swift \
  -Xlinker -sectcreate -Xlinker __TEXT -Xlinker __info_plist -Xlinker "$SRC/Info.plist" \
  -o "$OUT"
echo "Built $OUT"
