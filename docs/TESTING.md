# Testing

What exists, how to run it, and what each profile is for.

The suite is **633 tests** as of 2026-09-25: 594 in the console crate – twenty
integration binaries plus the library and the binary's own tests – and 39 in the
desktop shell. The crates were joined into one workspace on 2026-08-30, so
`cargo nextest run` from the repository root runs both, and `-p aruna` narrows it
back to the console crate. It needs no network, and it runs **626**: seven are
behind `#[ignore]` by design and come in with `--run-ignored all` — three that
read the whole corpus in the core (`authenticity`, `corpus`, `document_model`),
three in the shell that build it or read a package built from it (the two
corpus `cancelling` tests and `markup::every_name_of_a_real_package_reaches_the_window_byte_for_byte`),
and the shell's `regenerate_the_bindings`, which
is not a check but the way `frontend/src/bindings.ts` is refreshed.
Beside it, and in a language of its own, are the **146 `vitest` tests** in
`frontend/` — see *Frontend* below — and the six end-to-end scenarios of
`frontend/e2e/smoke.e2e.ts`, run against the real window by `pnpm test:e2e`. Retries are deliberately absent from
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
cargo nextest run --profile ci -E 'not binary(corpus) and not binary(document_model)'   # 581
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
ARUNA_REQUIRE_FIXTURE=1 cargo nextest run --profile ci -E 'binary(corpus)'   # 5
ARUNA_REQUIRE_FIXTURE=1 cargo nextest run --profile ci -E 'binary(document_model)'   # 5, 30 s, needs xsltproc
cargo nextest run --profile ci -E 'binary(document_model)' --run-ignored ignored-only   # the whole corpus against xsltproc, 67 s
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
cargo nextest run --profile ci -E 'binary(xml_hostile) + binary(export_hostile)'   # 28
cargo nextest run --profile ci -E 'binary(export_recovery) + binary(cache_concurrency)'  # 13
cargo run --release --example fuzz_naming
cargo run --release --example fuzz_pipeline   # 200 000 documents
cargo run --release --example fuzz_layers     # 300 000 inputs
cargo run --release --example fuzz_xml        # 200 000 inputs
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

Observed on 2026-09-18: 23 940 files each time, 0 present in one build and not
the other, 0 with the same path and different bytes. (It read 23 938 from
2026-08-23 until the packaged font and the text of its terms went in on
2026-09-17, and 24 601 before 2026-09-06, from a run made while the package
still carried a page per CTH folder; those went on 2026-08-23.) `tests/reliability.rs` holds the same
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
pnpm test:unit      # 146
```

`vitest` runs two projects. **`component`** is jsdom: the 18 tests of
`src/inventory/filter.test.ts`, which drive the search box and the fold controls
against a document built out of the artifacts the crate compiles in —
`document.html` and the row fragments beside it, filled the way `html.rs` fills
them, so the fixture cannot drift from the page — and the 66 tests
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
| `tests/spec-guard.test.ts` | the decisions [`PROJECT-SPEC.ru.md`](PROJECT-SPEC.ru.md) fixed — the pnpm pin, the `safari16` floor, the identifier and bundle targets, matching window and document titles, the window's permissions compared as a whole list (`core:default` and `dialog:allow-open`, since 2026-09-22) with `{ open }` from the dialog as the page's only plugin import, and the four gates that keep the E2E contour out of a release |
| `tests/cth-title-style.test.ts` | the catalogue's title in a group heading: one line on screen, whole on hover, wrapped on a narrow screen, whole on paper |
| `tests/inventory-artifact.test.ts` | everything in `cli/src/generated/` — the script and the three stylesheet sections — is byte-for-byte what `frontend/src/inventory/` now builds, builds the same twice, and carries none of the bundler's leavings |
| `tests/failure-texts.test.ts` | every failure code `app::Failure::of` can send has a Russian sentence in the window, the failures the shell raises itself are Russian too, and all of them keep the project's typography — the guard that stops a new core code reaching the reader in English |

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
| library and `bin/aruna` | 420 | parsing, scanning, naming, ordering, the catalogue, MD5, the export's pure halves, the presentation model, the embedded stylesheet, the progress wording, and which failures get advice |
| `tests/integration.rs` | 6 | archive to HTML, malformed input, the corpus if present |
| `tests/cli_process.rs` | 20 | the binary as a child process, cache versus network, and the two words it answers on the command line |
| `tests/cache_lifecycle.rs` | 10 | the cache against a local HTTP server: redirects, loops, failures, and the release advisory |
| `tests/export_integration.rs` | 8 | the export against an archive shaped like the corpus |
| `tests/export_hostile.rs` | 18 | archives written to break the export, and destinations that refuse it |
| `tests/export_recovery.rs` | 9 | building again over what a killed run left behind, including a staging directory whose owner is gone |
| `tests/package_pages.rs` | 13 | the inventory against the package it describes, and that no CTH folder has a page |
| `tests/cancellation.rs` | 11 | stopping a run, that it leaves the reader's package alone, and that a run stopped half way is followed by a complete one in the same process |
| `tests/cache_concurrency.rs` | 4 | several runs competing for one cache: the race, the sweep, the sockets |
| `tests/catalog_contract.rs` | 12 | the shape of the JSON catalog, held steady now that its former reader is gone |
| `tests/progress_flow.rs` | 9 | which stages a run reports, in what order, with what numbers |
| `tests/reliability.rs` | 4 | two builds byte-identical, no descriptors accumulated, nothing left beside the package |
| `tests/xml_contract.rs` | 12 | the fixture set: immutability, the permit list, field extraction |
| `tests/xml_hostile.rs` | 10 | XXE, entity expansion, external DTD, XInclude, resource exhaustion — through the export and, since 2026-09-13, through the document model |
| `tests/authenticity.rs` | 3 | the published package against the archive, as multisets of file contents: nothing lost, invented, altered or written twice; and, since 2026-09-22, that the manifest names the entry the content gate turned away. The whole-corpus one is `#[ignore]` — `--run-ignored ignored-only` — and holds that entry to exactly `KUB 37.25.xml` |
| `tests/window_seams.rs` | 8 | the seams a window will drive: the build on a thread of its own stopped from the caller's, that the library neither prints nor ends the process, and that the destination is the caller's to name |
| `tests/corpus.rs` | 6 | the whole archive: non-distortion, no writes, the malformed count, and that nothing the gates admit comes out of decoding damaged |
| `tests/document_model.rs` | 6 | the document model against `xsltproc`, node for node: the valid fixtures, a 52-document sample of the archive, and the whole corpus behind `#[ignore]`; the whole corpus read twice and refused exactly where the manifest says; and, since 2026-09-25, UTF-16 in both byte orders, Latin-1 bytes and a prolog cut in three places refused, never read and never a panic |
| `tests/fonts.rs` | 4 | the one font this repository carries, held to the bytes it arrived as, the terms beside it, and that no source file reaches for a system font directory |
| `tests/pdf_acceptance.rs` | 1 | the instruments a PDF will be held to, run before there is a PDF: `sips` turns a hand-written one-page PDF into pixels |

Counted on 2026-09-25 with `cargo nextest list --run-ignored all`: 594 in the `aruna` crate, as above, and 39 in `aruna-desktop`, which is 633. Without `--run-ignored` the run is 626: the seven heavy ones stay behind `#[ignore]` and need the archive. The thirty-three since 2026-09-24 came with the polish block for 2.6.1, `eef9f4e` to `ed8f48d` – a console that survives a closed stream and refuses in Russian, the published copy checked before the reader's goes, the proxy from the environment, the publish lock on `flock` and its race, a link under the package's name, long names cut to fit and names or group folders the disk takes for one, a cancel from the window on a small archive, the manifest's names reaching the window, and the model refusing what is not its to read; 600 and 594 before them. The nine since 2026-09-23 came with `ede953a` – a named pipe where the publish lock or a staging marker belongs, the window's font-error sentences, the model's boundary guard, and the new `pdf_acceptance.rs`; 591 and 585 before them. The previous count, 555 and 549 on 2026-09-19, had gone stale by the acceptance audit of 21.09, which ran 560 and 554; since then came the catalogue's titles in the group headings, the package-name boundary of the shell's reading commands, and the manifest's list of entries that are not manuscripts; on 2026-09-23 three download tests (the read timeout in force, a cancel reaching a silent server, a cut body reported as the network's) and two for the declared XML version, 586 and 580 before them.

## Coverage floors

Since 2026-09-22 coverage has floors, and a run under one fails.

```sh
pnpm coverage                      # root: scripts/coverage.sh, cargo llvm-cov per crate
cd frontend && pnpm test:coverage  # Vitest with thresholds
```

| part | measured 2026-09-22 | floor |
|---|---|---|
| core, `-p aruna` | regions 95.91 %, functions 97.13 %, lines 95.61 % | 95 / 96 / 95 |
| shell, `-p aruna-desktop` | regions 65.39 %, functions 62.63 %, lines 66.52 % | 64 / 62 / 65 |
| frontend | statements 82.48 %, branches 75.22 %, functions 59.57 %, lines 81.03 % | 81 / 74 / 58 / 80 |

The floors bind the ordinary run, **without** the heavy `#[ignore]` tests: that
run needs no archive and gives the same numbers on any machine, while the heavy
one lifted the shell to 79.95 % of regions when last measured, on 2026-09-19. Each crate is measured alone, so
the core's 18 767 regions cannot hide the shell's 1 303. The floors are a
ratchet, not a target: raise them when the numbers rise, and a change that falls
under one says why in its commit rather than lowering it.

Measured again on 2026-09-25, after `cargo llvm-cov clean --workspace`: core
95.80 / 97.00 / 95.67 %, shell 76.14 / 73.17 / 75.91 % (regions / functions /
lines).

**A heavy test in the shell costs coverage it does not earn – a trap met on
2026-09-24 and defused the same day.** The body of a test under `#[ignore]` in
`src-tauri/src/lib.rs` is counted in the shell's denominator, and the ordinary
`pnpm coverage` never runs it. On 2026-09-24 the shell stood 0.63 points over
its functions floor (62.63 against 62), and the reliability run of that day
measured what one such test of about a hundred lines does: 60.13 / 56.36 /
61.81, under all three floors. The probe it wanted to add was kept as a patch
for that reason. `110f3fc` then proved the cancel from the window in the
ordinary suite, on an archive the test builds itself, and lifted the shell to
73.74 / 70.48 / 74.23; the probe landed with `9b15172` as an ordinary test with
a heavy twin. **The rule stays:** a heavy shell test comes with an ordinary
one that runs its code, or with a decision on how it enters the coverage run –
not alone.

Fixtures are described in `cli/fixtures/xml/MANIFEST.md` with a SHA-256 for each.

`tests/support/` is compiled into several of these binaries rather than being a
binary itself: a strict reader for the one JSON document this crate writes, a
local origin that answers more than one client at a time, and the archives the
newer tests are built from.

Two binaries need something the crate does not: `corpus` and `document_model`
need the 71 MiB archive, and `document_model` also needs `xsltproc`, which ships
with macOS and is a package on Linux. They skip when either is absent, and
`ARUNA_REQUIRE_FIXTURE=1` turns that skip into a failure — which is what a CI job that has just downloaded
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
BY THE REPOSITORY’S FILES     639 of 645      ← expected on a correct machine
```

Below 639 means a face is missing and the program names which. That makes it the
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
  installed. Nothing needs one yet, and `docs/PDF-ACCEPTANCE.md` stopped naming
  them on 2026-09-08: with Homebrew unused and a Java runtime barred, a
  requirement whose only instrument cannot be had is a requirement nobody
  checks.
- `xmllint` and `xsltproc` are in `/usr/bin`, signed `com.apple.*`, and are the
  two macOS programs `docs/PDF-ACCEPTANCE.md` names for the XML side of that
  acceptance — canonical comparison and an independent extraction. `sips`, for
  rasterising a page, is the third. All three ship with the system, and all
  three are for examples and tests; nothing on the working path calls any of
  them. The first two run on the same libxml2 2.9.13, so together they are one
  opinion, not two (`docs/PDF-ACCEPTANCE.md`, measured 2026-09-22).
