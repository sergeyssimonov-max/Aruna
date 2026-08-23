# Aruna / TLHdig Inventory

Production toolkit for [TLHdig Beta 0.3](https://zenodo.org/records/20328284) (Hittite cuneiform transliterations).

Two programs share one parser. The CLI downloads the corpus from Zenodo and writes a standalone HTML inventory; the desktop application opens the same corpus in a window. The parser in [`cli/`](cli/) is the source of truth for both.

## Layout

| Path | Description |
|------|-------------|
| [`cli/`](cli/) | **Aruna** — Rust CLI: download Zenodo ZIP → parse XML → HTML inventory. Builds on macOS and Linux; the `.app` and DMG are macOS-only |
| [`frontend/`](frontend/) | Desktop UI — Svelte 5, Vite, TypeScript, no SvelteKit |
| [`src-tauri/`](src-tauri/) | The Tauri 2 shell the UI runs in, and the Rust side it talks to |
| [`cli/examples/emit_inventory_json.rs`](cli/examples/emit_inventory_json.rs) | Emit the catalog as JSON from the archive |

There was a React web application here until 2026-08-23, served from `src/` with a WASM search module under `wasm/search/`. It has been removed along with the ARUN container built for it; the desktop application replaces it. [`docs/FRONTEND-CONTRACT.md`](docs/FRONTEND-CONTRACT.md) records what went and why.

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

## Desktop application

pnpm only — the version is pinned by `packageManager` in [`package.json`](package.json), so `corepack` hands every machine the same one.

```bash
pnpm --dir frontend install
pnpm dev             # Tauri window, frontend on Vite's dev server
pnpm build           # → Universal .app + DMG (macOS, both architectures)
pnpm build:host      # → .app + DMG for this machine's architecture only
pnpm check           # svelte-check + tsc over both tsconfigs
pnpm test            # vitest — including the font-stack agreement check
```

`pnpm build` targets `universal-apple-darwin`, so the bundle it produces runs on
Intel and Apple Silicon alike — the same guarantee the CLI's DMG has always
carried. It needs both Rust targets installed (`rustup target add
x86_64-apple-darwin aarch64-apple-darwin`) and, being a macOS target, it is
macOS-only; `build:host` is the escape hatch anywhere else.

`pnpm dev` and `pnpm build` drive the frontend themselves: `beforeDevCommand` and `beforeBuildCommand` in [`src-tauri/tauri.conf.json`](src-tauri/tauri.conf.json) run it, and `frontendDist` points at what `vite build` leaves in `frontend/dist`.

## CI

[`.github/workflows/release-dmg.yml`](.github/workflows/release-dmg.yml) · [`cli/docs/AUTO_DMG.md`](cli/docs/AUTO_DMG.md)

Three jobs on every push, all on Ubuntu: tests and clippy for the CLI; `svelte-check`, vitest and a production build for the frontend; and a full parse of all 23 936 manuscripts against the real archive.

The Universal `.app` and DMG are built on macOS, but only when a release is being cut or when the workflow is run by hand — macOS minutes bill at ten times the rate, and nothing but a release consumes that artifact. Run it by hand before tagging, since a break in the packaging script no longer surfaces on the push that caused it:

```bash
gh workflow run release-dmg.yml --ref main
```

A tag builds it and publishes the release:

```bash
git tag v2.0.1 && git push origin v2.0.1   # → GitHub Release with DMG
```

The tag has to match `version` in [`cli/Cargo.toml`](cli/Cargo.toml), and CI refuses the release if it does not: the `.app` takes its `CFBundleShortVersionString` from that manifest, so a tag and a manifest that disagree publish a DMG whose application reports a different version from the release it is attached to.

The corpus job is the one that runs the parser against the real 71 MiB archive rather than fixtures, which is where the character-boundary panic on actual cuneiform was caught. It gates the release.

## Releases

[v2.1.0](https://github.com/sergeyssimonov-max/Aruna/releases/tag/v2.1.0) is current, and is also the third of the releases this project keeps as references — states it measures itself against and can fall back to. Everything else has been withdrawn along with its tag and its DMG.

[v1.0.5](https://github.com/sergeyssimonov-max/Aruna/releases/tag/v1.0.5) is the floor: the first release of the numbering that survives, and the oldest state still known to be good.

[v1.0.9](https://github.com/sergeyssimonov-max/Aruna/releases/tag/v1.0.9) closes the 1.x line: it credits the corpus authors and bounds a download that had nothing but the disk to stop it.

[v2.1.0](https://github.com/sergeyssimonov-max/Aruna/releases/tag/v2.1.0) carries the 2.x one — the corpus as a folder that can be opened, the CTH pages given up, and the inventory's own script and stylesheet built by the frontend stack rather than written by hand.

Three, spread across the project rather than clustered at its end, which is what makes them useful: a fault introduced this week is bracketed by v2.1.0, and one that turns out to be much older still has a floor under it.

All three are recorded in [`.github/reference-release.json`](.github/reference-release.json) with the commit they point at and the digest of the DMG published from them, and CI fails if any tag disappears or moves to a different commit. A ruleset could stop a tag being deleted; it could not say which commit the tag was supposed to point at.

## Documentation

| document | what it holds |
|---|---|
| [`docs/TESTING.md`](docs/TESTING.md) | the test profiles — fast, standard, full corpus, stress, soak — and the exact command for each |
| [`docs/XML-CONTRACT.md`](docs/XML-CONTRACT.md) | what may be done to the corpus, what must be preserved, what the corpus measurably contains, and where every kind of XML data has to end up in a PDF |
| [`docs/PDF-ACCEPTANCE.md`](docs/PDF-ACCEPTANCE.md) | the criteria a future converter will be held to, written before it exists |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | the shape of the Rust side: the layers, which way they depend, and where a Tauri adapter and a PDF renderer attach |
| [`docs/FRONTEND-CONTRACT.md`](docs/FRONTEND-CONTRACT.md) | the agreed desktop stack, what the Rust core still owes it, and what the removal of the React application took with it |
| [`PERFORMANCE.md`](PERFORMANCE.md) | measured numbers, baselines, and the rules for changing them |
| [`cli/fixtures/xml/MANIFEST.md`](cli/fixtures/xml/MANIFEST.md) | every XML fixture, with a SHA-256 and what it is for |

## Performance and reliability

Measured numbers, and the rules for changing them, are in [`PERFORMANCE.md`](PERFORMANCE.md).

## License

MIT — [`LICENSE`](LICENSE), and declared in [`cli/Cargo.toml`](cli/Cargo.toml) and [`src-tauri/Cargo.toml`](src-tauri/Cargo.toml).
