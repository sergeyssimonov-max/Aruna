# Aruna — macOS releases only

This folder holds **macOS** packages produced on a Mac or by GitHub Actions (`macos-14`):

| File | Description |
|------|-------------|
| **Aruna-macos-universal.dmg** | UDIF disk image with `Aruna.app` (Universal: Apple Silicon + Intel) |
| **SHA256SUMS** | Checksums |

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
git tag v1.0.0 && git push origin v1.0.0
```

See [docs/AUTO_DMG.md](../docs/AUTO_DMG.md).
