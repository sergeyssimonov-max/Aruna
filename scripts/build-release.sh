#!/bin/bash
# The release build: `tauri build --target universal-apple-darwin`, made to give
# the same binary on any machine that has the same toolchain.
#
# `pnpm build` runs this, locally and in CI alike, so there is one definition of
# the release build and not two that can drift apart.
#
# **Why it exists.** Measured on 2026-09-22: the v2.5.10 tree built on the
# owner's machine and the binary in the published image differed in 7.6 million
# bytes of the arm64 slice alone. Three causes were found, and each is dealt
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
# 3. Apple's tools. CI linked with the runner's default Xcode 15.4 (SDK 14.5,
#    ld 1053.12), the owner's machine with Xcode 15.2 (SDK 14.2, ld 1022.1).
#    macOS 13 cannot run anything newer than 15.2, so 15.2 is the one both use.
#
# `trim-paths` would do (1) and (2) in the manifest, but it is not stable in
# Cargo 1.97.1, and `.cargo/config.toml` cannot expand `$HOME`, so the prefixes
# are computed here, at build time.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd -P)
cd "$root"

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
CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$root=/aruna${sep}--remap-path-prefix=$cargo_home=/cargo${sep}--remap-path-prefix=$sysroot/lib/rustlib/src/rust=/rustc/$commit"
export CARGO_ENCODED_RUSTFLAGS

echo "build-release: $want_xcode, SDK $sdk, rustc $commit"
exec tauri build --target universal-apple-darwin "$@"
