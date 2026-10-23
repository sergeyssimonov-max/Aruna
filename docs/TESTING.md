# Testing

What exists, how to run it, and what each profile is for.

The suite is **760 tests** as of 2026-10-24, counted after the task «tls-trust»: 714 in the
console crate – 25 integration binaries plus the library and the binary's own
tests – and 46 in the desktop shell. The crates were joined into one workspace on 2026-08-30, so
`cargo nextest run` from the repository root runs both, and `-p aruna` narrows it
back to the console crate. It needs no network, and it runs **750**: ten are
behind `#[ignore]` by design and come in with `--run-ignored all` — five that
read the whole corpus in the core (two in `authenticity`, one each in `corpus`,
`document_model` and `pdf_module`), four in the shell that build it or read a
package built from it (the three corpus `cancelling` tests and
`markup::every_name_of_a_real_package_reaches_the_window_byte_for_byte`),
and the shell's `regenerate_the_bindings`, which
is not a check but the way `frontend/src/bindings.ts` is refreshed.
Beside it, and in a language of its own, are the **261 `vitest` tests** in
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
cargo nextest run --profile ci -E 'not binary(corpus) and not binary(document_model)'   # 677
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
pnpm test:unit      # 261
```

`vitest` runs two projects, 18 files. **`component`** is jsdom: the 45 tests of
`src/inventory/filter.test.ts`, which drive the search box and the fold controls
against a document built out of the artifacts the crate compiles in —
`document.html` and the row fragments beside it, filled the way `html.rs` fills
them, so the fixture cannot drift from the page — and the 80 tests
of `src/App.test.ts`, which render the window against a mocked `invoke`. The
second file arrived on 2026-08-30 with the screen it tests: before that the
window held a prototype nobody intended to keep, so a jsdom test of its markup
would have been rewritten as often as the markup, and there was none. What it holds is what the screen
promises — both counts, the path they were asked for, the error text in place of
the counts when the command refuses, and the click counter — while
`e2e/smoke.e2e.ts` holds the same button in the real WKWebView, which is the one
thing jsdom cannot answer for.
**`node`** holds the tests that read the repository rather than a DOM, 136 in
16 files (counted on 2026-10-20 at `87e1b28`; 82 in 15 before the three documentation tests of readme-ru):

| test | what it holds |
|---|---|
| `tests/font-stack.test.ts` | the cuneiform stack in `src/inventory/canonical.css` and `src/app.css` name the same faces, in the same order |
| `tests/readme-links.test.ts` | every link in the nine documents resolves – the seven of before and, since 2026-10-20, `cli/releases/README.md` and `frontend/README.md` |
| `tests/release-version.test.ts` | the release the README calls current is the version `cli/Cargo.toml` declares — CI holds the tag to the manifest, this holds the sentence a reader acts on; the paragraph is found by its link to `/releases/latest`, not by its words, since 2026-10-20 |
| `tests/docs-typography.test.ts` | the four README files and every `.github/release-notes/vX.Y.Z.md` keep the Russian typography of CLAUDE.md outside code – no em dash, no `ё`, no comma before a dash, a non-breaking space before `%` and after `№` – and are Cyrillic at all; every reference release has its page file |
| `tests/one-frontend-stack.test.ts` | React is in no manifest, no lock file, no source file and no artifact — and the stack is Svelte without SvelteKit |
| `tsconfig.e2e.json` | the E2E contour — `e2e/*.e2e.ts` and `wdio.conf.ts` — which no project covered until 2026-08-25. It found both a `wdio.conf.ts` annotated with `Options.Testrunner` (a standalone-session type with no `capabilities` key) and an import of `@wdio/types` that was never declared as a dependency and survived only because it is type-only |
| `tests/spec-guard.test.ts` | the decisions [`PROJECT-SPEC.ru.md`](PROJECT-SPEC.ru.md) fixed — the pnpm pin, the `safari16` floor, the identifier and bundle targets, matching window and document titles, the window's permissions compared as a whole list (`core:default` and `dialog:allow-open`, since 2026-09-22) with `{ open }` from the dialog as the page's only plugin import, and the four gates that keep the E2E contour out of a release |
| `tests/cth-title-style.test.ts` | the catalogue's title in a group heading: one line on screen, whole on hover, wrapped on a narrow screen, whole on paper |
| `tests/inventory-artifact.test.ts` | everything in `cli/src/generated/` — the script and the three stylesheet sections — is byte-for-byte what `frontend/src/inventory/` now builds, builds the same twice, and carries none of the bundler's leavings |
| `tests/failure-texts.test.ts` | every failure code `app::Failure::of` can send has a Russian sentence in the window, the failures the shell raises itself are Russian too, and all of them keep the project's typography — the guard that stops a new core code reaching the reader in English |
| `tests/ipc-shape.test.ts` | the sample `src-tauri/stats-sample.json` is what Rust serialises, and empty places arrive as `null` rather than as missing fields |
| `tests/column-order.test.ts` | the column the narrow screen hides by its number in `screen.css` is the editor's column of `COLUMNS` in `cli/src/html.rs` |
| `tests/audit-exception.test.ts` | the one `pnpm audit` exception, GHSA-vfj7-8cjw-p6xm, still covers braces 3.0.3 reached by its one chain, and nothing else |
| `tests/third-party-notices.test.ts` | `THIRD-PARTY-NOTICES.md` names every package of the three lock files once, and nothing else |
| `tests/reference-images.test.ts` | the gate of tree cleanliness on the reference images in `cli/releases/`, run on inputs of its own |
| `tests/e2e-isolation.test.ts` | the E2E contour's guard against the real `~/Library`, shown on a `Library` and a temporary directory of its own |
| `tests/loopback-record.test.ts` | the tests that go to loopback are the ones *Tests that go to loopback* lists, with its numbers |

The last of those is what makes committed build products safe. The script and
the stylesheet sections are built by Vite and compiled into the binary with
`include_str!`, and they are committed rather than produced by `build.rs` for
one reason: **`cargo build` must never need Node.** The whole of `cli/` still builds and tests on a machine
with no `pnpm` on it; what needs Node is the check that the artifact is current.

### ~~Future~~ PDF acceptance

~~It does not exist. See `PDF-ACCEPTANCE.md` for the criteria it will be held to.~~
*Corrected 2026-10-19:* the converter exists since 2026-09-30 and every build
runs it since 2026-10-18. Its acceptance is `tests/pdf_module.rs` and
`tests/export_pdf.rs` below, the instruments of `tests/pdf_acceptance.rs`, and
the release gate's example `pdf_gate`, which checks every criterion of
`PDF-ACCEPTANCE.md` on every PDF of the package (`PROJECT-SPEC.ru.md` 6.8).
**No placeholder tests were written for it** before it existed: an `#[ignore]` that never runs is
not coverage, and a fake converter built to satisfy a test is worse than no
test.

---

## What the tests are grouped into

| binary | tests | what it holds |
|---|---|---|
| library and `bin/aruna` | 504 | parsing, scanning, naming, ordering, the catalogue, MD5, the export's pure halves, the presentation model, the embedded stylesheet, the progress wording, and which failures get advice |
| `tests/integration.rs` | 6 | archive to HTML, malformed input, the corpus if present |
| `tests/cli_process.rs` | 23 | the binary as a child process, cache versus network, and the two words it answers on the command line |
| `tests/cache_lifecycle.rs` | 10 | the cache against a local HTTP server: redirects, loops, failures, and the release advisory |
| `tests/export_integration.rs` | 11 | the export against an archive shaped like the corpus |
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
| `tests/authenticity.rs` | 4 | the published package against the archive, as multisets of file contents: nothing lost, invented, altered or written twice; and, since 2026-09-22, that the manifest names the entry the content gate turned away. The whole-corpus one is `#[ignore]` — `--run-ignored ignored-only` — and holds that entry to exactly `KUB 37.25.xml`; and, since 2026-10-13, behind `#[ignore]` as well, that each of the 23 936 documents names in `source_entry` an entry of the archive, byte for byte, whose bytes give its `source_sha256`, and no two name one |
| `tests/window_seams.rs` | 8 | the seams a window will drive: the build on a thread of its own stopped from the caller's, that the library neither prints nor ends the process, and that the destination is the caller's to name |
| `tests/corpus.rs` | 6 | the whole archive: non-distortion, no writes, the malformed count, and that nothing the gates admit comes out of decoding damaged |
| `tests/document_model.rs` | 6 | the document model against `xsltproc`, node for node: the valid fixtures, a 52-document sample of the archive, and the whole corpus behind `#[ignore]`; the whole corpus read twice and refused exactly where the manifest says; and, since 2026-09-25, UTF-16 in both byte orders, Latin-1 bytes and a prolog cut in three places refused, never read and never a panic |
| `tests/fonts.rs` | 4 | the one font this repository carries, held to the bytes it arrived as, the terms beside it, and that no source file reaches for a system font directory |
| `tests/pdf_acceptance.rs` | 2 | the instruments a PDF is held to, written before there was a PDF: `sips` turns a hand-written one-page PDF into pixels |
| `tests/quick_xml_guard.rs` | 3 | the guard of the `quick-xml` 0.38.4 exception: no template names a bibliography or a way to read files, no source names the citation crates, and markup in a document is set as text by `aruna::pdf` with only the template and the data asked for |
| `tests/pdf_module.rs` | 11 | the PDF of one document on synthetic samples – a label, a repeated cluster, the credit, `text:tab`, a comment – each put to every check of the gate; no date and the same bytes twice; the tag tree against the model with its four negative controls; snapshots of structure (insta); the reader's teeth; and the same constructs on six corpus documents behind `#[ignore]` |
| `tests/export_pdf.rs` | 8 | the PDF phase of the export: a PDF in the folder of every document but the refused one, and the package naming each – the manifest by path or reason, the inventory by one link on the name, at the PDF or at the XML – a broken invariant stops the build, a run stopped in the phase publishes nothing, two builds are one package, the tick ends on the whole |
| `tests/verify_two_declarations.rs` | 1 | what `verify::compare` answers for a source with two `<?xml?>` declarations, held as it is today so a restructuring cannot change it unnoticed |
| `tests/tls_trust_guard.rs` | 5 | the guard of TLS trust, without a date, since 2026-10-24: no exception about the TLS path in `.cargo/audit.toml` or `deny.toml`, the floors in Cargo.lock (`rustls` 0.23.46, `rustls-webpki` 0.103.16, `webpki-roots` 1.x 1.0.10), the 0.26 shim with no roots of its own, no manifest naming reqwest, hyper, curl or native-tls, and each check failing on made-up inputs |

Counted on 2026-10-24 after the task «tls-trust», with `cargo nextest list --run-ignored all`: 714 in the `aruna` crate and 46 in `aruna-desktop`, which is 760; without `--run-ignored` the run is 750. That is 759 − 4 + 5: the four of the guard of the webpki exceptions went with them, and the five of the permanent guard of TLS trust came in. Counted on 2026-10-21 after the task «rustls-0.23.46», with `cargo nextest list --run-ignored all`: 713 in the `aruna` crate and 46 in `aruna-desktop`, which is 759; without `--run-ignored` the run is 749. The four fewer are the guard of the `rustls` 0.23.45 exceptions, removed with them when `rustls` 0.23.46 came in. Counted on 2026-10-21 at `a24dd54`, with `cargo nextest list --run-ignored all`: 717 in the `aruna` crate and 46 in `aruna-desktop`, which is 763; without `--run-ignored` the run is 753. The four since 2026-10-19 are the guard of the webpki exceptions. Counted on 2026-10-19 at `f8d7f06`, with `cargo nextest list --run-ignored all`: 713 in the `aruna` crate, as above, and 46 in `aruna-desktop`, which is 759; without `--run-ignored` the run is 749, 708 and 41. The 22 since 2026-10-15 came with the third part of the PDF transfer, part3-1 to part3-4, and the tasks between; each is recorded in the specification's history (7.8). Counted on 2026-10-15 at `7f0ae50`, with `cargo nextest list --run-ignored all`: 692 in the `aruna` crate, as above, and 45 in `aruna-desktop`, which is 737; without `--run-ignored` the run is 727, 687 and 40. What each task between added is recorded in the specification's history (7.8); the numbers here are taken from the list, not reconstructed from it. Counted on 2026-09-30, after the second part of the PDF transfer: 655 in the `aruna` crate and 40 in `aruna-desktop`, which is 695; without `--run-ignored` 687. The 55 since 2.6.2 came with the PDF: the module and its samples, the phase of the export, the console's two PDF variables (the switch among them removed on 2026-10-18), the `quick-xml` guard, and one in the shell for the PDF phase the window does not show yet. Counted on 2026-09-29, at the release gate of 2.6.2, with `cargo nextest list --run-ignored all`: 601 in the `aruna` crate, as above, and 39 in `aruna-desktop`, which is 640. Without `--run-ignored` the run is 633: the seven heavy ones stay behind `#[ignore]` and need the archive. The one since 2026-09-26 came with `d18e1dd` – a run that waits through many holders of the publish lock takes the lock itself; 639 and 632 before it. The one since the release of 2.6.1 came with `69c2a9c` – a symlinked file inside the package refused rather than read as a document; 638 and 631 before it. The three since that afternoon came with `a0f929c` – the code points of a folder collision in Unicode form, told in the console and on the window; 635 and 628 before them. The two since 2026-09-25 came after the release gate of 2.6.1 – a question behind a silent proxy ending by its own deadline (`4550fee`) and a deadline under the TLS handshake named as one (`42529e6`); 633 and 626 before them. The thirty-three since 2026-09-24 came with the polish block for 2.6.1, `eef9f4e` to `ed8f48d` – a console that survives a closed stream and refuses in Russian, the published copy checked before the reader's goes, the proxy from the environment, the publish lock on `flock` and its race, a link under the package's name, long names cut to fit and names or group folders the disk takes for one, a cancel from the window on a small archive, the manifest's names reaching the window, and the model refusing what is not its to read; 600 and 594 before them. The nine since 2026-09-23 came with `ede953a` – a named pipe where the publish lock or a staging marker belongs, the window's font-error sentences, the model's boundary guard, and the new `pdf_acceptance.rs`; 591 and 585 before them. The previous count, 555 and 549 on 2026-09-19, had gone stale by the acceptance audit of 21.09, which ran 560 and 554; since then came the catalogue's titles in the group headings, the package-name boundary of the shell's reading commands, and the manifest's list of entries that are not manuscripts; on 2026-09-23 three download tests (the read timeout in force, a cancel reaching a silent server, a cut body reported as the network's) and two for the declared XML version, 586 and 580 before them.

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
- **Under a sandbox that denies the network, the tests that go to loopback can
  fail without a defect of theirs.** Which they are, how many, and what the
  offline runs measured: *Tests that go to loopback*, below.
- No `sleep` is used for synchronisation.
- No random seed is unfixed: the fuzz harnesses use a constant seed and print it.
- Temporary directories are removed by `Drop`, including on failure.
- The heavy fixtures are generated inside the test that needs them, not
  committed: their size is the point and the repository is not the place for it.

### Tests that go to loopback

No test leaves the machine, but 43 open a socket on 127.0.0.1: a local server
the download is pointed at, a local proxy, or a port where nothing listens so
that the download fails. All 43 are in the ordinary run of 727 and none is
behind `#[ignore]`; all are in the `aruna` crate, the shell has none. Counted
on 2026-10-15 at `7f0ae50`.

`frontend/tests/loopback-record.test.ts` holds the two tables below to the
sources, and fails naming the tests that are missing from them, the ones they
name that the sources do not give, and a number that is not the length of its
table. It looks where they were found, in `cli/src`, `cli/tests` and
`src-tauri/src`, by the same signals: a test opens a socket when its body calls
`TcpListener::bind`, `TcpStream::connect` or `UdpSocket::bind`, or reaches a
function that does – of its own file, or of `tests/support/mod.rs` for a binary
that declares `mod support` – directly, through another such function, or
through the type such a function belongs to (`FakeServer`, `Origin`). A test
that only writes a loopback address down is found too, and which table it goes
in is a person's call, made by reading it: four of them send the download to a
closed port and are in the first table, five never open a socket and are in the
second. Moving a test from one table to the other is the one mistake the guard
cannot see.

**43 tests open a socket on loopback.**

| binary | test | file | opens | through |
|---|---|---|---|---|
| `aruna::bin/aruna` | `console::tests::a_proxy_refusal_is_named_as_one` | `cli/src/console.rs` | listens as a proxy; `ureq` connects; the test joins the thread that waits in `accept` | the test body |
| `aruna` | `download::tests::the_user_agent_carries_the_version_from_the_manifest` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::truncated_body_is_rejected` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::transient_failure_is_retried` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::checksum_mismatch_is_reported_without_downloading_again` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::a_question_behind_a_silent_proxy_ends_by_its_deadline` | `cli/src/download.rs` | listens as a proxy; the question connects | the test body |
| `aruna` | `download::tests::a_proxy_named_in_the_environment_carries_the_request` | `cli/src/download.rs` | listens as a proxy; the question connects | the test body |
| `aruna` | `download::tests::server_unavailable_is_retried` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::not_found_is_reported_once_and_as_a_status` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::matching_checksum_is_accepted` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::asking_a_question_fails_as_an_error_rather_than_a_surprise` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::an_answer_that_is_not_text_is_refused` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::a_response_with_no_stated_length_still_arrives_whole` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::a_refused_download_leaves_nothing_behind` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::a_flood_is_refused_by_size` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::a_cut_inside_a_character_is_still_a_refusal` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::the_metadata_cap_is_a_refusal_and_not_a_prefix` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::an_endless_answer_is_given_up_on_by_its_deadline` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::a_server_that_dribbles_for_ever_is_given_up_on` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::a_cancel_reaches_a_download_whose_server_went_silent` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::a_server_that_never_answers_is_given_up_on_within_the_read_timeout` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::a_server_that_goes_silent_mid_body_is_given_up_on_within_the_read_timeout` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::a_cancel_reaches_a_download_whose_server_went_silent_mid_body` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::a_proxy_the_crate_cannot_speak_is_bypassed_and_the_request_goes_direct` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::a_body_cut_by_the_server_is_a_network_failure_not_a_disk_one` | `cli/src/download.rs` | listens; the code under test connects | `FakeServer`, `cli/src/download.rs` |
| `aruna` | `download::tests::network_error_on_unreachable_host` | `cli/src/download.rs` | the download connects to `127.0.0.1:1`, where nothing listens | `download_file` |
| `aruna` | `download::tests::failed_download_leaves_no_files_behind` | `cli/src/download.rs` | the download connects to `127.0.0.1:1`, where nothing listens | `download_file` |
| `aruna` | `download::tests::failed_download_preserves_previous_file` | `cli/src/download.rs` | the download connects to `127.0.0.1:1`, where nothing listens | `download_file` |
| `aruna` | `tests::an_unusable_proxy_is_said_before_the_download` | `cli/src/lib.rs` | the download connects to `127.0.0.1:9`, where nothing listens | `download_archive` |
| `aruna::cache_concurrency` | `runs_racing_for_one_archive_all_get_a_whole_one` | `cli/tests/cache_concurrency.rs` | listens; the runs connect; `Drop` connects to wake `accept` and joins its thread | `Origin`, `cli/tests/support/mod.rs` |
| `aruna::cache_concurrency` | `a_second_wave_is_served_entirely_from_the_cache` | `cli/tests/cache_concurrency.rs` | listens; the runs connect; `Drop` connects to wake `accept` and joins its thread | `Origin`, `cli/tests/support/mod.rs` |
| `aruna::cache_concurrency` | `repeated_downloads_do_not_accumulate_connections_or_descriptors` | `cli/tests/cache_concurrency.rs` | listens; the runs connect; `Drop` connects to wake `accept` and joins its thread | `Origin`, `cli/tests/support/mod.rs` |
| `aruna::cache_concurrency` | `a_stale_part_file_is_swept_and_a_fresh_one_is_left_alone` | `cli/tests/cache_concurrency.rs` | listens; the runs connect; `Drop` connects to wake `accept` and joins its thread | `Origin`, `cli/tests/support/mod.rs` |
| `aruna::cache_lifecycle` | `a_cold_run_downloads_and_a_warm_one_does_not` | `cli/tests/cache_lifecycle.rs` | listens; the download connects; `Drop` connects to wake `accept` | `Origin` of the file |
| `aruna::cache_lifecycle` | `an_archive_that_hashes_wrong_is_refused_and_not_cached` | `cli/tests/cache_lifecycle.rs` | listens; the download connects; `Drop` connects to wake `accept` | `Origin` of the file |
| `aruna::cache_lifecycle` | `a_republished_archive_replaces_the_edition_it_supersedes` | `cli/tests/cache_lifecycle.rs` | listens; the download connects; `Drop` connects to wake `accept` | `Origin` of the file |
| `aruna::cache_lifecycle` | `an_unwritable_cache_directory_falls_back_to_a_run_of_its_own` | `cli/tests/cache_lifecycle.rs` | listens; the download connects; `Drop` connects to wake `accept` | `Origin` of the file |
| `aruna::cache_lifecycle` | `an_unreachable_source_fails_without_leaving_anything_behind` | `cli/tests/cache_lifecycle.rs` | binds a port and lets it go; the download connects to it | the test body |
| `aruna::cache_lifecycle` | `a_failed_download_leaves_no_scratch_directory_behind` | `cli/tests/cache_lifecycle.rs` | binds a port and lets it go; the download connects to it | the test body |
| `aruna::cache_lifecycle` | `a_redirect_is_followed_and_the_result_still_has_to_hash_right` | `cli/tests/cache_lifecycle.rs` | listens; the download connects; `Drop` connects to wake `accept` | `Origin` of the file |
| `aruna::cache_lifecycle` | `a_redirect_loop_gives_up_instead_of_going_round_for_ever` | `cli/tests/cache_lifecycle.rs` | listens; the download connects; `Drop` connects to wake `accept` | `Origin` of the file |
| `aruna::cache_lifecycle` | `a_newer_edition_is_announced_and_the_pinned_archive_still_arrives` | `cli/tests/cache_lifecycle.rs` | listens; the download connects; `Drop` connects to wake `accept` | `Origin` of the file |
| `aruna::cache_lifecycle` | `a_file_where_the_cache_directory_belongs_does_not_fail_the_run` | `cli/tests/cache_lifecycle.rs` | listens; the download connects; `Drop` connects to wake `accept` | `Origin` of the file |

**5 tests write a loopback address down and open no socket.**

| binary | test | file | the address is | – |
|---|---|---|---|---|
| `aruna` | `cache::tests::the_name_is_always_a_single_path_component` | `cli/src/cache.rs` | a URL the cache file is named after | – |
| `aruna` | `cache::tests::a_plain_url_is_named_after_its_file` | `cli/src/cache.rs` | a URL the cache file is named after | – |
| `aruna` | `download::tests::the_proxy_is_chosen_as_curl_chooses_it` | `cli/src/download.rs` | proxy settings, parsed and never used | – |
| `aruna` | `download::tests::a_proxy_that_cannot_be_used_is_named` | `cli/src/download.rs` | proxy settings, parsed and never used | – |
| `aruna::cli_process` | `a_local_archive_run_makes_no_network_request` | `cli/tests/cli_process.rs` | proxy variables a run that reads a local archive must not use | – |

#### What the no-network profile does to them

The offline runs of the acceptance use the profile `scripts/nonet.sb` (in this
repository since 2026-10-16; before that a copy lay beside each task's report)
and run the ordinary set from the repository root as

```
sandbox-exec -f scripts/nonet.sb cargo nextest run --locked --offline --workspace --no-tests=pass --no-fail-fast
```

The profile is four rules; the block below is the text of `scripts/nonet.sb`
as it stands:

```
(version 1)
(allow default)
(deny network-outbound)
(allow network-outbound (remote ip "localhost:*"))
(allow network-outbound (remote unix-socket))
```

Loopback is allowed by its text, and refused in practice now and then: on
2026-10-11, 8 of 72 000 `connect` calls to a listener on 127.0.0.1 failed with
`EPERM` (and one with `ETIMEDOUT`) under it, against 0 of 60 000 with no
sandbox and 0 of 30 000 under a profile that allows everything (task
pre-3.0.0-hardening). Nothing reaches the sandbox's log, and the machine runs
two network filters (Cisco AnyConnect's socket filter and LuLu), so where the
refusal is made is not known; that it comes with the profile is.

How many tests it turns red is not a constant, and is recorded as measured:

| date, tree | run | not passed |
|---|---|---|
| 2026-10-11, pre-3.0.0-hardening | offline runs of the acceptance | `download::tests::a_proxy_named_in_the_environment_carries_the_request`, `cache_lifecycle::a_redirect_is_followed_and_the_result_still_has_to_hash_right`; in the full offline run `cache_concurrency::a_stale_part_file_is_swept_and_a_fresh_one_is_left_alone` hung past 120 s |
| 2026-10-14, `0481b5e` | ordinary set | FAIL `cache_lifecycle::a_redirect_loop_gives_up_instead_of_going_round_for_ever` – "the loop was walked 7 times, so it is being retried"; TIMEOUT `console::tests::a_proxy_refusal_is_named_as_one` |
| 2026-10-15, `534e095` and the tree of the first group of updates for 3.0.0 that became `7f0ae50` | ordinary set, once on each | none on `534e095`; FAIL `download::tests::asking_a_question_fails_as_an_error_rather_than_a_surprise` – `Operation not permitted (os error 1)` – on the updated tree |
| 2026-10-15, `7f0ae50` | ordinary set, first of two in a row; host 80 % idle, 221 s | 6 FAIL, 2 TIMEOUT – below |
| 2026-10-15, `7f0ae50` | ordinary set, second of the two; host 88 % idle, 76 s | none: 727 of 727 |
| 2026-10-15, `f0da0f6` | ordinary set, one run | 726 of 727: FAIL `download::tests::an_answer_that_is_not_text_is_refused` – `Operation not permitted (os error 1)` on 127.0.0.1 |

The first of the two runs of 2026-10-15, test by test:

| test | outcome | what it said |
|---|---|---|
| `download::tests::asking_a_question_fails_as_an_error_rather_than_a_surprise` | FAIL, 11.1 s | status 500 came back as a network error: `Connect error: Operation not permitted (os error 1)` |
| `download::tests::transient_failure_is_retried` | FAIL, 66.4 s | "should have taken exactly two attempts", 3 against 2 |
| `download::tests::server_unavailable_is_retried` | FAIL, 67.7 s | 3 attempts against 2 |
| `cache_lifecycle::a_cold_run_downloads_and_a_warm_one_does_not` | FAIL, 44.9 s | "the second run went back to the network instead of using the cache", 2 against 1 |
| `cache_concurrency::runs_racing_for_one_archive_all_get_a_whole_one` | FAIL, 65.1 s | "six racing runs made 12 requests; that is more than one each" |
| `cache_concurrency::a_second_wave_is_served_entirely_from_the_cache` | FAIL, 72.7 s | "the cold run made 3 requests", against 1 |
| `cache_concurrency::a_stale_part_file_is_swept_and_a_fresh_one_is_left_alone` | TIMEOUT, 120 s | ended by nextest |
| `console::tests::a_proxy_refusal_is_named_as_one` | TIMEOUT, 120 s | ended by nextest |

Every test that has gone red under the profile, eleven in all, is in the first
table above. The difference between the two runs of the day is a fact and is
left as one: the binaries were the same (the build before them compiled
nothing), the first ran on a busier host, and why it met eight refusals and the
second none was not found.

Read by the code, a refusal shows in three ways:

- **One request too many – FAIL on a count.** The download takes a connection
  that does not go through for a passing fault of the network and tries again;
  that is its job. The local server counts connections, and on this machine it
  accepts one before the client's `connect` is let go (`download.rs`, the note
  on `PAST_DEADLINE`), so a connection held and then given up on is counted
  and the retry is counted again. Tests that assert how many requests arrived
  see one more than they expect – the two retry tests, the cache tests, and the
  redirect loop of 2026-10-14, which walked its chain seven times against six –
  and the waits before the retries are what made those runs 45–73 s long.
- **The refusal is the error – FAIL with `EPERM` in its text.** Where the path
  under test does not retry, the refusal comes back as what the test sees:
  `asking_a_question…` expected the server's status 500 and got
  `Operation not permitted (os error 1)`.
- **A wait for a connection that never comes – TIMEOUT.** Two tests wait on a
  connection of their own. `a_proxy_refusal_is_named_as_one` starts a proxy
  thread that blocks in `accept` and joins it once `ureq` is done; when the
  client's `connect` is refused the thread is never woken, the join waits, and
  nextest ends the test at 120 s. The `Origin` of `tests/support/mod.rs` is
  stopped by `Drop`, which connects once to wake its own `accept` and then
  joins the thread; when that connection is refused the join waits the same
  way – `a_stale_part_file…`. (The comment there reads a failed wake-up as "it
  is already gone"; under this profile it is not.)

None of this is a defect of the tests or of the code: a refused loopback
connection is the environment's, and every one of these tests is green in an
ordinary run and on CI. They are not changed for it, **and the answer is
neither a retry nor leaving them out** – retries are absent from
`.config/nextest.toml` on purpose, and excluding a test from the run or
skipping it by environment is barred (owner's decision, 2026-10-15). A red
offline run is read against this section: a test that went red and is in the
first table is this; one that is not is a finding.

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
