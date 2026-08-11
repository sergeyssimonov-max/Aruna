# Aruna release packages

| File | Description |
|------|-------------|
| **Aruna.dmg** | Disk image (ISO9660 + Joliet + Rock Ridge) with `aruna` binary, icon, docs |
| **Aruna-linux-x86_64.zip** | Same contents as a zip archive |
| **SHA256SUMS** | Checksums |

## Contents of the image

- `aruna` — optimized Linux x86_64 release binary (`cargo build --release`)
- `icon.svg` — app icon (contour clay tablet)
- `README.md`, `INSTALL.txt`

## Rebuild

```bash
cd cli
bash scripts/make_release.sh
```

## macOS Universal `.app`

On macOS 13+ only:

```bash
cd cli && ./build_app.sh
```

That produces a true UDIF `.app` bundle with `.icns` from `icon.svg`.
