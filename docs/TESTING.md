# Testing

What exists, how to run it, and what each profile is for.

The suite is **271 tests** across eight integration binaries plus the library and
the binary's own tests. It runs in about
ten seconds and needs no network. Retries are deliberately absent from
`.config/nextest.toml`: a flaky test is a defect to find, not a wait to sit out.

---

## Profiles

### Fast — about 12 s

Formatting, compilation, and everything that does not touch the corpus archive.

```sh
cd cli
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo nextest run --profile ci -E 'not binary(corpus)'   # 269
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
ARUNA_REQUIRE_FIXTURE=1 cargo nextest run --profile ci -E 'binary(corpus)'   # 2
cargo run --release --example corpus_inventory -- fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip
cargo run --release --example verify_normalization -- fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip
shasum -a 256 fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip
```

Use `ARUNA_ZIP=/path/to.zip` to point at an archive elsewhere.

### Stress — about 20 s

Documents built to break the reader rather than to be read: 50 000 levels of
nesting, 100 000 attributes on one element, an 8 MiB text node, a ZIP bomb that
inflates past the 64 MiB per-document ceiling, a redirect loop, an archive that
names one entry twice.

```sh
cd cli
cargo nextest run --profile ci -E 'binary(xml_hostile) + binary(export_hostile)'   # 19
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

### Supply chain

```sh
cd cli
cargo audit
cargo deny check
cargo machete
```

### Future PDF acceptance / future frontend

Neither component exists. See `PDF-ACCEPTANCE.md` and `FRONTEND-CONTRACT.md`
for the criteria they will be held to. **No placeholder tests were written for
them**: an `#[ignore]` that never runs is not coverage, and a fake converter
built to satisfy a test is worse than no test.

---

## What the tests are grouped into

| binary | tests | what it holds |
|---|---|---|
| library and `bin/aruna` | 206 | parsing, scanning, naming, ordering, the catalogue, MD5, the export's pure halves, and which failures get advice |
| `tests/integration.rs` | 6 | archive to HTML, malformed input, the corpus if present |
| `tests/cli_process.rs` | 13 | the binary as a child process, cache versus network |
| `tests/cache_lifecycle.rs` | 8 | the cache against a local HTTP server: redirects, loops, failures |
| `tests/export_integration.rs` | 8 | the export against an archive shaped like the corpus |
| `tests/export_hostile.rs` | 10 | archives written to break the export |
| `tests/xml_contract.rs` | 9 | the fixture set: immutability, the permit list, field extraction |
| `tests/xml_hostile.rs` | 9 | XXE, entity expansion, external DTD, XInclude, resource exhaustion |
| `tests/corpus.rs` | 2 | the whole archive: non-distortion, no writes, the malformed count |

Fixtures are described in `cli/fixtures/xml/MANIFEST.md` with a SHA-256 for each.

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
```

Baselines are in `PERFORMANCE.md`.

---

## Rules these tests keep

- No test writes to the corpus, to `~/Downloads`, or to any user file. `HOME` is
  overridden for the ones that would.
- No test reaches a production service. The only network is a local server bound
  to port 0, so concurrent runs cannot collide.
- No `sleep` is used for synchronisation.
- No random seed is unfixed: the fuzz harnesses use a constant seed and print it.
- Temporary directories are removed by `Drop`, including on failure.
- The heavy fixtures are generated inside the test that needs them, not
  committed: their size is the point and the repository is not the place for it.

---

## Environment limits worth knowing

- `wasm/search` pins toolchain 1.97.1 with `profile = "minimal"`, so `clippy` is
  not installed for it. Its 22 tests run; its lints do not. Installing the
  component is a change to the machine, not to the project.
- `cargo test --doc` reports 0 tests: the crate has no doc examples.
- No PDF tool (`qpdf`, `pdfinfo`, `pdftotext`, `pdffonts`, `mutool`) is
  installed. Nothing needs one yet.
