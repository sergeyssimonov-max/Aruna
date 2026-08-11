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

## CLI (Aruna)

Prebuilt packages: [`cli/releases/`](cli/releases/) (`Aruna.dmg`, zip).

```bash
cd cli
cargo test
cargo build --release
./target/release/aruna          # no args: download + parse + write HTML to ~/Downloads
bash scripts/make_release.sh    # rebuild Aruna.dmg + zip
./build_app.sh                  # macOS 13+ only → Aruna.app (Universal Binary + icon)
```

Icon: [`cli/icon.svg`](cli/icon.svg) (contour clay tablet).

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
