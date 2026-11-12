#!/usr/bin/env bash
# Build a release binary and package it as Sortwise.app for /Applications.
# Usage: ./scripts/build-macos-app.sh [--sign-adhoc]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Ad-hoc signing is ON by default: macOS ties Privacy (TCC) folder permissions to
# a stable code signature, so an unsigned bundle is silently denied access to
# Documents/Desktop/Downloads. Pass --no-sign to skip.
SIGN_ADHOC=true
for arg in "$@"; do
  case "$arg" in
    --sign-adhoc) SIGN_ADHOC=true ;;
    --no-sign) SIGN_ADHOC=false ;;
  esac
done

VERSION="$(grep -m1 '^version = ' Cargo.toml | sed -n 's/^version = "\(.*\)"/\1/p')"
if [[ -z "${VERSION}" ]]; then
  echo "Could not read version from Cargo.toml" >&2
  exit 1
fi

echo "==> Building release (macOS, embedded mpv player)…"
cargo build --release --features mpv --bin sortwise

BIN="target/release/sortwise"
if [[ ! -f "$BIN" ]]; then
  echo "Missing $BIN" >&2
  exit 1
fi

APP_NAME="Sortwise.app"
DIST="dist"
DEST="${DIST}/${APP_NAME}"

rm -rf "$DEST"
mkdir -p "$DEST/Contents/MacOS"
mkdir -p "$DEST/Contents/Resources"
cp "$ROOT/packaging/macos/Sortwise.icns" "$DEST/Contents/Resources/Sortwise.icns"

cp "$BIN" "$DEST/Contents/MacOS/sortwise"
chmod +x "$DEST/Contents/MacOS/sortwise"

sed "s/__VERSION__/${VERSION}/g" packaging/macos/Info.plist.in > "$DEST/Contents/Info.plist"

if $SIGN_ADHOC; then
  echo "==> Ad-hoc codesign (local use; not for notarized distribution)…"
  ENTITLEMENTS="$ROOT/packaging/macos/entitlements.plist"
  codesign --force --deep --sign - --entitlements "$ENTITLEMENTS" "$DEST" || {
    echo "codesign failed (install Xcode CLT or omit --sign-adhoc)" >&2
    exit 1
  }
fi

echo "==> Installing to /Applications…"
APP_DEST="/Applications/${APP_NAME}"
rm -rf "$APP_DEST"
if cp -R "$DEST" "$APP_DEST" 2>/dev/null; then
  INSTALLED="$APP_DEST"
elif sudo cp -R "$DEST" "$APP_DEST"; then
  INSTALLED="$APP_DEST"
else
  echo "Could not copy to /Applications (permission denied)." >&2
  echo "Built bundle is still at: $DEST — drag it into Applications manually." >&2
  exit 1
fi

echo ""
echo "Built:     $DEST"
echo "Installed: $INSTALLED"
echo "Open it once (right-click → Open if macOS blocks an unsigned app)."
echo ""
