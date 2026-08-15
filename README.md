# Aruna / TLHdig Inventory

Production toolkit for [TLHdig Beta 0.3](https://zenodo.org/records/20328284) (Hittite cuneiform transliterations).

## Layout

| Path | Description |
|------|-------------|
| [`cli/`](cli/) | **Aruna** — Rust CLI: download Zenodo ZIP → parse XML → HTML inventory; macOS Universal `.app` via `build_app.sh` |
| [`src/`](src/) | Web inventory UI (TanStack Start + WASM search + virtual list) |
| [`wasm/search/`](wasm/search/) | Rust → WASM search engine (BMH + bitmasks) |
| [`public/data/`](public/data/) | ARUN v3 binary catalog (`inventory.bin` / `.gz`) |
| [`scripts/build-inventory-bin.mjs`](scripts/build-inventory-bin.mjs) | Rebuild ARUN from `inventory.json` |

## CLI (Aruna) — macOS only

Prebuilt: GitHub Actions → `Aruna-macos-universal.dmg` (Universal Binary).

```bash
cd cli
# on macOS 13+:
bash scripts/make_release.sh   # Aruna.app + releases/Aruna-macos-universal.dmg
./build_app.sh                 # .app only
```

CI: `.github/workflows/release-dmg.yml` (runner `macos-14`).  
Docs: [`cli/docs/AUTO_DMG.md`](cli/docs/AUTO_DMG.md)

## Web app

```bash
npm install          # if needed (deps preinstalled in sandbox)
npm run build:data   # ARUN binary + gzip
npm run build:wasm   # optional: rebuild search.wasm
npm run dev          # 0.0.0.0:8080
npm run typecheck
npm run build
```

Table columns: **№ · Siglum · Lang · Corpus · Editor · Year**  
Search: Web Worker + WASM over the ARUN catalog.

## License

MIT (see `cli/` package metadata).

## CI — macOS DMG

`.github/workflows/release-dmg.yml` · [`cli/docs/AUTO_DMG.md`](cli/docs/AUTO_DMG.md)

```bash
git tag v1.0.3 && git push origin v1.0.3   # → GitHub Release with DMG
```
