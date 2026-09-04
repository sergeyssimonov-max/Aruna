# The frontend, and what the Rust core owes it

The agreed stack for the desktop application is fixed:

**Rust · Tauri 2 · Svelte 5 · Vite · TypeScript · no SvelteKit.**

As of **2026-08-23 it exists**, at the repository root, and it is the only
frontend. This document records what is there, what the removal of the previous
one took with it, and what the core still owes the new one.

---

## 1. What is here today

| | |
|---|---|
| where | `frontend/` (Svelte 5, Vite, TypeScript) and `src-tauri/` (Tauri 2) |
| package manager | pnpm only, pinned by `packageManager` in the root `package.json` |
| entry | `src-tauri/src/main.rs` → `aruna_desktop_lib::run()` |
| plugins | dialog, opener, store, window-state, log |
| checked by CI | `pnpm check`, `pnpm lint`, `pnpm format:check`, `pnpm test:unit` and `pnpm build` in `frontend/`; `cargo fmt`, `clippy --all-features` and `nextest` for `src-tauri/`, on macOS because that is the only platform this targets; `cargo deny` runs once, in the Ubuntu `check` job, because the workspace is one dependency graph judged by one policy |
| bundle | `pnpm build` targets `universal-apple-darwin` — Intel and Apple Silicon in one binary |

There is no SvelteKit: no `@sveltejs/kit`, no adapter, no `svelte-kit` command.
The requirement holds by absence, as it always has.

`src-tauri/tauri.conf.json` drives the frontend itself — `beforeDevCommand` and
`beforeBuildCommand` run `pnpm --dir frontend`, and `frontendDist` points at
`../frontend/dist`. Those three lines are the whole coupling between the two
directories.

### The target state, agreed 2026-08-23

Everything in this repository is aimed at one description. When a change is
proposed, this is what it is measured against.

**The program does what v1.0.9 does, plus the export the 2.x line added.** The
single HTML inventory with its six columns and its filter, and the corpus
written out as folders of documents — one HTML file in the whole package, and
that is the inventory. This sentence used to end *"with a `CTH N/index.html` in
each"*; those pages were abolished on 2026-08-23 (see *No page for a CTH
folder*), and the inventory has linked straight at every `.xml` ever since. The
standalone copy the program also wrote into `~/Downloads` went in 2.3.0, for the
same reason in miniature: two documents of one name, and the one without links
opened first. Nothing is to be rolled back to
reach this: the inventory's behaviour *already is* 1.0.9's — the columns are the
same six, and the client script was **byte-for-byte identical** to the file at
that tag until it was rebuilt in the new stack on 2026-08-23, behaviour
unchanged and checked against the whole corpus (see *The client script*, below).
(There is no build "1.9.0"; the tags are `v1.0.5`, `v1.0.9`, `v2.5.1`, and
`v1.0.9`'s manifest reads `version = "1.0.9"`.)

**Svelte 5 · Vite · TypeScript is the only frontend technology in the
repository — including for what the program writes out.** Not just the
application window: the inventory file is to be authored in the new stack too.
It is the **only** document the program writes — see *No page for a CTH folder*
below.

**How, and this is the constraint that decides the shape:** Vite builds the
markup, styles and client script **at build time**, and the result is compiled
into the Rust binary the way the stylesheets are compiled in today. Rust substitutes the data at run time. Node is never needed to export
anything, and `aruna` stays a self-contained binary — which is the whole premise
of the `.app` and the DMG.

**React is already absent from everything the program produces**, and always was:
React was the website, which is gone — and since 2026-08-23 that is a test
rather than a memory: `frontend/tests/one-frontend-stack.test.ts` fails if React
appears in either manifest, in a lock file, in `node_modules`, in a source file
or in an artifact the crate carries, and fails on SvelteKit in the same breath. So this is not a removal. It is a
replacement of hand-written generation — `cli/src/html.rs` (markup assembled
with `write!`), four embedded stylesheets, and 216 lines of vanilla
`html_filter.js` — with output from the new stack. **All three parts are done**:
the script and the styles on 2026-08-23, the markup on 2026-09-01.

**What the replacement has to keep, because the current generator guarantees it
and the tests hold it to that:**

| property | held by |
|---|---|
| the inventory is one self-contained file that opens offline | `export/inventory.rs`, and the point of the artifact |
| two builds are byte-identical | `cli/tests/reliability.rs` |
| the cuneiform font stack reaches the page | `frontend/src/inventory/canonical.css`, `frontend/tests/font-stack.test.ts` |
| a print stylesheet | `frontend/src/inventory/print.css` |
| every row links straight at its XML file | `cli/src/presentation.rs`, `export/inventory.rs` |
| no `index.html` is written anywhere | `cli/tests/package_pages.rs`, `export/validate.rs` |
| HTML and JSON escaping | `escape_html`, `json_str` |
| the search, the aliases and the folding behave as they did | `frontend/src/inventory/filter.test.ts` |

Determinism is the sharp one: a bundler that stamps a hash, a date or a
non-deterministic chunk order into the output breaks `reliability.rs`, and that
test exists because a corpus artifact nobody can reproduce is not evidence of
anything.

### The client script and the stylesheet — built by Vite, 2026-08-23

**Done — two of the three parts.** The 216 lines of vanilla JavaScript that sat
in `cli/src/html_filter.js` and the four hand-written stylesheets are now
sources under `frontend/src/inventory/`, built by Vite, and compiled into the
binary out of `cli/src/generated/`. The markup followed on 2026-09-01 — see
*The markup*, below.

| | |
|---|---|
| sources | `filter.ts` and `main.ts`; `canonical.css`, `screen.css`, `print.css` — all in `frontend/src/inventory/` |
| build | `pnpm build:inventory` in `frontend/`, options in `build/inventory.ts` |
| artifacts | `cli/src/generated/{inventory_filter.js,canonical.css,screen.css,print.css}`, **committed**, never edited by hand |
| compiled in by | `html::INVENTORY_SCRIPT`, `style::{SHARED,INVENTORY,PRINT}` |
| behaviour | `frontend/src/inventory/filter.test.ts`, 14 tests against the document the artifacts describe |
| the artifacts are current | `frontend/tests/inventory-artifact.test.ts` |

**The three sections stay three files.** The order they are emitted in *is* the
cascade, and `style.rs::join` is the one place that decides it; bundling them
into one stylesheet in the build would move that decision somewhere the reason
for it is not written down. It is also the shape the architecture drawing asks
for — a self-contained document carrying canonical CSS, screen rules and
print/PDF rules as three embedded things.

**Stating the Lightning CSS targets is what makes the floor apply at all.**
Vite resolves the transformer's *compilation* targets to its own baseline —
Chrome 111 and its contemporaries — whenever `css.lightningcss.targets` is
unset, and `build.cssTarget` reaches Lightning CSS only while it is minifying.
The window is fine on that path, because it does minify: its `build.target` of
`safari16` becomes `cssTarget` and lowers at minify time, which is what §2 of
the specification describes. This build does not minify, so the explicit
targets in `build/inventory.ts` are load-bearing rather than decorative.

**The exported document has its own engine floor, and it is not the window's.**
`vite.config.ts` builds the window for `safari16`, because that is the WKWebView
macOS 13 ships and never updates. `build/inventory.ts` builds this document for
**Safari 13**: it travels with the corpus and is opened by a reader on a browser
nobody here chose, and copying the window's floor would make the corpus's reach
a side effect of one laptop's operating system. The number costs almost nothing
— built at 11 through 15 the sections come out byte-for-byte identical, and the
floor decides exactly one declaration, `-webkit-appearance` on the search field,
which a floor of 16 drops as redundant.

**What Lightning CSS changed in the output, and why each is the same
stylesheet.** Sixty-four rules were compared declaration by declaration before
and after; five differences, all of them equivalent spellings:
`::before` → `:before` (the older syntax, understood everywhere),
`transition: transform .12s ease` → `transform .12s` (`ease` is the initial
value), `flex: 0 0 auto` → `flex: none` and `flex: 1 1 16rem` → `flex: 16rem`
(both are what the shorthand means). Everything else is formatting: leading
zeros dropped, declarations in Lightning CSS's canonical order, one-line rules
opened out. The comments no longer travel — as they never did — but a parser
drops them now instead of `style.rs`'s own scanner, which is why the two tests
about `content: "/*"` and unterminated comments went with it.

**Why the build products are committed.** `cargo build` must never need Node —
that is the premise of the `.app` and the DMG, and a `build.rs` calling `pnpm`
would end it. The cost of a committed artifact is that it can go stale, or that
someone edits it instead of the source; `inventory-artifact.test.ts` rebuilds
all four into a temporary directory and fails if any byte differs, so that cost
is a failing test rather than a mystery inside an exported document. It also
builds twice and compares, which is the same reproducibility `reliability.rs`
demands of the exporter, asked of the bundler — and it checks that none of the
bundler's own leavings reach the corpus: the `$vite$` marker Vite appends to a
bundled stylesheet, the chunk lib mode writes beside one, and the `light-dark()`
scaffolding Lightning CSS declares for `color-scheme` and nothing reads.

**How the build is kept deterministic**: fixed file names rather than hashed
ones, no minification anywhere, one IIFE with no exports. The script keeps the
source's comments, so a reader who opens the document still finds out why a row
is searched by its cells and not by its `textContent`; the stylesheet does not,
because it never did.

**Evidence that behaviour did not change**, beyond the unit tests: the real
corpus was exported both ways and driven in jsdom — 23 936 rows in 663 groups,
`schwemer` → 91, `ds` → 1 150, `CTH 16` → 65, fold-all → 0 shown, one heading
folded → 23 927. Identical, query for query, old script and new.

One comment was corrected on the way: the alias table said an agreement test
held it level with the website's search index. Both went with the React site on
2026-08-23, and this is now the only list of editor spellings in the repository.

### The markup — built by Vite, 2026-09-01

**The third and last part.** `cli/src/html.rs` assembled the document with
`write!` — 499 lines of it, and every tag in the exported inventory was a Rust
string literal. The page is now authored in `frontend/src/inventory/` as Svelte
components, rendered once at build time by `render()` from `svelte/server`, and
compiled into the binary out of `cli/src/generated/` on exactly the terms the
script and the stylesheet already had. Nothing in those components is ever
mounted or hydrated: this is a template engine whose templates happen to be
type-checked, formatted and linted like the rest of the frontend, and that is
the whole reason for authoring them in Svelte rather than in a string.

| | |
|---|---|
| sources | `Document.svelte` and seven fragments — `GroupHeading`, `ManuscriptRow`, `ManuscriptLink`, `ColumnWidth`, `ColumnHeading`, `LegendEntry`, `GeneratedLine` |
| build | the same `pnpm build:inventory`, a second plugin beside the stylesheets' |
| artifacts | `cli/src/generated/{document,group_heading,manuscript_row,manuscript_link,column_width,column_heading,legend_entry,generated_line}.html` |
| compiled in by | `html::{DOCUMENT, GROUP_HEADING, …}` |
| what Rust does now | substitution, repetition and escaping — `html::fill` and the callers around it |

**`Document.svelte` is the whole page, `<html>` to `</html>`.** Svelte renders
`<html>`, `<head>` and `<body>` as ordinary elements, and a `<style>` or
`<script>` element written inside the body comes out verbatim, with the
component's own `<script lang="ts">` still recognised as its instance script.
The one thing it cannot say is the doctype — a literal `<!DOCTYPE html>` is
parsed as an element named `!DOCTYPE` with an attribute `html` and rendered back
as `<!doctype html=""/>` — so the component emits it through `{@html}`, which is
the only way, and the artifact still begins at the doctype.

**A hole is a prop, and its name is the prop's name upper-cased.** The build
renders each component with every prop set to `@@PROPNAME@@` and checks, before
writing, that each one reached the output; `html.rs` fills them by the same
names, and `no_placeholder_survives_into_a_document` checks that none is left.
That is two lists across a language boundary, and those two tests are what keep
them level — the alternative was a rename producing a valid document with a
sentence missing from it.

**Markup slots are `{@html}`, text slots are `{expr}`, and that is not a
preference.** Svelte rejects a text expression as a child of `<colgroup>`,
`<thead>`'s `<tr>` or `<tbody>` — `node_invalid_placement`, because a browser
would repair the HTML — and those are exactly the slots that receive markup the
crate assembled. Text slots stay expressions so Svelte escapes them; markup
slots carry text that went through `escape_html` before it got there.
`eslint.config.js` turns off `svelte/no-at-html-tags` and
`svelte/no-raw-special-elements` for this directory alone, with the reason
written where the override is.

**No `{#if}` and no `{#each}` anywhere.** How many rows, groups and columns
there are is known when a corpus is exported, not when the components are built,
so repetition is the crate's; and the crate's `COLUMNS` stays the one
declaration of the six columns, which a Svelte template listing them again would
quietly become the second of. Control flow would also leave Svelte's hydration
markers in the artifact, and the build strips every comment on the way out
precisely because those markers are not stable: the fence around `{@html}` came
out `<!---->` from `pnpm build:inventory` and `<!--12ftosl-->` from the same
build under Vitest until `mode` and `dev` were pinned.

**Where a list breaks lines is the document's decision, read off the document.**
`html::separator` looks at what precedes a placeholder: one that stands on a line
of its own — the rows, the legend — gets one fragment per line indented to match,
and one inside a line — `<colgroup>@@COLGROUP@@</colgroup>` — gets its fragments
run together, because that is what the markup asked for. Nothing about the shape
of the page is decided in Rust.

**Evidence that the document did not change.** The real corpus was exported by
the binary before the change and after it, from one archive, and the two were
compared as documents rather than as bytes: 24 599 rows — 23 936 manuscripts in
663 groups — identical row for row, cell for cell, href for href, along with the
title, the summary line, the legend, the `<colgroup>`, the `<thead>` and every
control in the toolbar. The only difference is three bytes of whitespace at the
end of the `<style>` and `<script>` blocks. Both documents were then opened in
jsdom and driven through their own script: `schwemer` → 91, `ds` → 1 150,
`CTH 16` → 65, `fuscagni` → 27, fold-all → 0, one heading folded → 23 930,
identical throughout. The file is 19 % smaller, because a row is one line now
rather than eight.

### CI does not build the application, and that is the decision

**2026-08-23, superseded 2026-08-29.** The rule read: no CI job compiles
`src-tauri`, and none is to be added yet — what CI controls at this stage is the
correctness of the frontend stack, and nothing beyond it. The reason given was
what the application then *was*: Rust logic with no interface on top, so
compiling a shell around it in CI would spend macOS minutes proving that a
scaffold still links.

The condition it named — «when there is an interface to break, the job is worth
adding» — has been met. A `desktop` job runs the specification's §6.1 battery for
the crate on macOS since 2026-08-29, and on 2026-08-30 the shell stopped being a
scaffold: it depends on the core and calls it. The gap is closed rather than
still deliberate.

The bundle *has been* built and checked by hand — the last such verification, on
2026-08-30, produced a Universal `.app` and `aruna_0.2.0_universal.dmg`, with the
hashed `frontend/dist` assets present in both architecture slices, and confirmed
that the version in that name comes from `src-tauri/Cargo.toml` and from nowhere
else: the `version` field was taken out of `tauri.conf.json` the same day, so the
two can no longer disagree.

**The image is not built again until it is wanted.** The owner said so on
2026-08-30, and §6.7 of the specification is the reason it costs nothing: what
users receive is built from the core, so a disk image of the shell is a five
megabyte artifact nobody installs. `bundle.targets` still names `dmg` — the
configuration is what a release would use, not an instruction to produce one now
— and routine checking of the shell goes through
`cargo tauri build --debug --no-bundle`, which is what the E2E run already does.
Ask before bundling.

### No page for a CTH folder — decided 2026-08-23

**The exporter wrote an `index.html` into each of the 663 CTH folders, and the
inventory's group headings linked at them. Both are gone, and neither is to come
back.** The user's decision, in their words: *«убрать все файлы index.html из
всех папок CTH и гиперссылки с группировки CTH; ссылки только на сами фрагменты
… никогда больше их не генерируй — отказываемся от этого функционала.»*

What the inventory does now: a CTH label is **plain text inside its fold
button**, and every row links straight at the manuscript's own XML file —
`./CTH%205/KBo%201.1.xml`. Grouping, counts and folding are untouched; only the
destination is gone.

**Grouping was never the thing being removed.** The rows still group by CTH, and
the fold control still works — that half of the heading is exactly the shape
the client script was written against, and its behaviour is byte-for-byte what
it was at v1.0.9 even though the file is no longer.

**The consequence, stated rather than discovered later:** clicking a manuscript
opens **raw XML** in the browser. The rendered reading page is the functionality
being given up, which is what was asked for.

**It is a tested property, not a convention.** `package_pages.rs` fails if an
`index.html` appears anywhere in a package or if the inventory names one;
`export/validate.rs` refuses such a package outright, at the root and inside a
group alike. Deleting those tests to "clean up" would be undoing the decision.

What went with the feature: `render_group_index`, `group_css()` and
`style_group.css`, `GROUP_INDEX`, `Hrefs::from_group` and
`GroupPresentation::index_href`, `Validation::group_links` and `Built::group_links`,
the `.group-head` and `a.group-label` rules in the screen section — which returns
that stylesheet to the shape it had at v1.0.9 — and the eight tests in
`package_pages.rs` whose subject was the CTH pages.

Verified against the real corpus on 2026-08-23: 663 groups, 23 936 documents,
**23 936 links and every one of them an `.xml`**, one HTML file in the whole
384 MB package, zero validation errors.

### The React application was removed on 2026-08-23

It was a React 19 web application on TanStack Start — the public inventory site,
the same catalogue searchable in a browser — and the decision to retire it was
taken on 2026-08-21. It is gone, together with everything that existed only to
serve it:

| removed | what it was |
|---|---|
| `src/` | the React application, its routes, components and hooks |
| `wasm/search/` | the Rust → WASM search engine, 947 lines and 22 tests |
| `src/data/`, `src/wasm/search.wasm` | the ARUN container and the compiled module the site downloaded |
| `scripts/` | the ARUN builder, the browser smoke test and the Node tests beside them |
| `public/`, `screenshots/`, `startup.sh` | site assets and its dev entry point |
| `vite.config.ts`, `tsconfig.json`, `eslint.config.mjs`, `.prettierrc`, `package-lock.json` | the root configuration, all of it the web app's |
| `cli/tests/catalog_roundtrip.rs` | it staged a tree from `scripts/` and `src/lib/` and read the container back with the site's own reader; with all three gone it had no subject |
| the `web` CI job, the wasm artifact check, two `corpus` steps | everything in `.github/workflows/release-dmg.yml` that checked the above |

The root `package.json` was replaced rather than deleted: it is now the Tauri
app's, holding `@tauri-apps/cli`, the pnpm pin and four scripts.

**Two obligations did not survive the removal. Both are settled as of 2026-08-23.**

1. **The agreement tests — one of four rebuilt.** Four tests on the React side
   read Rust source and held the two languages to the same facts: columns,
   corpus authors, editor aliases, and the TLH2 format. They went with
   `src/lib/`.

   The seam that mattered most has a guard again:
   `frontend/tests/font-stack.test.ts` reads `frontend/src/inventory/canonical.css` and
   `frontend/src/app.css` and fails if the cuneiform stack in one is not
   mirrored in the other — in composition, in order, and in being referenced by
   all three of the frontend's font variables. It runs as `pnpm test:unit`, in the
   `Check + build (frontend)` CI job, and it is mutation-tested: dropping a face,
   leaving `--corpus` defined but unused, and reordering the Rust side each make
   it fail.

   **The other three need no test yet, and may never need one.** They guarded a
   *duplication*: the React application re-declared columns, corpus credits and
   the TLH2 format in TypeScript, so the two languages could drift. The Svelte
   application declares none of them — grepping `frontend/src` for columns,
   editors, corpus or TLH2 returns nothing but the font comment in `app.css`.
   `COLUMNS` in `cli/src/html.rs` is the single declaration — and stayed one when
the markup moved to Svelte on 2026-09-01: the `<col>`, the `<th>` and the legend
line are fragments the crate repeats per column, not a list a template restates.

   So the obligation is conditional, and the condition is worth stating: it
   returns only if the desktop application **re-declares** one of these facts. If
   it asks the core for them over IPC — which is what this document has argued
   for from the start — the duplication never exists and there is nothing to hold
   in agreement. Prefer that. Write a mirror test only if a mirror is created on
   purpose.
2. **`scripts/readme-links.test.mjs` — rehomed.** It checked that the links in
   `README.md`, `cli/README.md` and `PERFORMANCE.md` resolve; it was never about
   the site and merely shared a directory with it. It now lives as
   `frontend/tests/readme-links.test.ts` in the `node` vitest project, beside the
   font-stack check, and covers three more documents than the original —
   `docs/FRONTEND-CONTRACT.md`, `docs/TESTING.md` and `docs/FONTS.md`, all of
   which the removal rewrote. It reports `file:line → target` and is
   mutation-tested: a link reinstated into the deleted `src/` fails it.

**What stayed, and the question that turned out not to exist.** This document
previously asked whether the ARUN *container* code in `cli/src/catalog.rs` should
follow the site out. **There is no such code.** `catalog.rs` emits JSON and
nothing else — no binary packing, no gzip; the container was built entirely by
`scripts/build-inventory-bin.mjs`, which is already deleted. The question was an
error in this document, not an open decision.

What remains is a JSON catalogue with three callers inside the crate
(`emit_inventory_json`, `fuzz_layers`, `catalog_contract.rs`) — a reasonable
machine-readable export independent of any browser, kept for that reason. Its
comments and the test's assertion messages named the deleted script throughout
and were rewritten on 2026-08-23 to describe the format rather than a reader
that no longer exists — and again on 2026-08-25, for two the first pass had
missed: a test's doc comment and the failure text beside it still said the
script *refuses* a label, in the present tense, which is a diagnosis pointing at
a file the reader cannot open.

**Vercel.** The site was deployed from `src/`. Removing the source does not take
a running deployment down by itself, but it orphans the project and the next
push to a watched branch fails. Retiring that project is an act on the Vercel
account, outside this repository, and it has **not been done**.

---

## 2. What the Rust core is missing

The core is a good starting point in one important way: **every operation runs
without a GUI and is already tested that way.** `aruna::run()` takes an optional
path and returns a `Result<PathBuf>`; `export::build()` takes an archive, a
destination and a label. Neither needs a window.

Four things are missing, and each is a real piece of work rather than a
formality.

### 2.1 No `serde`

Dependencies are `dirs`, `memchr`, `thiserror`, `ureq`, `zip`. Tauri's IPC needs
`Serialize`/`Deserialize` on everything crossing the boundary.

This is deliberate today — the crate hand-writes both JSON documents it produces
rather than carry a serialisation dependency. Introducing `serde` for the DTO
layer is defensible; letting it spread into `ManuscriptRecord`, `Placed` and the
rest is not. **Put the DTOs in their own module and derive there.**

### 2.2 Progress — done

The core used to report progress by printing to stderr: 17 call sites across six
modules (`lib.rs`, `cache.rs`, `download.rs`, `archive.rs`, `zenodo.rs`,
`export/mod.rs`). A GUI cannot show that, and a library that prints is a library
that cannot be embedded.

`cli/src/progress.rs` now holds the sink. `Event` is one variant per stage,
carrying the numbers and paths rather than a formatted line; `Progress` is the
one-method trait (`Send + Sync`, `&self`) that receives them. Two
implementations ship: `Stderr`, which prints exactly what the 17 printed, and
`Silent`, which the test suite runs with.

The sink is threaded as an explicit last parameter — `run`, `obtain_archive`,
`download_verified`, `download_file`, `parse_zip`, `parse_zip_timed`,
`export::build` — rather than installed globally, because two conversions
running at once in one process must not share it.

Two of the 17 never got a sink at all: `cache::lookup` returns a `Lookup`
saying *why* it missed, and `Replaced::committed` hands back the path it could
not remove. Both facts are reported by the caller that has the sink, which keeps
`cache.rs` and a `Drop` guard free of any opinion about who is listening.
`zenodo::report` is gone; `zenodo::advice` composes the prose and the caller
says it.

Behaviour is unchanged: `Event`'s `Display` is the CLI's wording, pinned string
by string by `the_wording_is_what_the_core_used_to_print`, and the CLI process
tests still read the same lines off stderr.

What a Tauri command implements is `Progress` over an `ipc::Channel` — one
`report` that maps the variant to a serialisable DTO and sends it. `Event` is
deliberately not `#[non_exhaustive]`, so a new stage is a compile error in the
frontend layer rather than a silently dropped line.

### 2.4bis Presentation model — done

**2026-08-21.** `cli/src/presentation.rs`. `CorpusPresentation` → `GroupPresentation`
→ `FragmentPresentation`: what a reader is shown, decided once and rendered
many times.

It absorbs the three decisions the renderers each used to make on their own —
which of a manuscript's two names to print, where a link points *from the page
that carries it*, and which facts a row shows when some are absent. Those are
not properties of a manuscript and not properties of HTML, and having them in
two places is how the inventory and the CTH pages came to name and link the same
document slightly differently.

It does **not** absorb escaping (the renderer's, and different for text,
attribute and URL), styling, or counts that are a `len()`.

`the_renderers_take_their_decisions_from_here` pins the separation as a property
of the source rather than of one document's output: `html.rs` and
`export/inventory.rs` may no longer contain `cth.is_some()`, `!= MISSING`,
`href(&`, `GROUP_INDEX` or `group_runs(` outside their test modules.

**Borrowed, not owned**, and this is the part that matters for the DTO. The
corpus is 23 936 records whose strings are already in memory; an owning
presentation would allocate tens of megabytes to say the same thing twice. The
lifetimes suit HTML and suit a future PDF — same process, same pass over the
same data. They do **not** suit IPC: a value crossing a process boundary has to
outlive the call. So the Tauri DTO is a separate owning type built from this one
by copying, in its own module where `serde` is derived without spreading inward
(§2.1). **This model is not IPC-ready as it stands, and is not meant to be** —
it is the layer the DTO will be built from.

Verified behaviour-preserving rather than assumed: the package rebuilt after the
refactor is byte-for-byte the one built before it — 24 601 files, zero
differences.

### 2.3 Cancellation — done

**2026-08-21.** `cli/src/job.rs`. `Cancel` is an atomic flag with cloneable
handles; `Job` carries it together with the progress sink and a `JobId`, and is
passed as the one parameter a long operation takes from its caller.

Checked between units of work, never inside one: between archive entries,
between documents, between 64 KiB download chunks, and before each download
attempt. The backoff between attempts is slept in slices so a cancelled run does
not sit out sixteen seconds nobody is waiting for.

A cancellation travels as `ArunaError::Cancelled { phase }` — an outcome rather
than a fault. It is an error so that it propagates out of a loop nested five
calls deep without every function between growing a third return case, and
because the cleanup a `?` triggers on the way out is exactly the cleanup a
cancelled run needs: the staging directory removes itself because it was never
published, and the scratch file because it was never committed. Stopping and
failing leave the same thing behind, by the same mechanism.

`tests/cancellation.rs` (9 tests) asserts what actually matters: a cancelled
build publishes nothing, a cancelled *rebuild* leaves the package the reader
already had byte for byte, a late cancellation does not undo a finished run, and
a run that is never cancelled produces exactly the package an unattended one
does. Cancellation is triggered from a progress sink at a named stage rather
than from a timer, so no test here depends on how busy the machine is.

The CLI creates a handle and never sets it. Ctrl-C remains the terminal's
answer; the flag exists so the same core can be driven by a window with a Cancel
button.

### 2.5 Application services — done

**2026-08-21.** `cli/src/app.rs`. The scenarios a front end invokes, named once,
each taking a typed request and a `Job` and returning an owned report. Today
they are `build_corpus` — the whole run, which is what the binary drives — and
`build_package`, the export under it. There was a third, `build_inventory`, for
the standalone inventory the program wrote beside the package; 2.3.0 gave that
artifact up and the scenario with it, rather than leave a public function with
no caller.

This is the layer where borrowing stops. Everything below it borrows because it
runs in one pass over data the caller holds; a report outlives the call and a
window's copy has to survive the archive being dropped.

`Failure` is the error as a front end can act on it: a stable machine code
(`network`, `checksum`, `collision`, `cancelled`, …), the phase, one sentence,
`retryable`, and `cancelled`. The retryable flag is the distinction that matters
— a busy server is worth another attempt, a checksum mismatch is not, and an
interface offering *Retry* for the second would invite someone to download
71 MiB to get the same answer.

`main.rs` calls through this layer rather than past it. A binary that reached
directly for `aruna::run` would be the second answer to "what does building the
inventory come to", which is what the layer exists to prevent.

**Still not serialisable, on purpose.** Deriving `serde` here is one line per
type plus a dependency, added when there is something to serialise for. Adding
it now would be a dependency with no consumer, and §2.1 above is the standing
decision that it must not spread inward past this module.

### 2.4 Single-threaded

There is one `thread::spawn` in the whole crate and it is in a test helper. That
is not a fault — the export is I/O-bound and takes 2.7 s of user CPU — but there
is no bounded-concurrency mechanism to build on either. If conversion turns out
to need parallelism, the limit must be explicit from the first line.

### What is already right

- Errors are a single `ArunaError` enum with `thiserror`, carrying paths and
  causes. It maps cleanly to a structured IPC error; it needs `Serialize` and a
  stable discriminant, not redesigning.
- Destination checking already refuses a path that is not the exporter's own,
  refuses a symlink, and never follows one.
- The write path is atomic and self-cleaning (`Staging`, `Replaced`,
  `paths::write_atomic`).
- Bounds exist and are tested: 1 GiB per download, 64 MiB per document, a
  bounded directory walk.

---

## 3. The IPC contract, as it was written

Written on 2026-09-02, when the window stopped being a viewer over
`~/Downloads` and became the program's front door. Four commands and one event,
and the names are part of the contract:

| command | in | out |
|---|---|---|
| `corpus_location` | — | where the package and the inventory go, and whether they are there |
| `corpus_stats` | package path | counts, the CTH spread, the writing counters, and which of the two answered |
| `build_corpus` | an archive path, or nothing for the pinned Zenodo record | `BuildReport`, or `BuildFailure` |
| `cancel_build` | — | nothing; the confirmation arrives as the failure |

| event | payload |
|---|---|
| `build-progress` | `{ job, stage, done, total, manuscripts, groups, note }` |

**The types are generated, not written twice.** `frontend/src/bindings.ts` comes
out of the same declarations that register the commands — specta 2.0.0-rc.25,
tauri-specta 2.0.0-rc.25, specta-typescript 0.0.12, pinned exactly because that
family has no stable release for Tauri 2 and §3.5 of the specification
anticipated it. The file is committed and a Rust test rebuilds it and fails on
any difference, the same discipline the inventory's artifacts live under; it is
not written at start-up, because a debug run would then edit the working tree.
`cargo test -p aruna-desktop -- --ignored regenerate` refreshes it.

This closed a hazard that was named here in advance and had no test behind it:
Tauri expects a command's arguments in camelCase, and the first two-word
argument is where a hand-written call site silently stops matching. The first
two-word argument is `local_archive`, and it arrived with the generator that
spells it.

**The rules, and how each is kept:**

- *The frontend never gets a raw filesystem handle.* Paths cross as strings.
  `chosen_archive` is where one becomes a path, and a string with no file behind
  it is refused there — `archive_missing`, before anything starts.
- *Progress is an event stream, aggregated.* One event per document across
  23 936 documents would be 23 936 round trips; the core batches instead, and
  says which way: the transfer by interval (a quarter-second), the write by
  count (every five hundred documents, forty-eight messages for this corpus).
  The stage announces the denominator, the ticks fill the numerator in.
- *Errors cross as a tagged structure.* `BuildFailure` carries `code`, `phase`,
  `message`, `retryable` and `cancelled` — `app::Failure`'s five fields, one for
  one. The two reading commands keep a bare sentence, because each has exactly
  one failure and there is nothing to branch on; the build has twenty codes and
  the window's next move depends on which.
- *Nothing long-running touches the main thread.* `build_corpus` is `async` and
  the work is on `spawn_blocking`. `Job<'a>` borrows its sink and its flag, so
  it is built inside the worker; `Cancel` is `Clone` over an `Arc`, so the
  thread that stops the run is not the thread doing it.
- *Cancellation is acknowledged and then confirmed.* The click shows
  «Останавливаю…»; the state becomes stopped only when the failure with
  `cancelled: true` comes back. It is not instant and the window says so: the
  core cannot interrupt the Zenodo metadata request (up to 10 s), the archive's
  digest, or anything after the publish begins.

**Two things the wire refuses to carry.** No filesystem path reaches a message —
five `progress::Event` variants hold a `&Path` and none of it crosses, held by
`a_progress_event_never_carries_a_filesystem_path`; the one message that does
cross, a retry's, is taken from `app::Failure`, where the core has already
stripped it. And no count crosses as a 64-bit integer: specta refuses `usize` and
`u64` outright, because JSON carries numbers as doubles, so the wire declares
`u32` — which is what these counts are. Documents are bounded by
`archive::MAX_ENTRIES` (500 000), a download by the gibibyte the client will
accept, and a length above that is not shown as a denominator at all.

**`Stage` is a union, not a string.** Seventeen literals generated from a Rust
enum, so `svelte-check` fails when the core gains a stage and the window forgets
it — the same guarantee, one language further on, that `progress::Event` gets
from not being `#[non_exhaustive]`.

---

## 4. The tests that come with each component

None of these can be written yet, and none has been.

**Rust ↔ Tauri**: command registration; DTO serialisation both ways; error
serialisation; path validation; capability and permission scope; cancellation;
progress delivery; clean shutdown; nothing blocking on the main thread.

**Tauri ↔ TypeScript**: command names stable; types compatible; required versus
optional fields; enums; contract versioning; large numbers and awkward paths
surviving JSON.

**Svelte 5**: form state; file selection; error display; progress display;
cancellation; the run button disabled while running; partial-success results;
keyboard and screen-reader access; recovery after an error.

**Vite and TypeScript**: production build; `strict` on with no implicit `any`;
asset paths; no dev-server dependency in production; Tauri asset protocol.

**Smoothness**: no blocked interface during parsing or conversion; progress
updates evenly and not too often; cancel responds immediately; state correct
after cancellation; no double start; 23 936 rows without 23 936 DOM nodes;
logs bounded; no leaked event or Tauri listeners; no memory steps.

**No SvelteKit** — **written, 2026-08-23**, in
`frontend/tests/one-frontend-stack.test.ts`: it fails on `@sveltejs/kit` in the
manifest or in `node_modules`, on `svelte-kit` in any script, on a SvelteKit
adapter, on a `src/routes` directory and on any SvelteKit import. The caveat
this rule used to carry — that the React app's own `src/routes/` was TanStack
Router and legitimate — is spent: that application is gone, so a `src/routes`
anywhere is now unambiguous.

---

## 5. Order of work

1. Progress sink and cancellation token in the core, behaviour-preserving, with
   the CLI as the first caller.
2. A DTO module with `serde`, and the error enum made serialisable.
3. Only then `src-tauri`, thin, with no Tauri type reaching the domain.
4. Then Svelte 5 + Vite + TypeScript, and the no-SvelteKit check with it.

Steps 1 and 2 are useful on their own and can be done before any decision about
the desktop application is final.
