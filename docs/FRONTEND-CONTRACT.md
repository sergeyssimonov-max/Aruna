# The frontend, and what the Rust core owes it

The agreed stack for the desktop application is fixed:

**Rust · Tauri 2 · Svelte 5 · Vite · TypeScript · no SvelteKit.**

Nothing of it exists yet. This document records what is actually in the
repository today, what the core is missing before a Tauri layer can sit on it,
and what the tests will check once there is something to test.

---

## 1. What is here today

`grep -ril 'tauri|svelte|sveltekit'` across the repository returns **nothing**.
There is no `src-tauri`, no `tauri.conf.json`, no `svelte.config.js`, no
`@sveltejs/kit`, no Svelte adapter, and no `svelte-kit` command.

So the "no SvelteKit" requirement holds today by absence. There is nothing to
remove and nothing to migrate away from — and no automatic check has been added
for it, because a check needs a `package.json` for the app that does not exist,
and creating one to satisfy a check is the wrong order.

**But there is a frontend**, and it is not this stack:

| | |
|---|---|
| what | a React 19 web application |
| framework | TanStack Start with TanStack Router, Nitro, Tailwind 4 |
| build | Vite 8, TypeScript 5.7 |
| where | `src/`, `vite.config.ts`, `package.json` at the repository root |
| state | working, tested (50 tests), typechecked, linted, deployed to Vercel |
| purpose | the public inventory site — the same catalogue, searchable in a browser |

`src/routes/` exists and belongs to **TanStack Router's** file-based routing. It
is not SvelteKit and must not be mistaken for it.

### The obstacle this creates

Two frontends will exist: a React web app that ships the catalogue to the public,
and a Svelte desktop app that drives conversion. They share the Rust parser
through generated data, not through code. Before the desktop app starts, decide:

1. **Do both stay?** Most likely yes — they do different jobs for different
   audiences. Then the repository needs two `package.json` files in separate
   directories and a build that does not confuse them.
2. **Is anything shared?** The catalogue format (ARUN v3) and the WASM search
   module could be. Svelte and React cannot share components.
3. **Whose `vite.config.ts` is at the root?** The web app's, today. The desktop
   app must not inherit it by accident.

None of this is decided here. It is written down so it is not discovered midway
through the Tauri work.

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

### 2.2 No progress

The core reports progress by printing to stderr — 17 call sites across six
modules (`lib.rs`, `cache.rs`, `download.rs`, `archive.rs`, `zenodo.rs`,
`export/mod.rs`). A GUI cannot show that, and a library that prints is a library
that cannot be embedded.

Needed: a progress sink passed in by the caller, with the CLI passing one that
prints exactly what it prints now. Behaviour-preserving, and the CLI tests
already pin the wording.

### 2.3 No cancellation

There is no cancellation anywhere in the crate. A download takes about a minute
and a package build about six seconds; both are unstoppable once started.

Needed: a cancellation token checked at document boundaries — between entries in
the archive loop, and between chunks in the download loop. Both are already
loops over bounded units, so the check has an obvious place to go.

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

## 3. The IPC contract, when it is written

Commands, as a first cut — names are part of the contract and must be versioned:

| command | in | out |
|---|---|---|
| `inventory` | archive or directory path | document count, groups, warnings |
| `preflight` | the same | per-document readiness, malformed list |
| `convert` | source, destination, options | run id |
| `cancel` | run id | acknowledgement |
| `report` | run id | per-document outcome, summary |
| `open_result` | path | acknowledgement |

Rules:

- The frontend never gets a raw filesystem handle. Paths cross as strings and
  are validated in Rust against a root the user chose through the system picker.
- Progress is an event stream, **aggregated**: one event per document across
  23 936 documents is 23 936 IPC round trips. Batch by count or by interval and
  say which.
- Errors cross as a tagged structure — kind, message, path, whether retrying is
  worthwhile — not as a rendered string.
- Nothing long-running touches the main thread.
- Cancellation is acknowledged and then confirmed, so the interface can
  distinguish "asked" from "stopped".

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

**No SvelteKit**: once a `package.json` for the desktop app exists, a test that
fails on `@sveltejs/kit`, on `svelte-kit` in any script, on a SvelteKit adapter,
on `src/routes` **within that app's directory**, and on any SvelteKit import.
The last one needs care: the React app's `src/routes/` is TanStack Router and is
legitimate.

---

## 5. Order of work

1. Progress sink and cancellation token in the core, behaviour-preserving, with
   the CLI as the first caller.
2. A DTO module with `serde`, and the error enum made serialisable.
3. Only then `src-tauri`, thin, with no Tauri type reaching the domain.
4. Then Svelte 5 + Vite + TypeScript, and the no-SvelteKit check with it.

Steps 1 and 2 are useful on their own and can be done before any decision about
the desktop application is final.
