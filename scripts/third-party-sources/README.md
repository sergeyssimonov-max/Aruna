# Reviewed npm licence sources

`npm-reviewed.json` is an exact-version allowlist for six optional WASM build
packages introduced by Rolldown 1.2.19. It is not a general MIT fallback.
The notice generator works offline and checks every stored text's SHA-256.
Unknown versions remain unreviewed; conflicting installed declarations or
texts cause an error before the output is written.

Reviewed on 2026-11-16:

- `@emnapi/core` and `@emnapi/runtime` 2.0.0-beta.2,
  `@emnapi/wasi-threads` 2.2.0, and `@tybys/wasm-util` 0.10.4:
  exact registry tarballs, verified against their npm SHA-512 integrity;
  package.json declares MIT and the archived LICENSE supplies the text.
  The three emnapi texts are byte-identical.
- `@napi-rs/wasm-runtime` 1.2.6: the tarball declares MIT but contains no
  licence file. npm's `gitHead` is
  `4fff029f67693d08be0ff16818eb9ead2637a99e`; the manifest at
  `wasm-runtime/package.json` in that commit confirms name, version and MIT.
  Preserve the complete root LICENSE, including both copyright notices.
- `@rolldown/binding-wasm32-wasi` 1.2.19: the tarball declares MIT but
  contains no licence file. The registry's publication provenance names
  commit `b9c75ddc8bd50f2c1a4c7be0b548bdab5cba85ab`; its subject SHA-512
  matches the published tarball. The source package manifest confirms
  Rolldown 1.2.19 and MIT. Preserve both root LICENSE and the referenced
  THIRD-PARTY-LICENSE from that immutable commit.

Source URLs and text hashes are recorded in the JSON. Registry provenance
was read as source attribution; no independent Sigstore verification is
claimed. These are build-tool packages, not additions to Aruna's runtime
capabilities. Original licence bytes are never reformatted.
