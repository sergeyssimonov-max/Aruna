# The shape of the Rust side

What each module is for, which way the dependencies point, and where the thing
that does not exist yet — a PDF renderer — attaches without the layers below it
changing. The Tauri interface used to be named here as the second of two; it was
attached at `app` on 2026-08-30 and started building the corpus on 2026-09-02,
and §7 is where it is described.

The environment, the frontend stack, the pinned versions and the checks that
guard them are **not** described here. They are fixed by
[`PROJECT-SPEC.ru.md`](PROJECT-SPEC.ru.md) (редакция 62, 2026-09-18), which is normative; this
document is about the Rust that runs underneath it.

---

## 1. Two crates, one workspace

| | |
|---|---|
| `cli/` | package `aruna` — the program; its version is the current release named in the [README](../README.md#выпуски). A library (`aruna`) plus a binary (`aruna`) that is a thin adapter over it. |
| `src-tauri/` | Package `aruna-desktop`, library `aruna_desktop_lib`. The window locates and opens a library, reads its statistics and asks the core to check for a new edition, build or cancel. `lib.rs` holds startup and plugins, `commands.rs` registers IPC, `dto.rs` describes the messages, `build.rs` owns the running job and `read.rs` reads the published library. |

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
| **adapter** | `main.rs`, `console` | — |
| **application** | `app` | parse a command line, choose an exit code, print |
| **presentation** | `presentation`, `style`, `html`, `export/inventory`, `pdf/{mod,layout,world,label,post,fonts}` | read the filesystem, parse XML |
| **domain** | `parse`, `document`, `xml_wellformed`, `order`, `paths`, `catalog`, `json`, `cth_titles`, `fonts`, `md5`, `sha256`, `export/{naming,normalize,validate,verify,manifest,counts}` | know a renderer exists |
| **infrastructure** | `archive`, `cache`, `download`, `zenodo`, `xml_scan`, `export/{mod,staging,lock,pdf}` | decide what the corpus means |
| **signals** | `progress`, `job`, `error` | depend on any of the above |

The map as of 2026-10-01, after the clean-up of that day: `export/mod` owns the
pipeline and is the one part besides `export/staging` (the staging directory,
its owner's marker, the package held aside, the sweep of abandoned ones) and
`export/lock` (one publication at a time) that opens a file; `export/pdf` is the
PDF phase, `pdf/` the renderer it calls – pure, a document in and bytes out,
with `pdf/post` the one walk over a page's content stream that the product, the
tests and the gate share. `console` is a module of the binary, not of the
library: the Russian wording of the console.

In the console adapter, `main.rs` selects exit codes, writes to stdout/stderr
and supplies advice. `console` formats error headlines and causes. The private
`diagnostics.rs` catalogue supplies named English templates to both the library
and the binary; `export::validate` uses those keys when formatting its 36
diagnostics. A macro expands each key to a literal, so Rust's `format!` still
checks the arguments. The binary compiles the same catalogue into `Phrase`;
its exhaustive `russian` match requires a translation for every key. No key or
helper is a public Rust API or part of IPC.

The error fields remain strings, so the console still recognises their text.
Translation uses the first matching template in catalogue order, preserves the
captured data verbatim, and leaves an unknown phrase unchanged. In particular,
the template for a file belonging to no document precedes the general template
for a file belonging to another document. Producers outside `export::validate`
still write English literals and are checked by the existing source guard.
The library's error types and their `Display` text are not translated; the
desktop window has its own wording for its audience.

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

For normal builds, `editions` checks the TLHdig collection before consulting the archive cache. The selected record, checksum and bibliographic data travel together into the exporter. It hashes the same file handle it parses, writes provenance into the inventory and manifest, checks for an older candidate under the publication lock and re-reads the installed provenance before releasing the previous package. The desktop retains the verified update with its destination while the reader uses the existing inventory. [Edition selection and migration](CORPUS-EDITIONS.md).

```
Zenodo / local .zip
  └─ download · cache · archive          infrastructure: fetch, verify MD5, open
      ├─ document                         bytes → Document: the XML model, read by the PDF phase
      └─ xml_scan · parse                 bytes → ManuscriptRecord, gates and limits
          └─ order                        the one sort the whole program uses
              └─ presentation             CorpusPresentation: groups, fragments, hrefs
                  ├─ html + style         the self-contained inventory
                  ├─ export               folders, normalised documents, PDFs, manifest
                  └─ pdf                  one document → its PDF, through Typst (§6)
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
architecture drawing the project works to shows the same shape – one
presentation model feeding a static HTML branch, ~~a future PDF branch,~~ and a
future frontend DTO. The PDF branch goes from the document model instead (§6,
since 2026-09-30).

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
package, cancellation, I/O. Variants keep their associated paths, counts and,
where applicable, typed source errors. Some detail fields (`reason`, `first`,
`holder`, `invariant`) already contain English strings; they are not a typed
diagnostic tree. `main.rs` selects the console's wording and advice without
changing those public variants or their `Display` contract.

### Manuscript identity during export

`export::build` sorts the admitted fragments once in display order.
`export/documents.rs` keeps each record, placement and source entry together.
The second pass over the same open ZIP looks up the archive name and fills
that document's two digests. Successful placement gives even entries with
identical bytes distinct destinations. The PDF phase consumes each row and
appends it with its PDF result to `PackageDocuments`. This is the only writer of the aligned
slices used by the public renderers and validator. Staged and published
validation each read the files from disk. Publication still rechecks the
destination under the lock and retains the previous package through read-back.
If an interrupted rollback left `.TLHdig_Beta_0.3.previous` beside the working
package, a retry leaves that backup in place. It moves the working package to
a marked, run-specific recovery directory. A rejected retry restores only the
package it moved. A successful read-back releases the backups. On a filesystem
without a verified publication lock, a run only releases copies it moved itself.
If restoring the working name fails, the error carries both the publication
failure and the rollback failure.

## 6. Where the PDF renderer attaches

*Corrected 2026-10-19.* The renderer is in the tree and attaches at the
document model: `cli/src/pdf/` takes a document read by `document`, lays it out
and hands Typst the template and the data, and `export` writes the bytes into
the folder of the document's XML, in a phase after the documents and before the
inventory and the manifest. Every build runs it since 2026-10-18 (owner's decision
of that day, for 3.0.0); the inventory, written by `html` from `presentation`, links
each document's name at its PDF, and at its XML where none was built (owner's
decision of 2026-10-19). The two paragraphs below are what this section said
before the converter was written.

~~At `presentation`, the same place HTML attaches – not at `parse`, and not at
`export`. What the renderer would need beyond `CorpusPresentation` is a print
model, and whether one is needed is a question for the day someone measures it,
not a trait to write in advance.~~

The print half of the stylesheet is already separate for this reason:
`frontend/src/inventory/print.css` is emitted last and is the only section that
speaks about paper. `docs/PDF-ACCEPTANCE.md` holds the criteria. ~~**No PDF
library has been chosen**, and none is to be chosen without a comparison run
against the real corpus.~~

**Decided 2026-09-30 by the owner, after four trials against the real
corpus.** The engine is Typst (`PROJECT-SPEC.ru.md` 4.10), and the renderer is
split the way HTML is. `cli/src/pdf/` is pure, like `html`: a document in, the
bytes of a PDF out, no file opened. Writing those bytes is `export`'s, as
writing the inventory is: a PDF phase after the XML documents are written, before
the inventory and the manifest, inside `Staging`, so cancellation and failure
leave the reader's folder untouched by the same mechanism as today. The
document model still knows nothing about PDF. "Not at `export`" above is kept
for the rendering; the phase that writes files is where files are written.

## 7. Where a Tauri adapter attaches

Current module boundaries, checked 2026-11-17:

| module | responsibility |
|---|---|
| `src-tauri/src/lib.rs` | Application startup, plugins, managed state and startup font logging. |
| `src-tauri/src/commands.rs` | The IPC entry points and the shared `contract()` used for runtime registration and TypeScript export. |
| `src-tauri/src/dto.rs` | Response and event types, error presentation, bounded counters and pure projections of core data. No filesystem access or Tauri runtime calls. |
| `src-tauri/src/build.rs` | Destination validation, the running-job slot and its guard, cancellation, blocking work and the progress sink. |
| `src-tauri/src/read.rs` | Reading and validating a finished package, using the core's names and rules. |
| `src-tauri/src/tests/` | Existing wire, reading, cancellation and release-feature checks, still registered under their original root module names. The poisoned-mutex test lives beside the private state in `build.rs`. |

The build command delegates to `build::start`. The order stays: validate the
destination, obtain the resource directory and create the request and cancel
flag, claim the slot, then create the job ID and progress sink inside
`spawn_blocking`. `Claimed` releases the slot on success, failure or unwinding;
`cancel_build` sets the existing core flag, and completion still supplies the
cancellation acknowledgement. The mutex protecting the slot stays private.

`BuildProgress` keeps the same event name, fields and stage spellings. Its pure
projection and PDF-count carry-forward live with the DTO; the sink in `build`
holds the last count and emits the event. `commands::contract()` remains the
single source of registration and generated bindings. The bindings comparison,
state and cancellation tests, component checks and native WebdriverIO scenario
protect these boundaries. The dated account below records how they arose.

The Svelte component creates one `WindowState` per mounted window.
`window-state.svelte.ts` keeps progress and cumulative counters on the
building screen and filters events using the active and last finished job IDs.
`window-ipc.ts` owns the subscription lifetime and the order of Tauri calls.
The view reads the state and formats it. `failure-text.ts` uses the window's
translation catalogue, selects cancellation text by phase and passes through
the shell message for unknown codes. The Rust window-seam test compares
actual core failures with the same catalogue used in the view.

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
arrived with it. It left on 2026-09-06 with the archive picker, and a different
one took its place the same day: `build_corpus(destination)`, the folder the
package goes into. `destination` is a single word, so the project is once again
without a two-word argument — and specta stays, which is the point. The call
sites are no longer written: `frontend/src/bindings.ts` is generated from these
declarations, and a Rust test fails if the committed file is not what the
declarations now produce. The hazard has no instance today;
the guard is in place for the next one, which is the only arrangement under
which it will be there when that argument arrives.

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
dialog and opener plugins, the window (the store plugin went on 2026-09-22,
`825bfd2`: nothing ever called it). What it needed from the core
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
`src-tauri/src/lib.rs` (the types of the reading commands in
`src-tauri/src/read.rs` since 2026-10-01), and the TypeScript that reads them is generated from
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
cargo audit && cargo deny check && cargo machete
```

Coverage is run from the root, for both crates at once and held to floors:
`pnpm coverage` (`scripts/coverage.sh`, see `docs/TESTING.md`).

`src-tauri/` takes the same battery. It holds **40 tests**, all of them in
`src/lib.rs` – counted 2026-10-01 with `cargo nextest list --run-ignored all`,
the same way `docs/TESTING.md` counts the console crate. Twelve are in `wire`
(the generated types the window receives, and the promises about what never
crosses), seven in `counting`, eight in `markup` (the manifest summary and its
refusals, and every name it gives reaching the window byte for byte), nine in
`opening` (opening the inventory, the name a package must carry before it is
read, and that a link under that name, or a manifest that is not a plain file,
is refused), three in `cancelling` –
and one stands alone: `a_build_without_the_feature_registers_no_webdriver`,
the fourth and last of the gates keeping the end-to-end contour out of a
release: the other three are declarations that
`frontend/tests/spec-guard.test.ts` reads, and this one is the compiler's own
answer about what the builder registers when the feature is off.

Four of the forty carry `#[ignore]` and run only when asked explicitly:
`regenerate_the_bindings`, which writes `frontend/src/bindings.ts`, the two
`cancelling` tests that read the corpus archive, and the `markup` test that
reads a package built from it. Since 2026-09-24 the cancel from the window is
also proven in the ordinary run, on an archive the test builds. A default run
is **36**, and **35** under `--features e2e`, where the gate test is compiled out.

Two flags need care here. `nextest` runs **without** `--all-features`: that flag
turns on `e2e`, and the gate test is compiled out under the feature
(`cfg(all(test, not(feature = "e2e")))`) — the very thing it exists to check.
Clippy, by contrast, takes `--all-features`, because that branch must be checked
too. `--no-tests=pass` is no longer a necessity: the other thirty-nine tests sit in
modules gated on `test` alone and run under the feature as well. It stays as
insurance for a crate that temporarily has no tests at all.
