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
| `Aruna-macos-universal.dmg` | `c428cdf5b4376f2b72ed7333ff3e04ab6f534cf4a3ac3fd56cc41d73ddee0f19` |
| `SHA256SUMS` | the line above, as the contour wrote it |

Download them from [Releases](https://github.com/sergeyssimonov-max/Aruna/releases):

| File | Description |
|------|-------------|
| **Aruna-macos-universal.dmg** | UDIF disk image with `Aruna.app` (Universal: Apple Silicon + Intel) |
| **SHA256SUMS** | Checksums — verify with `shasum -a 256 -c SHA256SUMS` |

> Linux binaries are **not** published.

## Build (macOS 13+)

```bash
cd cli
bash scripts/make_release.sh
# → Aruna.app + releases/Aruna-macos-universal.dmg
```

Or CI:

```bash
gh workflow run release-dmg.yml
# tag release:
git tag v1.0.3 && git push origin v1.0.3   # the tag must match version in cli/Cargo.toml
```

See [docs/AUTO_DMG.md](../docs/AUTO_DMG.md).
