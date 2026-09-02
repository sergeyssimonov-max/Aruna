# Testing

What exists, how to run it, and what each profile is for.

The suite is **415 tests** across seventeen integration binaries plus the library
and the binary's own tests. Since the crates were joined into one workspace on
2026-08-30, `cargo nextest run` from the repository root runs both of them —
**433** as of 2026-09-02, the other eighteen being the desktop shell's — and
`-p aruna` narrows it back to the console crate. It runs in about ten seconds and
needs no network. Two are skipped by design: the core's whole-package round trip,
which is expensive, and the shell's `regenerate_the_bindings`, which is not a
check but the way `frontend/src/bindings.ts` is refreshed.
Beside it, and in a language of its own, are the **63 `vitest` tests** in
`frontend/` — see *Frontend* below. Retries are deliberately absent from
`.config/nextest.toml`: a flaky test is a defect to find, not a wait to sit out.

**A misspelled key in that file is a warning, not an error** — nextest prints
`ignoring unknown configuration key` and carries on with exit 0, so a typo
silently drops the setting it was meant to make. Measured on 2026-08-24; both
profiles were checked and neither emits one. Worth grepping for after editing
the file, because nothing else will tell you.

Nothing under test prints on its own authority. The core reports progress
through `progress::Progress` (`cli/src/progress.rs`), the binary passes
`progress::Stderr`, and every test passes `progress::Silent` — a suite that
printed the parse of each synthetic archive buried its own failures.

---

## Profiles

### Fast — about 12 s

Formatting, compilation, and everything that does not touch the corpus archive.

```sh
cd cli
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo nextest run --profile ci -E 'not binary(corpus)'   # 389
```

### Standard — about 25 s

Everything above plus the corpus tests, both feature configurations, and the
doctests `nextest` does not run.

```sh
cd cli
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --features bench -- -D warnings
cargo nextest run --profile ci
cargo test --doc
```

### Full corpus — about 15 s, needs the 71 MiB archive

Skipped automatically when the archive is absent; set `ARUNA_REQUIRE_FIXTURE=1`
to make its absence a failure, which is what CI does after downloading it.

```sh
cd cli
ARUNA_REQUIRE_FIXTURE=1 cargo nextest run --profile ci -E 'binary(corpus)'   # 3
cargo run --release --example corpus_inventory -- fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip
cargo run --release --example verify_normalization -- fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip
shasum -a 256 fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip
```

Use `ARUNA_ZIP=/path/to.zip` to point at an archive elsewhere.

### Stress — about 20 s

Documents built to break the reader rather than to be read: 50 000 levels of
nesting, 100 000 attributes on one element, an 8 MiB text node, a ZIP bomb that
inflates past the 64 MiB per-document ceiling, a redirect loop, an archive that
names one entry twice, a destination the process cannot write to, and an archive
swapped for a different one between the two passes the build makes over it.

```sh
cd cli
cargo nextest run --profile ci -E 'binary(xml_hostile) + binary(export_hostile)'   # 21
cargo nextest run --profile ci -E 'binary(export_recovery) + binary(cache_concurrency)'  # 11
cargo run --release --example fuzz_naming
cargo run --release --example fuzz_pipeline   # 200 000 documents
cargo run --release --example fuzz_layers     # 300 000 inputs
```

### Soak — minutes, run by hand

Not automated: there is no long-running process to soak. The nearest thing is
repeating the whole export and watching resident memory, which is flat because
the pipeline holds one document at a time.

```sh
cd cli
for i in 1 2 3 4 5; do
  /usr/bin/time -l ./target/release/examples/export_beta \
    fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip 2>&1 | grep -E 'real|maximum resident'
done
```

### A full disk — run by hand, needs the archive

`tests/export_hostile.rs` covers a destination that cannot be written to. It
cannot cover one that runs out of room halfway: no space is a property of the
volume, not of the permissions, and faking it inside a test would test the fake.
A 64 MiB RAM disk against a 389 MB package is the real thing, costs half a
minute, and touches no volume that matters.

```sh
cd cli
MNT=$(mktemp -d)
DEV=$(hdiutil attach -nomount ram://131072 | awk '{print $1}')   # 64 MiB
newfs_hfs -v ArunaFull "$DEV"
diskutil mount -mountPoint "$MNT" "$DEV"

./target/release/examples/export_beta \
  fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip "$MNT"

ls -la "$MNT"          # expect: nothing of ours
diskutil unmount "$MNT" && hdiutil detach "$DEV"
```

Observed: the build stops inside the staging directory and names the file it
could not write —

```
BUILD FAILED: I/O error at …/.TLHdig_Beta_0.3.build/CTH 409/KUB 9.34.xml:
No space left on device (os error 28)
```

— and the staging directory is gone by the time the process exits. The volume is
back to what it was, with only macOS's own `.fseventsd` on it. That is the whole
point of building under `.TLHdig_Beta_0.3.build` and taking the final name last:
a disk that fills leaves the reader the package they already had, or nothing,
never half of one.

### Reproducibility — about 15 s, needs the archive

The next stage depends on it: a converter that maps 23 936 documents to 23 936
PDFs has to put each one where the last run put it, or every re-run rewrites the
whole corpus. Two builds of the same archive, walked and compared byte for byte.

```sh
cd cli
cargo run --release --example determinism -- fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip
```

Observed: 24 601 files each time, 0 present in one build and not the other, 0
with the same path and different bytes. `tests/reliability.rs` holds the same
property against a synthetic archive, so a regression is caught without the
71 MiB.

### Supply chain

```sh
cd cli
cargo audit
cargo deny check
cargo machete
```

### The compiler version

There is one, it is in `rust-toolchain.toml`, and rustup installs it before
anything is built. Neither manifest declares a `rust-version` any more.

They did until 2026-08-25, and the field was doing nothing here: it steers
dependency resolution only under the MSRV-aware resolver, which is the default
from edition 2024 onward, and both crates are edition 2021. Neither is published
to crates.io either, so the promise had no audience — and one of the two was
false. `src-tauri` claimed 1.77.2, a version whose Cargo cannot parse the locked
tree at all, since a dependency needs edition 2024; `darling` and the `icu_*`
crates put the real floor at 1.88. Nothing noticed for months, because nothing
ever compiled with it.

A dependency that needs a newer compiler still says so through its own
`rust-version`, which is exactly how that floor was measured:

```sh
cargo +1.88 check --locked --manifest-path src-tauri/Cargo.toml
```

If either crate is ever published, declare a minimum then — and measure it the
same way rather than writing down a number.

### Frontend — about 2 s, needs `pnpm`

A second suite, in a second language, for the part of the program that is not
Rust: the desktop window, and — since 2026-08-23 — the client script the
exported inventory carries.

```sh
cd frontend
pnpm check          # svelte-check over the app, tsc over the configs and node tests, tsc over the E2E contour
pnpm lint
pnpm format:check
pnpm test:unit      # 57
```

`vitest` runs two projects. **`component`** is jsdom: the 14 tests of
`src/inventory/filter.test.ts`, which drive the search box and the fold controls
against a document built out of the artifacts the crate compiles in —
`document.html` and the row fragments beside it, filled the way `html.rs` fills
them, so the fixture cannot drift from the page — and the 4 tests
of `src/App.test.ts`, which render the window against a mocked `invoke`. The
second file arrived on 2026-08-30 with the screen it tests: before that the
window held a prototype nobody intended to keep, so a jsdom test of its markup
would have been rewritten as often as the markup, and there was none. What it holds is what the screen
promises — both counts, the path they were asked for, the error text in place of
the counts when the command refuses, and the click counter — while
`e2e/smoke.e2e.ts` holds the same button in the real WKWebView, which is the one
thing jsdom cannot answer for.
**`node`** holds the tests that read the repository rather than a DOM:

| test | what it holds |
|---|---|
| `tests/font-stack.test.ts` | the cuneiform stack in `src/inventory/canonical.css` and `src/app.css` name the same faces, in the same order |
| `tests/readme-links.test.ts` | every link in the seven documents resolves |
| `tests/release-version.test.ts` | the release the README calls current is the version `cli/Cargo.toml` declares — CI holds the tag to the manifest, this holds the sentence a reader acts on |
| `tests/one-frontend-stack.test.ts` | React is in no manifest, no lock file, no source file and no artifact — and the stack is Svelte without SvelteKit |
| `tsconfig.e2e.json` | the E2E contour — `e2e/*.e2e.ts` and `wdio.conf.ts` — which no project covered until 2026-08-25. It found both a `wdio.conf.ts` annotated with `Options.Testrunner` (a standalone-session type with no `capabilities` key) and an import of `@wdio/types` that was never declared as a dependency and survived only because it is type-only |
| `tests/spec-guard.test.ts` | the decisions [`PROJECT-SPEC.ru.md`](PROJECT-SPEC.ru.md) fixed — the pnpm pin, the `safari16` floor, the identifier and bundle targets, matching window and document titles, a permission for every registered plugin, and the four gates that keep the E2E contour out of a release |
| `tests/inventory-artifact.test.ts` | everything in `cli/src/generated/` — the script and the three stylesheet sections — is byte-for-byte what `frontend/src/inventory/` now builds, builds the same twice, and carries none of the bundler's leavings |

The last of those is what makes committed build products safe. The script and
the stylesheet sections are built by Vite and compiled into the binary with
`include_str!`, and they are committed rather than produced by `build.rs` for
one reason: **`cargo build` must never need Node.** The whole of `cli/` still builds and tests on a machine
with no `pnpm` on it; what needs Node is the check that the artifact is current.

### Future PDF acceptance

It does not exist. See `PDF-ACCEPTANCE.md` for the criteria it will be held to.
**No placeholder tests were written for it**: an `#[ignore]` that never runs is
not coverage, and a fake converter built to satisfy a test is worse than no
test.

---

## What the tests are grouped into

| binary | tests | what it holds |
|---|---|---|
| library and `bin/aruna` | 270 | parsing, scanning, naming, ordering, the catalogue, MD5, the export's pure halves, the presentation model, the embedded stylesheet, the progress wording, and which failures get advice |
| `tests/integration.rs` | 6 | archive to HTML, malformed input, the corpus if present |
| `tests/cli_process.rs` | 16 | the binary as a child process, cache versus network, and the two words it answers on the command line |
| `tests/cache_lifecycle.rs` | 9 | the cache against a local HTTP server: redirects, loops, failures, and the release advisory |
| `tests/export_integration.rs` | 8 | the export against an archive shaped like the corpus |
| `tests/export_hostile.rs` | 12 | archives written to break the export, and destinations that refuse it |
| `tests/export_recovery.rs` | 7 | building again over what a killed run left behind |
| `tests/package_pages.rs` | 11 | the inventory against the package it describes, and that no CTH folder has a page |
| `tests/cancellation.rs` | 10 | stopping a run, that it leaves the reader's package alone, and that a run stopped half way is followed by a complete one in the same process |
| `tests/cache_concurrency.rs` | 4 | several runs competing for one cache: the race, the sweep, the sockets |
| `tests/catalog_contract.rs` | 12 | the shape of the JSON catalog, held steady now that its former reader is gone |
| `tests/progress_flow.rs` | 6 | which stages a run reports, in what order, with what numbers |
| `tests/reliability.rs` | 4 | two builds byte-identical, no descriptors accumulated, nothing left beside the package |
| `tests/xml_contract.rs` | 9 | the fixture set: immutability, the permit list, field extraction |
| `tests/xml_hostile.rs` | 9 | XXE, entity expansion, external DTD, XInclude, resource exhaustion |
| `tests/authenticity.rs` | 2 | the published package against the archive, as multisets of file contents: nothing lost, invented, altered or written twice. The second is `#[ignore]` and runs the whole corpus — `--run-ignored ignored-only` |
| `tests/window_seams.rs` | 3 | the seams a window will drive: the build on a thread of its own stopped from the caller's, that the library neither prints nor ends the process, and that the destination is the caller's to name |
| `tests/corpus.rs` | 3 | the whole archive: non-distortion, no writes, the malformed count, and that nothing the gates admit comes out of decoding damaged |

Fixtures are described in `cli/fixtures/xml/MANIFEST.md` with a SHA-256 for each.

`tests/support/` is compiled into several of these binaries rather than being a
binary itself: a strict reader for the one JSON document this crate writes, a
local origin that answers more than one client at a time, and the archives the
newer tests are built from.

One binary needs something the crate does not: `corpus` needs the 71 MiB
archive. It skips when the archive is absent, and `ARUNA_REQUIRE_FIXTURE=1`
turns that skip into a failure — which is what a CI job that has just downloaded
the archive should set, so the job cannot go back to passing without doing the
work.

There used to be a second, `catalog_roundtrip`, which needed Node and had
`ARUNA_REQUIRE_NODE=1` for the same purpose. It was removed with the React site
on 2026-08-23: it staged a tree out of `scripts/` and `src/lib/` and read the
result back with the browser's own reader, and all three are gone. No test in
this crate needs Node any more.

---

## Measurement tools

These are examples rather than tests: they measure, and a measurement that fails
a threshold on a busy laptop is a false alarm, not a defect.

```sh
cd cli
cargo run --release --example bench_parse   -- fixtures/…zip   # read, parse, sort, render
cargo run --release --example bench_export  -- fixtures/…zip   # place, inventory, normalise, build
cargo run --release --example bench_digest  -- fixtures/…zip   # MD5 from memory and from disk
cargo run --release --example bench_order   -- fixtures/…zip   # the sort alone
cargo run --release --example bench_fonts   -- fixtures/…zip   # the Unicode block scan
cargo run --release --features bench --example bench_fields -- fixtures/…zip
cargo run --release --example determinism   -- fixtures/…zip   # two builds, compared
cargo run --release --example font_coverage -- fixtures/…zip   # every cmap on the machine
```

`font_coverage` is the one of these that can fail meaningfully rather than
merely be slow, and the only one with a non-zero exit code: it checks that every
face `docs/FONTS.md` specifies is actually installed, and reports any code point
the corpus uses that nothing on the machine can draw.

```
by the declared font stack   642 of 648      ← expected on a correct machine
```

Below 642 means a face is missing and the program names which. That makes it the
check to run when setting up another machine, and the reason it is *not* in the
automated set: its answer depends on what is installed, and a number that is
right here and wrong on the next Mac is not something a test should assert. What
is asserted automatically is the stack itself —
`style.rs::the_font_stack_names_what_the_corpus_needs`.

Baselines are in `PERFORMANCE.md`.

---

## Rules these tests keep

- No test writes to the corpus, to `~/Downloads`, or to any user file. `HOME` is
  overridden for the ones that would.
- No test reaches a production service. The only network is a local server bound
  to port 0, so concurrent runs cannot collide. That was not true until
  2026-08-23: the download path asks Zenodo which edition of the corpus is
  current, so every test driving `obtain_archive` at a local server made a live
  request the local server never saw — ten seconds each on a day the API was
  slow. The lookup is now a parameter (`aruna::ReleaseLookup`), and the tests
  pass `support::obtain_archive`, which answers it instead of asking.
- No `sleep` is used for synchronisation.
- No random seed is unfixed: the fuzz harnesses use a constant seed and print it.
- Temporary directories are removed by `Drop`, including on failure.
- The heavy fixtures are generated inside the test that needs them, not
  committed: their size is the point and the repository is not the place for it.

---

## Environment limits worth knowing

- The `wasm/search` crate was removed with the React site on 2026-08-23, and
  with it the toolchain-pinning quirk that used to be recorded here: `clippy`
  and `rustfmt` had to be invoked from the repository root because that crate
  pinned a minimal toolchain of its own. There is one Rust toolchain in this
  repository again.
- `cargo test --doc` reports 0 tests: the crate has no doc examples.
- No PDF tool (`qpdf`, `pdfinfo`, `pdftotext`, `pdffonts`, `mutool`) is
  installed. Nothing needs one yet.
