ARUN v3 runtime data

  inventory.bin      raw binary
  inventory.bin.gz   preferred download (~gzip -9)
  inventory.json     catalog source (not fetched by the app)

Table: № | Siglum | Lang | Corpus | Editor | Year
Inv. numbers remain in the binary for search only.

These live under src/ rather than public/ on purpose. A file in public/ is
copied to the site verbatim, keeping the name it has here — and the app asks
for it with `cache: "force-cache"`, so a visitor who has one keeps it after a
deploy: same URL, and nothing tells the browser to ask again. src/lib/
load-inventory.ts imports the two binaries instead, and the build gives each an
emitted name containing a hash of its contents, so new data arrives under a new
URL and the file a visitor already holds can be kept forever.

inventory.json is here for the same reason inverted: nothing fetches it, and in
public/ it was published to the site anyway — 700 KB nobody downloads.

Both binaries are generated from the corpus — nothing here is edited by hand.
The CLI's parser is the single source of truth, so the site and the CLI
describe the same manuscripts:

  cargo run --release --manifest-path cli/Cargo.toml \
    --example emit_inventory_json -- <TLHdig archive.zip> src/data/inventory.json
  npm run build:data

CI rebuilds both from the archive and fails if either differs from what is
committed, so the catalog cannot drift away from the parser again.

That check needs the archive. scripts/inventory-data.test.mjs needs nothing:
it reads the three files as committed and asserts they describe one catalog —
including that the .gz decompresses to the .bin, which no other check looked
at, though it is the file every visitor downloads.
