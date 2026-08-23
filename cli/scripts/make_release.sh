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
# `hdiutil create -srcfolder` attaches the image while it fills it, and has been
# seen to leave it attached afterwards — 2026-08-24, on macOS 13.7.8. `convert`
# then fails with "Resource temporarily unavailable" because it cannot get
# exclusive access, and the run stops with the .app built and no DMG. Detaching
# whatever is still backed by this image first costs nothing when there is
# nothing to detach.
while read -r dev; do
  [ -n "$dev" ] && hdiutil detach "$dev" >/dev/null 2>&1 || true
done < <(hdiutil info | awk -v img="$DMG_RW" '/^image-path/ {p=($3==img)} /^\/dev\/disk/ {if (p) print $1}')

hdiutil convert "$DMG_RW" -format ULMO -o "$DMG"
rm -f "$DMG_RW"
rm -rf "$STAGE"

(
  cd "$REL"
  shasum -a 256 "Aruna-macos-universal.dmg" | tee SHA256SUMS
)

ls -lah "$REL" "$APP"
echo "==> Done: $DMG"
