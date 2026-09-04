# The shape of the Rust side

What each module is for, which way the dependencies point, and where the thing
that does not exist yet — a PDF renderer — attaches without the layers below it
changing. The Tauri interface used to be named here as the second of two; it was
attached at `app` on 2026-08-30 and started building the corpus on 2026-09-02,
and §7 is where it is described.

The environment, the frontend stack, the pinned versions and the checks that
guard them are **not** described here. They are fixed by
[`PROJECT-SPEC.ru.md`](PROJECT-SPEC.ru.md) (редакция 28, 2026-09-04), which is normative; this
document is about the Rust that runs underneath it.

---

## 1. Two crates, one workspace

| | |
|---|---|
| `cli/` | package `aruna` 2.5.2 — the program. A library (`aruna`) plus a binary (`aruna`) that is a thin adapter over it. |
| `src-tauri/` | package `aruna-desktop` 0.2.0, library `aruna_desktop_lib` — the desktop shell: the window, the permissions, and the bridge. Its two commands ask the core where the package went and count what is in it; the logic stays in `cli/`. |

They were independent crates with a `Cargo.lock` each until 2026-08-30, when
they were joined: the root manifest lists both, **one lock file** sits beside it
and one `target/` serves both. `src-tauri/` now depends on `cli/` by path and
calls it through its first command — see §7.

Profiles live in the root manifest and only there. A workspace ignores the
profile sections of its members, so a copy left in `cli/Cargo.toml` would have
stopped taking effect without saying so.

Both crates carry `#![forbid(unsafe_code)]` in every library and binary root.
The compiler holds it; `cargo-geiger` was excluded from this project and is not
to be reinstalled. What the *dependency* tree contains is `cargo-deny`'s
subject, and since the crates were joined into one workspace on 2026-08-30 that
subject has one policy: a single `deny.toml` at the root. The per-crate files
were removed with the merge — one dependency graph judged by two policies is not
a stricter arrangement but an incoherent one, and it showed: the run from
`src-tauri` began failing on `ISC`, a licence the same tree had carried all
along and the other file had allowed for two weeks.

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
- **It holds no global mutable state.** The one `OnceLock` in the crate is in
  `Job::unattended`: a cancel flag shared by every unattended run and never set,
  because nothing is handed out that could set it. It is initialised once and
  read-only thereafter, which is what makes it not state.
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

At `app`, and it is attached. `src-tauri/Cargo.toml` carries

```toml
aruna = { path = "../cli" }
```

since 2026-08-30, which makes `aruna::app::build_corpus`, `job::{Cancel, Job}`,
`progress::Progress` and `aruna::paths` reachable from the shell. The line was
compiled once before, on 2026-08-23, and reverted the same day: an unused
dependency is a `cargo-machete` finding, and there was nothing to call it from
until a command existed. One does now.

**The first command is the shape the rest are held to.** `corpus_location` asks
`aruna::paths` for the downloads directory, takes the package and inventory
names from the constants declared beside it, and does nothing else but join the
path and ask whether it is there. No second answer to where the corpus goes, and
no path logic in the shell — that is the whole point of §4.9.6.

**One rule about the wire, and something checks it now.** Tauri 2 expects a
command's arguments in camelCase on the JavaScript side unless the command
declares `#[tauri::command(rename_all = "snake_case")]` — confirmed against the
pinned version's documentation on 2026-08-31 (`tauri.app/develop/calling-rust`).
`corpus_stats(path: String)` is a single word and reads the same either way, so
the call site was right by accident rather than by care. This paragraph then
said that the next argument with two words is where that stops being true, and
that without `specta` nothing in the build would say so.

That argument arrived on 2026-09-02 — `build_corpus(local_archive)` — and specta
arrived with it. The call sites are no longer written: `frontend/src/bindings.ts`
is generated from these declarations, spells the argument `localArchive`, and a
Rust test fails if the committed file is not what the declarations now produce.

**The second one shows what "held to" means.** `corpus_stats` counts the
manuscripts and the CTH groups in a package — and takes the path to it as an
argument rather than working it out, because working it out would be that second
answer. The window asks `corpus_location` first and hands the result on. The
counts come from the `counts` object of the manifest the core's exporter wrote,
under the file name the core declares (`aruna::export::MANIFEST`); a package with
no manifest, a broken one, or one without those fields is counted by walking it
instead, and the answer says which of the two happened.

It answers two further questions from the same manifest, and both are about how
uneven the corpus is rather than how large. The **spread** — the largest group
with its size, how many groups hold a single fragment, how many fragments carry
no CTH at all — is read from the `groups` array the exporter already writes,
whose per-group document lists are counted without being materialised; the walk
derives the same three from the directories when there is no manifest. The
**writing counters** — documents outside NFC, documents using private-use code
points and how many such points there are, and the sum of the manifest's
anomaly counters — come only from the manifest, because they were counted while
the documents were parsed and a walk cannot recover them; there they are `null`
rather than zero, since zero anomalies and an uncounted corpus are different
statements. The group with no CTH is recognised by the label the core gives it
(`aruna::parse::MISSING`), not by a string written here. What stays in the shell
is the counting, which is about a directory on disk; what the core owns is every
name involved in finding it.

**It used to need renaming on import.** Both packages were called `aruna`, so
the line read `aruna_core = { package = "aruna", path = "../cli" }` and rested
on the two library names differing — `aruna` and `aruna_lib`. The shell was
renamed to package `aruna-desktop`, library `aruna_desktop_lib`, which removes
the collision at its source: the dependency is now an ordinary one, and there
is no second name for a reader to keep in their head.

What that adapter owns, and the core does not: job identifiers, IPC events, the
dialog and opener plugins, the store, the window. What it needed from the core
was already there — a typed request, a progress sink, a cancel flag, a typed
report — and on 2026-09-02 it was connected: `build_corpus` and `cancel_build`
are commands, `Progress` is implemented by a sink that emits `build-progress`
into the window, and `Job` is built inside the worker thread because it borrows
both halves and cannot outlive the call that made it. The contract those four
commands make is written out in `docs/FRONTEND-CONTRACT.md` §3.

The core gained exactly one thing in the process, and it is not serde: two
progress variants. `Downloading` and `DocumentsWritten` refine the two stages
that take time, because a stage that announces itself and then goes quiet for a
minute cannot drive a bar. They are refinements rather than milestones —
`Event::is_tick` — and `progress::Stderr` drops them, so the terminal says what
it always said.

**One constraint on the remaining markup work, checked before it starts:**
Svelte 5 renders a component to static HTML through `render()` from
`svelte/server`, which returns `{ body, head }` and needs the component
compiled with the server target. It does **not** return CSS — Svelte 4 did, and
Svelte 5 leaves extraction to the build tool unless `css: 'injected'` is set in
`svelte.config.js`. That option must stay off: the config is shared with the
window, and this document's stylesheet is already three separately built
sections that `style::join` orders. Render markup; leave the CSS where it is.

**DTOs are a separate layer from all of these.** `CorpusPresentation` borrows
(`<'a>`) and is not a serialisation format; an IPC type is an owned projection of
it, with `serde`, in the shell. That day came on 2026-09-02 and the decision went
to specta: `BuildReport`, `BuildFailure` and `BuildProgress` are declared in
`src-tauri/src/lib.rs`, and the TypeScript that reads them is generated from
those declarations. ts-rs was the alternative and would have generated the types
without the command wrappers, which is the half that was going wrong.

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
