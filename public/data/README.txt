ARUN v3 runtime data

  inventory.bin      raw binary
  inventory.bin.gz   preferred download (~gzip -9)
  inventory.json     catalog source (not fetched by the app)

Table: № | Siglum | Lang | Corpus | Editor | Year
Inv. numbers remain in the binary for search only.

Both files are generated from the corpus — nothing here is edited by hand. The
CLI's parser is the single source of truth, so the site and the CLI describe
the same manuscripts:

  cargo run --release --manifest-path cli/Cargo.toml \
    --example emit_inventory_json -- <TLHdig archive.zip> public/data/inventory.json
  npm run build:data

CI rebuilds both from the archive and fails if either differs from what is
committed, so the catalog cannot drift away from the parser again.
