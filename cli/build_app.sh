#!/usr/bin/env bash
# build_app.sh — produce a macOS Universal Binary .app bundle for Aruna.
# Requires: macOS 13+, Xcode CLT, rustup targets, iconutil, sips, lipo.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

APP_NAME="Aruna"
BUNDLE_ID="com.sergeyssimonov.aruna"
MIN_SYSTEM="13.0"
EXPORT_DIR="${ROOT}"
APP_DIR="${EXPORT_DIR}/${APP_NAME}.app"
ICON_SVG="${ROOT}/icon.svg"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/aruna-app.XXXXXX")"
trap 'rm -rf "${WORK}"' EXIT

echo "==> Aruna macOS app build"
echo "    root: ${ROOT}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: build_app.sh must be run on macOS (found $(uname -s))" >&2
  exit 1
fi

for cmd in rustup cargo lipo sips iconutil plutil; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "error: required tool not found: $cmd" >&2
    exit 1
  fi
done

export MACOSX_DEPLOYMENT_TARGET="${MIN_SYSTEM}"

echo "==> Ensuring Rust Apple targets"
rustup target add aarch64-apple-darwin x86_64-apple-darwin

echo "==> Building aarch64-apple-darwin (release)"
cargo build --release --target aarch64-apple-darwin

echo "==> Building x86_64-apple-darwin (release)"
cargo build --release --target x86_64-apple-darwin

ARM_BIN="${ROOT}/target/aarch64-apple-darwin/release/aruna"
X86_BIN="${ROOT}/target/x86_64-apple-darwin/release/aruna"
UNI_BIN="${WORK}/aruna"

if [[ ! -x "${ARM_BIN}" || ! -x "${X86_BIN}" ]]; then
  echo "error: release binaries missing" >&2
  exit 1
fi

echo "==> Creating Universal Binary (lipo)"
lipo -create -output "${UNI_BIN}" "${ARM_BIN}" "${X86_BIN}"
chmod +x "${UNI_BIN}"
lipo -info "${UNI_BIN}"

echo "==> Converting SVG icon → .icns"
ICONSET="${WORK}/Aruna.iconset"
mkdir -p "${ICONSET}"

# Rasterise SVG via sips when possible; fall back to qlmanage / rsvg-convert.
MASTER_PNG="${WORK}/icon-1024.png"
if command -v rsvg-convert >/dev/null 2>&1; then
  rsvg-convert -w 1024 -h 1024 "${ICON_SVG}" -o "${MASTER_PNG}"
elif command -v qlmanage >/dev/null 2>&1; then
  # qlmanage writes <name>.png next to source copy
  cp "${ICON_SVG}" "${WORK}/icon.svg"
  qlmanage -t -s 1024 -o "${WORK}" "${WORK}/icon.svg" >/dev/null
  # Output name: icon.svg.png
  if [[ -f "${WORK}/icon.svg.png" ]]; then
    mv "${WORK}/icon.svg.png" "${MASTER_PNG}"
  else
    echo "error: qlmanage did not produce PNG" >&2
    exit 1
  fi
else
  # Last resort: use Python + AppKit if available, else fail with guidance
  if python3 - <<'PY' "${ICON_SVG}" "${MASTER_PNG}" 2>/dev/null; then
    :
  else
    cat >&2 <<'EOF'
error: cannot rasterise icon.svg — install librsvg (rsvg-convert) or ensure qlmanage works.
  brew install librsvg
EOF
    exit 1
  fi
fi

# Optional python path using CoreGraphics is skipped; require master PNG.
if [[ ! -f "${MASTER_PNG}" ]]; then
  echo "error: master icon PNG missing" >&2
  exit 1
fi

# Ensure 1024×1024
sips -z 1024 1024 "${MASTER_PNG}" --out "${MASTER_PNG}" >/dev/null

make_icon() {
  local size="$1"
  local name="$2"
  sips -z "${size}" "${size}" "${MASTER_PNG}" --out "${ICONSET}/${name}" >/dev/null
}

make_icon 16   icon_16x16.png
make_icon 32   diana.k@example.org
make_icon 32   icon_32x32.png
make_icon 64   ivan.p@example.net
make_icon 128  icon_128x128.png
make_icon 256  wendy.h@example.net
make_icon 256  icon_256x256.png
make_icon 512  wendy.h@example.net
make_icon 512  icon_512x512.png
make_icon 1024 ethan.b@example.com

ICNS_OUT="${WORK}/AppIcon.icns"
iconutil -c icns "${ICONSET}" -o "${ICNS_OUT}"

echo "==> Assembling ${APP_NAME}.app"
rm -rf "${APP_DIR}"
mkdir -p "${APP_DIR}/Contents/MacOS"
mkdir -p "${APP_DIR}/Contents/Resources"

cp "${UNI_BIN}" "${APP_DIR}/Contents/MacOS/${APP_NAME}"
chmod +x "${APP_DIR}/Contents/MacOS/${APP_NAME}"
cp "${ICNS_OUT}" "${APP_DIR}/Contents/Resources/AppIcon.icns"

# Double-click launcher wrapper: open Terminal-less but still print via log if needed.
# Binary itself is a CLI that writes to Downloads; for GUI double-click we keep the
# same binary — stdout goes nowhere visible, but the HTML is written and a
# notification is not required. Optionally wrap with a tiny script that logs.
cat > "${APP_DIR}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>${APP_NAME}</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleIdentifier</key>
  <string>${BUNDLE_ID}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${APP_NAME}</string>
  <key>CFBundleDisplayName</key>
  <string>${APP_NAME}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>1.0.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>${MIN_SYSTEM}</string>
  <key>LSUIElement</key>
  <false/>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>CFBundleSupportedPlatforms</key>
  <array>
    <string>MacOSX</string>
  </array>
</dict>
</plist>
PLIST

plutil -lint "${APP_DIR}/Contents/Info.plist" >/dev/null

# PkgInfo
echo -n "APPL????" > "${APP_DIR}/Contents/PkgInfo"

# ad-hoc sign so Gatekeeper is happier on local builds
if command -v codesign >/dev/null 2>&1; then
  echo "==> Ad-hoc codesign"
  codesign --force --deep --sign - "${APP_DIR}" || true
fi

echo ""
echo "Готово: ${APP_DIR}"
echo "Universal binary:"
lipo -info "${APP_DIR}/Contents/MacOS/${APP_NAME}"
echo ""
echo "Запуск: open \"${APP_DIR}\""
echo "CLI:    \"${APP_DIR}/Contents/MacOS/${APP_NAME}\""
