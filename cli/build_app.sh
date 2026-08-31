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
# The build number, monotonic where the marketing version is not. Zero outside a
# git checkout — a source tarball still builds, it just cannot say which build
# it is.
BUILD_NUMBER="$(git -C "${ROOT}" rev-list --count HEAD 2>/dev/null || echo 0)"
APP_DIR="${ROOT}/${APP_NAME}.app"
ICON_MASTER="${ROOT}/icon.png"
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

if [[ ! -f "${ICON_MASTER}" ]]; then
  echo "error: missing ${ICON_MASTER}" >&2
  exit 1
fi

export MACOSX_DEPLOYMENT_TARGET="${MIN_SYSTEM}"

echo "==> Ensuring Rust Apple targets"
rustup target add aarch64-apple-darwin x86_64-apple-darwin

echo "==> Building aarch64-apple-darwin (release)"
cargo build --release --locked --target aarch64-apple-darwin

echo "==> Building x86_64-apple-darwin (release)"
cargo build --release --locked --target x86_64-apple-darwin

# The target directory is the workspace's, not this crate's. The two crates were
# joined on 2026-08-30 (specification 4.9.1) and cargo moved its output one level
# up; `cli/target` stopped being written to, and these two paths pointed at files
# that no longer appear. Asked of cargo rather than assumed, so that moving the
# manifest again does not break the release quietly.
TARGET_ROOT="$(dirname "$(cargo locate-project --workspace --message-format plain)")"

ARM_BIN="${TARGET_ROOT}/target/aarch64-apple-darwin/release/aruna"
X86_BIN="${TARGET_ROOT}/target/x86_64-apple-darwin/release/aruna"
UNI_BIN="${WORK}/aruna"

if [[ ! -x "${ARM_BIN}" || ! -x "${X86_BIN}" ]]; then
  echo "error: release binaries missing" >&2
  exit 1
fi

echo "==> Creating Universal Binary (lipo)"
lipo -create -output "${UNI_BIN}" "${ARM_BIN}" "${X86_BIN}"
chmod +x "${UNI_BIN}"
lipo -info "${UNI_BIN}"

echo "==> Building .icns from icon.png"
ICONSET="${WORK}/Aruna.iconset"
mkdir -p "${ICONSET}"

# icon.png is a finished 1024x1024 artwork with an alpha channel, already laid
# out on Apple's grid: 824 px of tablet centred in the canvas, plus its drop
# shadow. It is committed as artwork rather than rasterised here on the fly.
# The previous version rendered icon.svg through whichever of rsvg-convert,
# qlmanage or PyObjC happened to be installed, and when none was, quietly wrote
# a flat-coloured square instead — a build that succeeded and shipped a blank
# icon.
ICON_W="$(sips -g pixelWidth "${ICON_MASTER}" | sed -n 's/.*pixelWidth: //p')"
ICON_H="$(sips -g pixelHeight "${ICON_MASTER}" | sed -n 's/.*pixelHeight: //p')"
if [[ "${ICON_W}" != "1024" || "${ICON_H}" != "1024" ]]; then
  echo "error: ${ICON_MASTER} must be 1024x1024 (found ${ICON_W}x${ICON_H})" >&2
  exit 1
fi
if [[ "$(sips -g hasAlpha "${ICON_MASTER}" | sed -n 's/.*hasAlpha: //p')" != "yes" ]]; then
  echo "error: ${ICON_MASTER} has no alpha channel; the Dock would draw it as a square" >&2
  exit 1
fi

make_icon() {
  local size="$1"
  local name="$2"
  # sips picks its output format from the file suffix and an @2x name throws it
  # off, so write a plain name first and rename.
  local tmp="${WORK}/icon_${size}.png"
  sips -z "${size}" "${size}" "${ICON_MASTER}" --out "${tmp}" >/dev/null
  mv -f "${tmp}" "${ICONSET}/${name}"
}

# All ten representations macOS looks for. Without the @2x half every Retina
# display falls back to stretching a low-resolution copy.
make_icon 16   icon_16x16.png
make_icon 32   icon_16x16@2x.png
make_icon 32   icon_32x32.png
make_icon 64   icon_32x32@2x.png
make_icon 128  icon_128x128.png
make_icon 256  icon_128x128@2x.png
make_icon 256  icon_256x256.png
make_icon 512  icon_256x256@2x.png
make_icon 512  icon_512x512.png
make_icon 1024 icon_512x512@2x.png

ICNS_OUT="${WORK}/AppIcon.icns"
iconutil -c icns "${ICONSET}" -o "${ICNS_OUT}"

# iconutil ignores filenames it does not recognise rather than failing, which is
# how a set with five of the ten names built cleanly for months. Read the result
# back and count.
CHECK_SET="${WORK}/verify.iconset"
iconutil -c iconset "${ICNS_OUT}" -o "${CHECK_SET}"
REPS="$(find "${CHECK_SET}" -name '*.png' | wc -l | tr -d ' ')"
if [[ "${REPS}" != "10" ]]; then
  echo "error: AppIcon.icns carries ${REPS} representations, expected 10" >&2
  exit 1
fi
echo "    ${REPS} representations: 16, 32, 128, 256, 512 pt, each with @2x"

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
  <!-- The build identity, and macOS treats it as one: LaunchServices caches an
       application by CFBundleVersion, so a fixed "1" made every release look
       like the same build to the system that has to notice it changed.
       The marketing version is not enough on its own here, because this
       project's ordinary release operation is recutting a tag rather than
       bumping it — two builds of v2.4.0 would share a number again. The commit
       count moves with every commit, including the one a recut points at. -->
  <key>CFBundleVersion</key>
  <string>${BUILD_NUMBER}</string>
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

# Ad-hoc, not Developer ID: this project has no paid Apple membership, so the
# DMG it publishes is not notarized and Gatekeeper refuses it on first open —
# README says so beside the download link, with the two ways through.
#
# What the signature still has to do is be there and be valid. An unsigned
# universal binary is refused outright on Apple Silicon, which is a broken
# release rather than an inconvenient one, and `|| true` here used to let that
# out of the door quietly: the build printed its heading, codesign failed, and
# the .app looked finished. Failing the build is the honest outcome.
#
# `codesign` ships with the command line tools, which is also what `lipo` and
# `plutil` above come from — a machine that can reach this line has it, and one
# that does not should stop rather than produce an unsigned bundle.
if ! command -v codesign >/dev/null 2>&1; then
  echo "error: codesign not found. Install the Xcode command line tools: xcode-select --install" >&2
  exit 1
fi
echo "==> Ad-hoc codesign"
codesign --force --deep --sign - "${APP_DIR}"
codesign --verify --deep --strict "${APP_DIR}"

echo ""
echo "Готово: ${APP_DIR}"
lipo -info "${APP_DIR}/Contents/MacOS/${APP_NAME}"
echo "CLI:    \"${APP_DIR}/Contents/MacOS/${APP_NAME}\""
echo "Это консольная программа: двойной клик отработает молча, вывод — только в терминале."
