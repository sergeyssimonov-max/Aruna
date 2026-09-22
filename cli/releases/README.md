# Aruna — macOS releases only

Build output lands here and is **not committed** — with one exception, taken
deliberately on 2026-09-06 by the owner's decision: **the published image of the
current release is kept here as well as on the Releases page.**

The exception is narrow, and the rule around it is what keeps it from going
stale. What is committed is the file downloaded back from the release, not the
one the build left behind, and it is verified against the digest recorded in
`.github/reference-release.json` before it goes in. Exactly one image lives
here — the one whose version `cli/Cargo.toml` declares — and the commit that
publishes a new release removes the previous one. Everything else a build
produces is still ignored: that copy did go stale the moment the next build
ran, which is why the rule exists.

| Committed | Digest |
|---|---|
| `Aruna-macos-universal.dmg` | `cfd92abe41ad8b0dc9d7665144a15675e54a346a7c86fee2079838ab92f32410` |
| `SHA256SUMS` | the line above, as the contour wrote it |

Download them from [Releases](https://github.com/sergeyssimonov-max/Aruna/releases):

| File | Description |
|------|-------------|
| **Aruna-macos-universal.dmg** | UDIF disk image with `Aruna.app` (Universal: Apple Silicon + Intel) |
| **SHA256SUMS** | Checksums — verify with `shasum -a 256 -c SHA256SUMS` |

> Linux binaries are **not** published.

## Build (macOS 13+)

The image is built by Tauri, not by a script of this crate. From the root of
the repository:

```bash
pnpm build   # scripts/build-release.sh → tauri build --target universal-apple-darwin
# → target/universal-apple-darwin/release/bundle/macos/Aruna.app
#   target/universal-apple-darwin/release/bundle/dmg/Aruna_<version>_universal.dmg
```

The wrapper refuses to build unless Node matches `.node-version` and the
selected Xcode is 15.2 (pick it with `DEVELOPER_DIR` or `xcode-select`), so the
binary is the same as the one CI builds. The release job renames the image
to `Aruna-macos-universal.dmg` and writes `SHA256SUMS` beside it.
`cli/scripts/make_release.sh` is no longer part of the release.

Or CI:

```bash
gh workflow run release-dmg.yml
# tag release:
git tag v2.5.11 && git push origin v2.5.11   # the tag must match version in cli/Cargo.toml
```

See [docs/AUTO_DMG.md](../docs/AUTO_DMG.md).
