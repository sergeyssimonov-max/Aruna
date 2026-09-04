# Aruna / TLHdig Inventory

Production toolkit for [TLHdig Beta 0.3](https://zenodo.org/records/20328284) (Hittite cuneiform transliterations).

**What exists today is a command-line program.** It downloads the corpus from
Zenodo and writes it out as a folder of normalised documents with an inventory
over them, in `~/Downloads`. That program is [`cli/`](cli/), it is what the
releases ship, and it is the whole of the product.

**A desktop application is being built and is not finished.**
[`frontend/`](frontend/) and [`src-tauri/`](src-tauri/) hold the stack it is
built on — the toolchain is installed and pinned, and the frontend half is
checked and built by CI; `src-tauri` is not, and is kept buildable by the
pre-commit battery and by CI on macOS. Since 2026-09-02 the window builds the
corpus itself: it calls the same `app::build_corpus` the console binary calls,
shows the run as it goes, can stop it, and ends on the report of that run. What
is not settled is what a reader is given — the image is still built from the
core (`docs/PROJECT-SPEC.ru.md` §7.3). Read those two directories as work in
progress, not as a second program you can run.

## Layout

| Path | Description |
|------|-------------|
| [`cli/`](cli/) | **Aruna** — Rust CLI: download Zenodo ZIP → parse XML → a folder of documents with an HTML inventory over them. Builds on macOS and Linux; the `.app` and DMG are macOS-only |
| [`frontend/`](frontend/) | **The window** — Svelte 5, Vite, TypeScript, no SvelteKit. One screen with seven states: what is on disk, an offer to build from Zenodo or from an archive you pick, the stage and the fraction while it builds, and the report of that run with the inventory a click away. Its types are generated from the Rust commands into `src/bindings.ts` |
| [`src-tauri/`](src-tauri/) | **The Tauri 2 shell** that hosts it. Two commands: `corpus_location` says where the package is, `corpus_stats` counts what is in it — both answered by the `cli` crate's own paths and its own manifest |
| [`cli/examples/emit_inventory_json.rs`](cli/examples/emit_inventory_json.rs) | Emit the catalog as JSON from the archive |

There was a React web application here until 2026-08-23, served from `src/` with a WASM search module under `wasm/search/`. It has been removed along with the ARUN container built for it; the desktop application is to replace it, once it exists. [`docs/FRONTEND-CONTRACT.md`](docs/FRONTEND-CONTRACT.md) records what went and why.

## CLI (Aruna)

Builds on macOS 13+ and Linux. The tests run in CI on Linux; the macOS jobs check the desktop crate and build the `.app` and the DMG, and run no CLI tests — the CLI's own suite is the Ubuntu one. Windows is untried. Only the `.app` bundle and the DMG are macOS-only.

```bash
cargo build --release --locked -p aruna   # any supported platform, from the repository root
./target/release/aruna                    # → ~/Downloads/TLHdig_Beta_0.3/ — open TLHdig_Beta_0.3.html inside it
```

The path is the workspace's, not the crate's: since the two crates were joined
on 2026-08-30 `cargo` writes to `target/` at the root whichever directory it is
invoked from.

The first run downloads 71 MiB from Zenodo and keeps it in the OS cache directory, so later runs take about two seconds and need no network. `ARUNA_ZIP=/path/to.zip` uses a local archive instead. Details in [`cli/README.md`](cli/README.md).

Packaging, on macOS 13+ only:

```bash
pnpm build                     # → target/universal-apple-darwin/release/bundle/{macos/Aruna.app,dmg/*.dmg}
```

That is `tauri build --target universal-apple-darwin`: it builds the frontend,
packs it into `Aruna.app` for both architectures and writes the DMG beside it.
The console binary above is unaffected — `cargo build -p aruna` still produces
it, and that is still what a developer runs.

Prebuilt DMG: [Releases](https://github.com/sergeyssimonov-max/Aruna/releases) (Universal Binary).

### What the DMG contains, and what double-clicking it does

`Aruna.app` is an application with a window. Double-clicked from Finder, opened
from Launchpad or started with `open Aruna.app`, it comes up 800 by 600 in the
middle of the screen and says what is on disk: whether the package is already
built, how large it is and how uneven the corpus is. From there it builds the
corpus itself — from Zenodo or from an archive you pick — showing the stage and
the fraction while it works, and it can be stopped mid-run. When it finishes it
reports that run and offers the inventory it wrote.

Until 2026-09-04 what this DMG carried was the console program in an application
bundle: no window, no progress, no dialogs, and a double click that ran to
completion and said nothing — on a good run and on a bad one alike, because
everything it printed went to a terminal Finder never gave it. That is what the
window replaces.

**The console form has not gone anywhere.** It is the same core, and it is still
the shorter path when you want the package and not a screen:

```bash
cargo build --release -p aruna && ./target/release/aruna
```

### If a run is killed

Ctrl-C stops the console form the way the terminal stops any program: there is
no signal handler. The machinery for a clean stop exists and is exercised — work
checks a cancellation flag between documents — and the window is what presses
it: its «Остановить» reaches the same flag, and a run stopped that way publishes
nothing and leaves nothing behind. From a terminal, Ctrl-C is still the blunt
version.

What a killed run leaves in `~/Downloads` is one hidden directory,
`.TLHdig_Beta_0.3.build.<pid>.<n>`: its unfinished package. Nothing is published
from it and the next run is not blocked by it — that one builds beside it under
its own name — so it is safe to delete with `rm -rf
~/Downloads/.TLHdig_Beta_0.3.build.*`. A finished package is never at risk from
this: the previous one is moved aside only after the new one is written whole
and checked.

### Gatekeeper

**Read this before the first launch, because there will be nothing else to
read.** The `.app` is signed ad hoc: enough for macOS to run it locally, not a
Developer ID, and it is not notarized — this project has no paid Apple
membership. macOS will therefore refuse it on first open, with *"Aruna cannot
be opened because the developer cannot be verified"* or *"is damaged and can't
be opened"*, the second being what a quarantined download usually produces.

What that refusal costs changed on 2026-09-04, and it is worse than it was.
While this bundle held the console program, a blocked launch was one more
silence among others — the program said nothing from Finder anyway. Now the
application has a window, and a person who double-clicks it expects one: the
refusal is the whole of what they get, and nothing appears at all. So the steps
below are not a footnote.

**Check what you are letting through before you let it through.** Every release
publishes the digest of its DMG, and `shasum -a 256 -c SHA256SUMS` beside the
downloaded file checks it. The steps below hand an unverified binary the
system's benefit of the doubt; do them after the digest matches, not before.

Then open it once through the exception, or clear the quarantine flag:

```bash
xattr -dr com.apple.quarantine /Applications/Aruna.app   # then run it normally
```

`-r` because the flag is set on files inside the bundle as well as on the
bundle itself, and clearing only the top one leaves the copy that stops it
opening. Point it wherever the `.app` actually is — `/Applications` is where
this project suggests putting it, not where macOS requires it.

Finder's route to the same exception: right-click the app → **Open** → **Open**
in the dialog. On Ventura and later a blocked app can also be allowed under
**System Settings → Privacy & Security**, where an *"Open Anyway"* button
appears after the first refusal.

## Desktop application — a status window, not yet a product

**What exists is one screen, and it does the work.** The stack under it is
wired end to end and kept green: `pnpm dev` opens a window, and since
2026-09-02 that window is the program's front door rather than a view of its
output. It reads what is on disk — how many manuscripts, how many CTH groups,
how unevenly the one is spread across the other, and what the manifest counted
about the corpus's writing — and it builds: `build_corpus` runs the core off the
main thread, `build-progress` events carry the stage and the fraction, and
`cancel_build` stops a run that is not wanted. The report it ends on is the
run's own, not a second count taken afterwards.
The screen was drawn through `/design` once and rebuilt by hand in `App.svelte`
and `app.css`. It is not redrawn on a schedule: what it carries — the four
commands, the states it can be in, and the counts — is a contract, written out
in [`docs/FRONTEND-CONTRACT.md`](docs/FRONTEND-CONTRACT.md) §3, and replacing it
is a decision rather than a routine. The design tool is still checked against
this stack, on a mockup built beside the repository and never carried into it.

What the window still cannot do is be the thing a reader installs: the image is
built from the core, and whether that changes is an open position in the
specification rather than an oversight.
[`docs/FRONTEND-CONTRACT.md`](docs/FRONTEND-CONTRACT.md) records the contract
between the two halves. The commands below build and check the shell; since
2026-08-29 a `desktop` job on macOS runs the same battery in CI, so a break
surfaces on the push that caused it rather than on the next person to run them
by hand.

pnpm only — the version is pinned by `packageManager` in [`package.json`](package.json), so `corepack` hands every machine the same one.

```bash
pnpm --dir frontend install
pnpm dev             # Tauri window, frontend on Vite's dev server
pnpm build           # → Universal .app + DMG (macOS, both architectures)
pnpm build:host      # → .app + DMG for this machine's architecture only
pnpm check           # svelte-check + tsc over all three tsconfigs
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

Four jobs on every push. Three on Ubuntu: tests and clippy for the CLI; `svelte-check`, vitest and a production build for the frontend; and a full parse of all 23 936 manuscripts against the real archive. The fourth is on macOS — formatting, clippy and tests for `src-tauri`, because a Tauri crate on Linux needs GTK and WebKitGTK to compile at all, and checking it there would say nothing about the one platform this ships on. Since 2026-08-31 that fourth job also gates the release: the DMG is built from the console crate and carries no shell code, but a release is not cut from a tree whose desktop crate is red — the shell consumes the core's public contract from outside, and it is the only job that does.

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

**[v2.5.2](https://github.com/sergeyssimonov-max/Aruna/releases/latest) is the
current release — the one to download.** It is what `Releases` marks *Latest*,
and it is the only version this project asks anyone to install.

**Three releases are published, and no others.** Two are kept as **references**:
states this project measures itself against and can fall back to when a fault
has to be bracketed in time. They are baselines for the people working on it,
not versions to run — a reference is by definition behind. The third is the
release above, the one to install.

[v1.0.5](https://github.com/sergeyssimonov-max/Aruna/releases/tag/v1.0.5) is the floor: the first release of the numbering that survives, and the oldest state still known to be good.

[v1.0.9](https://github.com/sergeyssimonov-max/Aruna/releases/tag/v1.0.9) closes the 1.x line: it credits the corpus authors and bounds a download that had nothing but the disk to stop it.

[v2.5.2](https://github.com/sergeyssimonov-max/Aruna/releases/tag/v2.5.2) is the third: the release named at the top of this section.

**What that costs, said plainly.** v2.1.0, v2.2.0, v2.3.0 and v2.4.0 were
withdrawn on 2026-08-30 — tags and DMGs both — and v2.5.0 followed on
2026-09-01, when v2.5.1 replaced it as the third reference, and v2.5.1 itself on
2026-09-04, when v2.5.2 replaced it in turn — the release that carries the
window as the thing a reader installs. A reference cannot
be re-cut, so work that lands after one takes a number of its own; keeping the
list at three then means retiring the release it replaces rather than letting
both stand. Until 2026-08-30 the three references were deliberately spread
across the project, and v2.1.0 was what bracketed a fault introduced recently
from below; nothing between v1.0.9 and v2.5.2 does that now. The floor still holds, and the history of what changed when is in the
commits, which were not touched. The next reference worth adding is the first
2.x state after this one that is worth falling back to.

All three are recorded in [`.github/reference-release.json`](.github/reference-release.json) with the commit they point at and the digest of the DMG published from them, and CI fails if any tag disappears or moves to a different commit. A ruleset could stop a tag being deleted; it could not say which commit the tag was supposed to point at.

## Documentation

| document | what it holds |
|---|---|
| [`docs/TESTING.md`](docs/TESTING.md) | the test profiles — fast, standard, full corpus, stress, soak — and the exact command for each |
| [`docs/XML-CONTRACT.md`](docs/XML-CONTRACT.md) | what may be done to the corpus, what must be preserved, what the corpus measurably contains, and where every kind of XML data has to end up in a PDF |
| [`docs/PDF-ACCEPTANCE.md`](docs/PDF-ACCEPTANCE.md) | the criteria a future converter will be held to, written before it exists |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | the shape of the Rust side: the layers, which way they depend, and where a Tauri adapter and a PDF renderer attach |
| [`docs/FRONTEND-CONTRACT.md`](docs/FRONTEND-CONTRACT.md) | the agreed desktop stack, what the Rust core still owes it, and what the removal of the React application took with it |
| [`docs/PROJECT-SPEC.ru.md`](docs/PROJECT-SPEC.ru.md) | the normative specification of the desktop stack — every component pinned, with a status, and the rules a change is accepted under (in Russian) |
| [`PERFORMANCE.md`](PERFORMANCE.md) | measured numbers, baselines, and the rules for changing them |
| [`cli/fixtures/xml/MANIFEST.md`](cli/fixtures/xml/MANIFEST.md) | every XML fixture, with a SHA-256 and what it is for |

## Performance and reliability

Measured numbers, and the rules for changing them, are in [`PERFORMANCE.md`](PERFORMANCE.md).

## License

MIT — [`LICENSE`](LICENSE), and declared in [`cli/Cargo.toml`](cli/Cargo.toml) and [`src-tauri/Cargo.toml`](src-tauri/Cargo.toml).
