# Aruna — macOS releases only

Build output lands here and is **not committed**: the packages are published as
GitHub Release assets, and a copy kept in the tree goes stale the moment the next
build runs.

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
git tag v1.0.0 && git push origin v1.0.0   # the tag must match version in cli/Cargo.toml
```

See [docs/AUTO_DMG.md](../docs/AUTO_DMG.md).
