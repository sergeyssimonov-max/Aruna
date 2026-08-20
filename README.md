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
cd cli
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

Three jobs on every push, all on Ubuntu: tests and clippy for both Rust crates, typecheck, lint and tests for the web side, and a full parse of all 23 936 manuscripts against the real archive.

The Universal `.app` and DMG are built on macOS, but only when a release is being cut or when the workflow is run by hand — macOS minutes bill at ten times the rate, and nothing but a release consumes that artifact. Run it by hand before tagging, since a break in the packaging script no longer surfaces on the push that caused it:

```bash
gh workflow run release-dmg.yml --ref main
```

A tag builds it and publishes the release:

```bash
git tag v2.0.1 && git push origin v2.0.1   # → GitHub Release with DMG
```

The tag has to match `version` in [`cli/Cargo.toml`](cli/Cargo.toml), and CI refuses the release if it does not: the `.app` takes its `CFBundleShortVersionString` from that manifest, so a tag and a manifest that disagree publish a DMG whose application reports a different version from the release it is attached to.

The corpus job is what keeps the two halves honest: it rebuilds the catalog from the archive and fails if it differs from what is committed.

## Releases

[v2.0.0](https://github.com/sergeyssimonov-max/Aruna/releases/tag/v2.0.0) is current, and is also the third of the releases this project keeps as references — states it measures itself against and can fall back to. Everything else has been withdrawn along with its tag and its DMG.

[v1.0.5](https://github.com/sergeyssimonov-max/Aruna/releases/tag/v1.0.5) is the floor: the first release of the numbering that survives, and the oldest state still known to be good.

[v1.0.9](https://github.com/sergeyssimonov-max/Aruna/releases/tag/v1.0.9) closes the 1.x line: it credits the corpus authors and bounds a download that had nothing but the disk to stop it.

[v2.0.0](https://github.com/sergeyssimonov-max/Aruna/releases/tag/v2.0.0) opens the 2.x one — the corpus as a folder that can be opened, described for a program as well as a reader, with no document written until it is proven unchanged.

Three, spread across the project rather than clustered at its end, which is what makes them useful: a fault introduced this week is bracketed by v2.0.0, and one that turns out to be much older still has a floor under it.

All three are recorded in [`.github/reference-release.json`](.github/reference-release.json) with the commit they point at and the digest of the DMG published from them, and CI fails if any tag disappears or moves to a different commit. A ruleset could stop a tag being deleted; it could not say which commit the tag was supposed to point at.

## Documentation

| document | what it holds |
|---|---|
| [`docs/TESTING.md`](docs/TESTING.md) | the test profiles — fast, standard, full corpus, stress, soak — and the exact command for each |
| [`docs/XML-CONTRACT.md`](docs/XML-CONTRACT.md) | what may be done to the corpus, what must be preserved, what the corpus measurably contains, and where every kind of XML data has to end up in a PDF |
| [`docs/PDF-ACCEPTANCE.md`](docs/PDF-ACCEPTANCE.md) | the criteria a future converter will be held to, written before it exists |
| [`docs/FRONTEND-CONTRACT.md`](docs/FRONTEND-CONTRACT.md) | the agreed desktop stack, what the Rust core still owes it, and the obstacle of already having a different frontend |
| [`PERFORMANCE.md`](PERFORMANCE.md) | measured numbers, baselines, and the rules for changing them |
| [`cli/fixtures/xml/MANIFEST.md`](cli/fixtures/xml/MANIFEST.md) | every XML fixture, with a SHA-256 and what it is for |

## Performance and reliability

Measured numbers, and the rules for changing them, are in [`PERFORMANCE.md`](PERFORMANCE.md).

## License

MIT — declared in [`cli/Cargo.toml`](cli/Cargo.toml).
