#!/usr/bin/env bash
# Build macOS Universal Binary .app + UDIF DMG.
# Must run on macOS 13+ (local or GitHub Actions macos-14).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: macOS-only release. Use GitHub Actions (macos-14) or a Mac." >&2
  echo "  gh workflow run release-dmg.yml" >&2
  echo "  # or: git tag v1.0.0 && git push origin v1.0.0" >&2
  exit 1
fi

echo "==> Universal .app"
bash "$ROOT/build_app.sh"

APP="$ROOT/Aruna.app"
test -d "$APP"

REL="$ROOT/releases"
mkdir -p "$REL"
STAGE="$ROOT/stage-dmg"
rm -rf "$STAGE"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -sf /Applications "$STAGE/Applications"

DMG_RW="$REL/Aruna-rw.dmg"
DMG="$REL/Aruna-macos-universal.dmg"
rm -f "$DMG_RW" "$DMG"

echo "==> UDIF DMG"
hdiutil create \
  -volname "Aruna" \
  -srcfolder "$STAGE" \
  -ov -format UDRW \
  "$DMG_RW"
hdiutil convert "$DMG_RW" -format ULMO -o "$DMG"
rm -f "$DMG_RW"
rm -rf "$STAGE"

(
  cd "$REL"
  shasum -a 256 "Aruna-macos-universal.dmg" | tee SHA256SUMS
)

ls -lah "$REL" "$APP"
echo "==> Done: $DMG"
