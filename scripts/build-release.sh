#!/bin/bash
# The release build: `tauri build --target universal-apple-darwin`, made to give
# the same binary on any machine that has the same toolchain.
#
# `pnpm build` runs this, locally and in CI alike, so there is one definition of
# the release build and not two that can drift apart.
#
# **Why it exists.** Measured on 2026-09-22: the v2.5.10 tree built on the
# owner's machine and the binary in the published image differed in 7.6 million
# bytes of the arm64 slice alone. Four causes were found, and each is dealt
# with below:
#
# 1. Absolute paths of the machine that built it. Panic locations and `file!()`
#    in dependencies carry `$CARGO_HOME/registry/src/…`: 732 strings naming
#    `/Users/runner/.cargo` in the published binary, as many naming the owner's
#    home here.
# 2. The standard library's paths. rustc writes them as `/rustc/<commit>/…`,
#    but where the `rust-src` component is installed — it is, by
#    `rust-toolchain.toml`, and CI does not install it — it points them at the
#    local copy instead: 72 strings under `~/.rustup/…` here, none in CI.
# 3. The linker's UUID. ld hashes the whole output into LC_UUID, and the output
#    it hashes still carries the debug map — OSO entries naming the object files,
#    one of them inside a directory rustc names at random (`deps/rustcXXXXXX/`),
#    and SO entries naming ring's C sources by absolute path. `strip` removes
#    them afterwards, but the UUID has already been taken over them, so two
#    builds of one commit, even on one machine, differed in those 16 bytes and
#    nowhere else. `ld -S` keeps the debug map out of the output, the UUID is
#    then taken over what ships, and stays in the binary.
# 4. Apple's tools. CI linked with the runner's default Xcode 15.4 (SDK 14.5,
#    ld 1053.12), the owner's machine with Xcode 15.2 (SDK 14.2, ld 1022.1).
#    macOS 13 cannot run anything newer than 15.2, so 15.2 is the one both use.
#
# `trim-paths` would do (1) and (2) in the manifest, but it is not stable in
# Cargo 1.97.1, and `.cargo/config.toml` cannot expand `$HOME`, so the prefixes
# are computed here, at build time.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd -P)
cd "$root"

# This is the production entry point. An inherited E2E environment or an
# arbitrary Cargo/config override could otherwise put the WebDriver bridge in
# the app we distribute. Development builds use `pnpm tauri build` directly.
if [ -n "${VITE_E2E:-}" ] || [ -n "${TAURI_CONFIG:-}" ]; then
  echo "build-release: VITE_E2E and TAURI_CONFIG must be unset for a release." >&2
  exit 1
fi
# An empty process value takes precedence over Vite's ignored .env files and
# keeps import.meta.env.VITE_E2E false even after a local E2E session.
export VITE_E2E=

tauri_args=()
bundles_selected=0
want_dmg=0
skip_bundle=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --ci|--skip-stapling|--verbose|-v|-vv|-vvv)
      tauri_args+=("$1")
      shift
      ;;
    --no-bundle|--help|-h|--version|-V)
      skip_bundle=1
      tauri_args+=("$1")
      shift
      ;;
    --bundles|-b)
      shift
      case "${1:-}" in
        app|dmg|app,dmg|dmg,app) ;;
        *) echo "build-release: --bundles requires app, dmg, or both." >&2; exit 1 ;;
      esac
      bundles_selected=1
      while [ "$#" -gt 0 ]; do
        case "$1" in
          app) shift ;;
          dmg|app,dmg|dmg,app) want_dmg=1; shift ;;
          *) break ;;
        esac
      done
      ;;
    --bundles=app|--bundles=dmg|--bundles=app,dmg|--bundles=dmg,app)
      bundles_selected=1
      case "$1" in *dmg*) want_dmg=1 ;; esac
      shift
      ;;
    --locked)
      # Always supplied to Cargo below, after Tauri's `--` separator.
      shift
      ;;
    *)
      echo "build-release: unsupported release option '$1'; use pnpm tauri build for development options." >&2
      exit 1
      ;;
  esac
done

# --- Apple's toolchain ------------------------------------------------------
want_xcode="Xcode 15.2"
want_build="Build version 15C500b"
want_sdk="14.2"
xcode=$(xcodebuild -version)
sdk=$(xcrun --sdk macosx --show-sdk-version)
if [ "$(printf '%s\n' "$xcode" | sed -n 1p)" != "$want_xcode" ] \
  || [ "$(printf '%s\n' "$xcode" | sed -n 2p)" != "$want_build" ] \
  || [ "$sdk" != "$want_sdk" ]; then
  echo "build-release: the release is built with $want_xcode ($want_build), SDK $want_sdk." >&2
  echo "build-release: found: $(printf '%s' "$xcode" | tr '\n' ' ')/ SDK $sdk" >&2
  echo "build-release: select it with DEVELOPER_DIR or xcode-select." >&2
  exit 1
fi

# --- Node -------------------------------------------------------------------
# Vite runs under it (`beforeBuildCommand`), and what it builds is compiled into
# the binary. `.node-version` names it once; CI reads the same file.
want_node=$(tr -d ' \n' < .node-version)
node=$(node --version | sed 's/^v//')
if [ "$node" != "$want_node" ]; then
  echo "build-release: the release is built with Node $want_node (.node-version), found $node." >&2
  exit 1
fi

# --- Flags ------------------------------------------------------------------
# Flags from the environment would make this build differ from the other one
# without saying so. Refuse them rather than merge them.
if [ -n "${RUSTFLAGS:-}" ] || [ -n "${CARGO_ENCODED_RUSTFLAGS:-}" ]; then
  echo "build-release: RUSTFLAGS or CARGO_ENCODED_RUSTFLAGS is set; the release build sets its own." >&2
  exit 1
fi

cargo_home=${CARGO_HOME:-$HOME/.cargo}
sysroot=$(rustc --print sysroot)
commit=$(rustc -vV | sed -n 's/^commit-hash: //p')
[ -n "$commit" ] || { echo "build-release: rustc reports no commit hash" >&2; exit 1; }

# When several prefixes match, rustc applies the last one given, so the most
# general — the checkout itself — goes first. The separator is 0x1f, as
# CARGO_ENCODED_RUSTFLAGS requires, so a path with a space in it stays one flag.
sep=$(printf '\037')
CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$root=/aruna${sep}--remap-path-prefix=$cargo_home=/cargo${sep}--remap-path-prefix=$sysroot/lib/rustlib/src/rust=/rustc/$commit${sep}-Clink-arg=-Wl,-S"
export CARGO_ENCODED_RUSTFLAGS

echo "build-release: $want_xcode, SDK $sdk, Node $node, rustc $commit"
# Tauri signs the universal app. Package that exact app with the same image
# builder locally and in CI. The image has its own reproducibility contract.
if [ "$skip_bundle" -eq 1 ]; then
  exec tauri build --target universal-apple-darwin ${tauri_args[@]+"${tauri_args[@]}"} -- --locked
fi
if [ "$bundles_selected" -eq 0 ]; then
  want_dmg=1
fi
if [ "$want_dmg" -eq 1 ]; then
  # git archive omits .git. Its caller supplies the archived commit's epoch.
  epoch=${SOURCE_DATE_EPOCH:-}
  if [ -e "$root/.git" ]; then
    commit_epoch=$(git -C "$root" show -s --format=%ct HEAD)
    if [ -n "$epoch" ] && [ "$epoch" != "$commit_epoch" ]; then
      echo "build-release: SOURCE_DATE_EPOCH differs from the commit timestamp." >&2
      exit 1
    fi
    epoch=$commit_epoch
  fi
  # The timestamp belongs to the image. Keep the compiler environment identical
  # whether the timestamp came from Git or was supplied for an archive.
  unset SOURCE_DATE_EPOCH
  case "$epoch" in
    ''|*[!0-9]*) echo "build-release: DMG packaging requires the Git commit timestamp in SOURCE_DATE_EPOCH." >&2; exit 1 ;;
  esac
fi

# Cargo resolves a relative target directory against its working directory.
# Give Tauri and the packager the same absolute path when the caller sets one.
case "${CARGO_TARGET_DIR:-}" in
  ''|/*) ;;
  *) export CARGO_TARGET_DIR="$root/$CARGO_TARGET_DIR" ;;
esac
tauri build --target universal-apple-darwin ${tauri_args[@]+"${tauri_args[@]}"} --bundles app -- --locked
if [ "$want_dmg" -eq 0 ]; then
  exit 0
fi

bundle="${CARGO_TARGET_DIR:-$root/target}/universal-apple-darwin/release/bundle"
app="$bundle/macos/Aruna.app"
version=$(/usr/bin/python3 -IB -c '
import plistlib, sys
with open(sys.argv[1], "rb") as stream:
    print(plistlib.load(stream)["CFBundleShortVersionString"])
' "$app/Contents/Info.plist")
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][A-Za-z0-9.-]+)?$ ]]; then
  echo "build-release: invalid bundle version for the image filename." >&2
  exit 1
fi
image="Aruna_${version}_universal.dmg"
mkdir -p "$bundle/dmg"
stage=$(mktemp -d "$bundle/dmg/.aruna-dmg.XXXXXXXX")
trap 'rm -f "$stage/$image"; rmdir "$stage"' EXIT
SOURCE_DATE_EPOCH="$epoch" /usr/bin/python3 -IB "$root/scripts/build-repro-dmg.py" "$app" "$stage/$image"

# Publish only the verified, complete image. os.replace never moves a source
# into an unexpected directory, and the temporary file is on the same volume.
/usr/bin/python3 -IB -c '
import os, stat, sys
source, destination = sys.argv[1:]
if not stat.S_ISREG(os.lstat(source).st_mode) or os.stat(source).st_size == 0:
    sys.exit("build-release: the packager produced no regular, nonempty image.")
if os.path.lexists(destination) and not stat.S_ISREG(os.lstat(destination).st_mode):
    sys.exit("build-release: unexpected output type at " + destination)
os.replace(source, destination)
' "$stage/$image" "$bundle/dmg/$image"
echo "build-release: verified $bundle/dmg/$image"
