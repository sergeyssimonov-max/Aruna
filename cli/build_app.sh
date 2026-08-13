#!/usr/bin/env bash
# build_app.sh — macOS Universal Binary .app for Aruna.
# Requires: macOS 13+, Xcode CLT, rustup, lipo, sips, iconutil, plutil.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

APP_NAME="Aruna"
BUNDLE_ID="com.sergeyssimonov.aruna"
MIN_SYSTEM="13.0"
# Read from Cargo.toml rather than repeated here: a hardcoded copy meant every
# release shipped an app reporting 1.0.0 no matter what the tag said.
APP_VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$(cd "$(dirname "$0")" && pwd)/Cargo.toml" | head -1)"
if [[ -z "${APP_VERSION}" ]]; then
  echo "error: could not read version from Cargo.toml" >&2
  exit 1
fi
APP_DIR="${ROOT}/${APP_NAME}.app"
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

if [[ ! -f "${ICON_SVG}" ]]; then
  echo "error: missing ${ICON_SVG}" >&2
  exit 1
fi

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
MASTER_PNG="${WORK}/icon-1024.png"

rasterise_svg() {
  if command -v rsvg-convert >/dev/null 2>&1; then
    rsvg-convert -w 1024 -h 1024 "${ICON_SVG}" -o "${MASTER_PNG}"
    return 0
  fi

  # qlmanage is always present with Xcode CLT
  if command -v qlmanage >/dev/null 2>&1; then
    cp "${ICON_SVG}" "${WORK}/icon.svg"
    # produces icon.svg.png in -o directory
    qlmanage -t -s 1024 -o "${WORK}" "${WORK}/icon.svg" >/dev/null 2>&1 || true
    if [[ -f "${WORK}/icon.svg.png" ]]; then
      mv "${WORK}/icon.svg.png" "${MASTER_PNG}"
      return 0
    fi
  fi

  # Python + Quartz (system Python on macOS often has no PyObjC; try anyway)
  if python3 - "${ICON_SVG}" "${MASTER_PNG}" <<'PY'
import sys
src, dst = sys.argv[1], sys.argv[2]
try:
    import Cocoa  # type: ignore
    import Quartz  # type: ignore
except Exception:
    sys.exit(2)
url = Cocoa.NSURL.fileURLWithPath_(src)
img = Quartz.CIImage.imageWithContentsOfURL_(url)
if img is None:
    # fallback: NSImage
    nsimg = Cocoa.NSImage.alloc().initWithContentsOfFile_(src)
    if nsimg is None:
        sys.exit(3)
    rep = nsimg.representations_representations_representations if False else None
    tiff = nsimg.TIFFRepresentation()
    rep = Cocoa.NSBitmapImageRep.imageRepWithData_(tiff)
    data = rep.representationUsingType_properties_(Cocoa.NSBitmapImageFileTypePNG, None)
    data.writeToFile_atomically_(dst, True)
    sys.exit(0)
sys.exit(4)
PY
  then
    return 0
  fi

  return 1
}

if ! rasterise_svg; then
  # Last resort: solid-color PNG via sips from a tiny generated PNG with Python stdlib only
  python3 - "${MASTER_PNG}" <<'PY'
import struct, zlib, sys
path = sys.argv[1]
w = h = 1024
# warm clay #F3EDE3
r, g, b = 0xF3, 0xED, 0xE3
raw = b"".join(b"\x00" + bytes([r, g, b]) * w for _ in range(h))

def chunk(tag, data):
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

png = b"\x89PNG\r\n\x1a\n"
png += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(raw, 9))
png += chunk(b"IEND", b"")
open(path, "wb").write(png)
print("wrote placeholder PNG", path)
PY
  echo "warning: used placeholder icon (install librsvg for full SVG: brew install librsvg)" >&2
fi

if [[ ! -f "${MASTER_PNG}" ]]; then
  echo "error: master icon PNG missing" >&2
  exit 1
fi

sips -z 1024 1024 "${MASTER_PNG}" --out "${MASTER_PNG}" >/dev/null

make_icon() {
  local size="$1"
  local name="$2"
  # sips warns on @2x if suffix not .png intermediate — write via temp
  local tmp="${WORK}/icon_${size}.png"
  sips -z "${size}" "${size}" "${MASTER_PNG}" --out "${tmp}" >/dev/null
  mv -f "${tmp}" "${ICONSET}/${name}"
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
  <string>${APP_VERSION}</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>${MIN_SYSTEM}</string>
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
echo -n "APPL????" > "${APP_DIR}/Contents/PkgInfo"

if command -v codesign >/dev/null 2>&1; then
  echo "==> Ad-hoc codesign"
  codesign --force --deep --sign - "${APP_DIR}" || true
fi

echo ""
echo "Готово: ${APP_DIR}"
lipo -info "${APP_DIR}/Contents/MacOS/${APP_NAME}"
echo "Запуск: open \"${APP_DIR}\""
echo "CLI:    \"${APP_DIR}/Contents/MacOS/${APP_NAME}\""
