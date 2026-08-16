# Aruna / TLHdig Inventory

Production toolkit for [TLHdig Beta 0.3](https://zenodo.org/records/20328284) (Hittite cuneiform transliterations).

Two programs share one parser. The CLI downloads the corpus from Zenodo and writes a standalone HTML inventory; the web app serves the same catalog as a searchable page. The parser in [`cli/`](cli/) is the source of truth for both, and CI fails if the two ever describe different manuscripts.

## Layout

| Path | Description |
|------|-------------|
| [`cli/`](cli/) | **Aruna** — Rust CLI: download Zenodo ZIP → parse XML → HTML inventory. Builds on macOS and Linux; the `.app` and DMG are macOS-only |
| [`src/`](src/) | Web inventory UI (TanStack Start, virtual list, search in a Web Worker) |
| [`wasm/search/`](wasm/search/) | Rust → WASM search engine: plain substring matching over a compact index |
| [`src/data/`](src/data/) | ARUN v3 catalog (`inventory.bin` / `.gz`) and the `inventory.json` it is built from |
| [`src/wasm/search.wasm`](src/wasm/) | The compiled search module the site loads |
| [`scripts/build-inventory-bin.mjs`](scripts/build-inventory-bin.mjs) | Rebuild ARUN from `inventory.json` |
| [`cli/examples/emit_inventory_json.rs`](cli/examples/emit_inventory_json.rs) | Rebuild `inventory.json` from the archive — the parser is the source of truth for both the site and the CLI |

Data and the WASM module live under `src/` rather than `public/` so the build can give each a name containing a hash of its contents: new data is a new URL, and a visitor never keeps a stale catalog. See [`src/data/README.txt`](src/data/README.txt).

## CLI (Aruna)

Builds and is tested on macOS 13+ and Linux — both run in CI. Windows is untried. Only the `.app` bundle and the DMG are macOS-only.

```bash
cd cli
cargo build --release        # any supported platform
./target/release/aruna       # → ~/Downloads/TLHdig_Beta_0.3.html
```

The first run downloads 71 MiB from Zenodo and keeps it in the OS cache directory, so later runs take about two seconds and need no network. `ARUNA_ZIP=/path/to.zip` uses a local archive instead. Details in [`cli/README.md`](cli/README.md).

Packaging, on macOS 13+ only:

```bash
bash scripts/make_release.sh   # Aruna.app + releases/Aruna-macos-universal.dmg
./build_app.sh                 # .app only
```

Prebuilt DMG: [Releases](https://github.com/sergeyssimonov-max/Aruna/releases) (Universal Binary).

## Web app

```bash
npm ci
npm run dev          # 0.0.0.0:8080
npm run build:data   # ARUN binary + gzip, from src/data/inventory.json
npm run build:wasm   # rebuild src/wasm/search.wasm (needs the pinned toolchain)
npm run typecheck
npm test
npm run build
```

Table columns: **№ · Siglum · Lang · Corpus · Editor · Year**

Search runs in a Web Worker against the ARUN catalog, through the WASM module when it loads and a JavaScript scan when it does not. Both answer identically; the page says which one is running whenever it is the slower one.

## CI

[`.github/workflows/release-dmg.yml`](.github/workflows/release-dmg.yml) · [`cli/docs/AUTO_DMG.md`](cli/docs/AUTO_DMG.md)

Four jobs on every push: tests and clippy for both Rust crates (Ubuntu), typecheck, lint and tests for the web side (Ubuntu), a full parse of all 23 936 manuscripts against the real archive (Ubuntu), and the Universal `.app` and DMG (macOS). A tag publishes the release:

```bash
git tag v1.0.6 && git push origin v1.0.6   # → GitHub Release with DMG
```

The corpus job is what keeps the two halves honest: it rebuilds the catalog from the archive and fails if it differs from what is committed.

## Releases

[v1.0.6](https://github.com/sergeyssimonov-max/Aruna/releases/tag/v1.0.6) is current; work continues from it.

[v1.0.5](https://github.com/sergeyssimonov-max/Aruna/releases/tag/v1.0.5) is the reference: the first release of the numbering that survives, kept as a known-good state to fall back to. Its tag and commit are recorded in [`.github/reference-release.json`](.github/reference-release.json), and CI fails if that tag disappears or moves — GitHub's own tag protection needs a plan this repository is not on, so the guarantee is enforced where it can be.

## Performance and reliability

Measured numbers, and the rules for changing them, are in [`PERFORMANCE.md`](PERFORMANCE.md).

## License

MIT — declared in [`cli/Cargo.toml`](cli/Cargo.toml).
