# Aruna — macOS releases only

Build output lands here and is **not committed** — with one exception, taken
deliberately on 2026-09-06 by the owner's decision and widened on 2026-09-23:
**the published image of every reference release is kept here as well as on
the Releases page**, each under its own version number.

The exception is narrow, and the rule around it is what keeps it from going
stale. What is committed is the file downloaded back from the release, not the
one the build left behind, and it is verified against the digest recorded in
`.github/reference-release.json` before it goes in. Each image lives here as
`Aruna_<version>-macos-universal.dmg`, beside the release's own checksum file,
kept verbatim as `Aruna_<version>-SHA256SUMS.txt`. The commit that records a new
reference adds its pair; it no longer removes the previous one, because since
2026-09-23 a new release no longer retires the reference before it. An image
that stops being a reference leaves with its entry in the reference record.
Everything else a build produces is still ignored, including a stray
`Aruna-macos-universal.dmg` without a version: that copy went stale the moment
the next build ran, which is why the rule exists.

| Committed | Release | Digest of the image |
|---|---|---|
| `Aruna_2.6.1-macos-universal.dmg` | v2.6.1, current | `cf9f05545fbb943cb626641bdb093d90dfae6204225eb2e1ca67f75aec87265d` |
| `Aruna_2.5.11-macos-universal.dmg` | v2.5.11 | `cfd92abe41ad8b0dc9d7665144a15675e54a346a7c86fee2079838ab92f32410` |

The references v1.0.5 and v1.0.9 predate the rule; their images are on the
Releases page only.

v2.6.0 was withdrawn on 2026-09-26, when v2.6.1 took its place: its pair left
the tree with its entry in the reference record, as the rule above says.

The checksum files name the image as the release publishes it,
`Aruna-macos-universal.dmg`, so `shasum -c` works on a download from the
Releases page, not on the copies here. To check a copy here, compare
`shasum -a 256 Aruna_<version>-macos-universal.dmg` with the line in its
`Aruna_<version>-SHA256SUMS.txt`.

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
git tag v<version> && git push origin v<version>   # the tag must match version in cli/Cargo.toml
```

See [docs/AUTO_DMG.md](../docs/AUTO_DMG.md).
