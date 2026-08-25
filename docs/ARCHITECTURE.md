# The shape of the Rust side

What each module is for, which way the dependencies point, and where the two
things that do not exist yet — a Tauri interface and a PDF renderer — attach
without the layers below them changing.

The environment, the frontend stack, the pinned versions and the checks that
guard them are **not** described here. They are fixed by
`tauri-frontend-spec-v3.md` (редакция 3, 2026-08-23), which is normative; this
document is about the Rust that runs underneath it.

---

## 1. Two crates, no workspace

| | |
|---|---|
| `cli/` | package `aruna` 2.2.0 — the program. A library (`aruna`) plus a binary (`aruna`) that is a thin adapter over it. |
| `src-tauri/` | package `aruna` 0.1.0, library `aruna_lib` — the desktop shell. Today a proving ground for the frontend stack: it opens a window and contains no application logic. |

They are **independent crates with separate `Cargo.lock` files**, not a
workspace. Nothing in `src-tauri/` depends on `cli/` yet, and that is the
current design rather than an omission — see §7.

Both crates carry `#![forbid(unsafe_code)]` in every library and binary root.
The compiler holds it; `cargo-geiger` was excluded from this project and is not
to be reinstalled. What the *dependency* tree contains is `cargo-deny`'s
subject, and each crate has its own `deny.toml`.

## 2. The layers, and which module is which

Dependencies point one way only: downward in this table. Nothing in an upper
row is named by a lower one.

| layer | modules | what it may not do |
|---|---|---|
| **adapter** | `main.rs` | — |
| **application** | `app` | parse a command line, choose an exit code, print |
| **presentation** | `presentation`, `style`, `html`, `export/inventory` | read the filesystem, parse XML |
| **domain** | `parse`, `order`, `paths`, `catalog`, `md5`, `export/{naming,normalize,validate,verify,manifest}` | know a renderer exists |
| **infrastructure** | `archive`, `cache`, `download`, `zenodo`, `xml_scan`, `export/mod` | decide what the corpus means |
| **signals** | `progress`, `job`, `error` | depend on any of the above |

`progress` and `job` sit beside the stack rather than in it: every layer may
report through them, and they know nothing about what is being reported.

## 3. The core is independent, and this is what that means

Checked, not asserted:

- **It prints nothing.** The only `eprintln!` in the library is inside
  `progress::Stderr`, the sink the binary chooses; `progress::Silent` is what
  every test passes. A window will pass a third.
- **It never exits.** No `std::process::exit` outside `main.rs`; the binary is
  the only thing that turns a `Result` into an `ExitCode`.
- **It holds no global mutable state.** The one `OnceLock` in the crate is a
  test fixture.
- **It is not async, and has no thread pool.** The two `std::thread::spawn`
  calls in the crate are both inside `#[cfg(test)]`. Tauri's async commands do
  not require an async core — a window runs this on a blocking worker.
- **It knows nothing of Tauri, Svelte, TypeScript or a WebView**, and names no
  PDF library.

## 4. The pipeline

```
Zenodo / local .zip
  └─ download · cache · archive          infrastructure: fetch, verify MD5, open
      └─ xml_scan · parse                 bytes → ManuscriptRecord, gates and limits
          └─ order                        the one sort the whole program uses
              └─ presentation             CorpusPresentation: groups, fragments, hrefs
                  ├─ html + style         the self-contained inventory
                  ├─ export               folders, normalised documents, manifest
                  └─ (future) PDF         §6
```

**The binary drives the export branch, and only it, since 2.3.0.**
`app::build_corpus` resolves the archive and builds the package into the
reader's Downloads folder; the inventory a reader opens is the one inside it,
which links at every document.

Two releases were spent getting there. Until 2.2.0 the binary drove only the
first branch — it wrote a table of the corpus and never the corpus itself,
because the export was reachable from an example and nothing else. 2.2.0 wired
the second branch in and wrote both, which put two files called
`TLHdig_Beta_0.3.html` in one folder, and the one without links is the one a
reader opens first. 2.3.0 gave the standalone one up.

The library can still produce it: `crate::run` writes the unlinked inventory and
`tests/integration.rs` exercises it. What is gone is the application scenario
that had no caller left.

`presentation` is the single fan-out point, and that is the property to keep:
there is no path from XML to a rendered artefact that goes around it. The
architecture drawing the project works to shows the same shape — one
presentation model feeding a static HTML branch, a future PDF branch, and a
future frontend DTO.

## 5. Application services, progress, cancellation, errors

`app` holds the user-visible scenarios as typed request/report pairs —
`CorpusRequest` → `CorpusReport` for the whole run the binary drives, and
`PackageRequest` → `PackageReport` for the export it is built on — and `app::Failure` carries a partial result: a run that produced something and
also has something to say about it. Phases are explicit, so a failure names the
stage it happened in.

`job::Job` is what every long operation is handed: a `progress::Progress` sink
and a `job::Cancel` flag. Cancellation is **cooperative** and checked at safe
points — between documents, never inside an atomic rename — and it travels back
as `ArunaError::Cancelled { phase }`, which the binary words as a stop rather
than a failure. `Cancel` is `Send + Sync` and clonable, so the thread that
cancels is not the thread that works; that is the arrangement a window needs.

`error::ArunaError` is a `thiserror` enum, and the variants are the ones a
caller can act on differently — network, HTTP status, checksum, truncation,
oversize, collision, distortion, destination, invalid package, incomplete
package, cancellation, I/O. Each keeps its source and a relative path. Nothing
is flattened to a string before the outermost adapter: `main.rs` is where an
error becomes text and advice.

## 6. Where a PDF renderer attaches

At `presentation`, the same place HTML attaches — not at `parse`, and not at
`export`. What the renderer would need beyond `CorpusPresentation` is a print
model, and whether one is needed is a question for the day someone measures it,
not a trait to write in advance.

The print half of the stylesheet is already separate for this reason:
`frontend/src/inventory/print.css` is emitted last and is the only section that
speaks about paper. `docs/PDF-ACCEPTANCE.md` holds the criteria. **No PDF
library has been chosen**, and none is to be chosen without a comparison run
against the real corpus.

## 7. Where a Tauri adapter attaches

At `app`, and the wiring has been compiled rather than imagined. Adding

```toml
aruna_core = { package = "aruna", path = "../cli" }
```

to `src-tauri/Cargo.toml` makes `aruna_core::app::build_corpus`,
`job::{Cancel, Job}` and `progress::Progress` reachable from the shell; both
packages being named `aruna` is not an obstacle, because their library names
differ (`aruna` and `aruna_lib`). Verified on 2026-08-23 and then reverted: an
unused dependency is a `cargo-machete` finding, and there is nothing to call it
from until the first real command exists.

What that adapter will own, and the core will not: job identifiers, IPC events,
the dialog and opener plugins, the store, the window. What it will need from
the core is already there — a typed request, a progress sink, a cancel flag, a
typed report.

**One constraint on the remaining markup work, checked before it starts:**
Svelte 5 renders a component to static HTML through `render()` from
`svelte/server`, which returns `{ body, head }` and needs the component
compiled with the server target. It does **not** return CSS — Svelte 4 did, and
Svelte 5 leaves extraction to the build tool unless `css: 'injected'` is set in
`svelte.config.js`. That option must stay off: the config is shared with the
window, and this document's stylesheet is already three separately built
sections that `style::join` orders. Render markup; leave the CSS where it is.

**DTOs are a separate layer from all of these.** `CorpusPresentation` borrows
(`<'a>`) and is not a serialisation format; an IPC type will be an owned
projection of it, with `serde`, when there is a command to carry it. Specta or
ts-rs is a decision for that day, not before.

## 8. The two rules that outrank the architecture

**The source XML is never touched.** Not rewritten, not re-encoded, not
reordered, not normalised in place. `tests/corpus.rs` walks the real archive and
asserts non-distortion and that nothing was written; `tests/xml_contract.rs`
holds the fixture set immutable; `export/verify.rs` compares what was written
against what was read and refuses the package if a document changed beyond the
permitted normalisation.

**Two runs of one archive are byte-identical.**
`tests/reliability.rs::two_builds_of_one_archive_are_byte_identical` compares
two exports file by file; `examples/determinism.rs` does it against the real
corpus. Nothing may enter the output that a clock or a hash map decides — the
package's inventory carries no timestamp for exactly this reason.

## 9. Running the checks

`docs/TESTING.md` is the full account: profiles, what each test binary holds,
and the rules the suite keeps. In short, from `cli/`:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --profile ci
cargo llvm-cov nextest --summary-only
cargo audit && cargo deny check && cargo machete
```

`src-tauri/` takes the same battery. It has **one** test —
`a_build_without_the_feature_registers_no_webdriver`, which is the fourth and
last of the gates keeping the end-to-end contour out of a release: the other
three are declarations that `frontend/tests/spec-guard.test.ts` reads, and this
one is the compiler's own answer about what the builder registers when the
feature is off. `--no-tests=pass` is still needed under `--features e2e`, where
that test is deliberately absent.
