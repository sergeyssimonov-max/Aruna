/**
 * The TLH2 index is written in TypeScript and read in Rust, so the two halves
 * cannot share a module the way ARUN's writer and reader do. This reads the
 * constants straight out of `wasm/search/src/lib.rs` and checks the builder
 * agrees with them.
 *
 * It is here because the last drift was invisible in every other way: the
 * builder allowed 255 pool entries where the module accepted 64, and the only
 * symptom was search quietly falling back to the JavaScript scan once the
 * corpus grew past the smaller limit.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const rust = readFileSync(new URL("../../wasm/search/src/lib.rs", import.meta.url), "utf8");

/** Value of a `const NAME: type = value;` item in the Rust module. */
function rustConst(name: string): number {
  const m = new RegExp(`const\\s+${name}\\s*:\\s*\\w+\\s*=\\s*([0-9_a-fA-Fx]+)`).exec(rust);
  assert.ok(m, `wasm/search/src/lib.rs no longer defines ${name}`);
  return Number(m![1]!.replace(/_/g, ""));
}

test("the builder and the module agree on the TLH2 layout", () => {
  // Mirrors of the Rust side, as the builder in search-index.ts uses them.
  assert.equal(rustConst("MAGIC"), 0x32484c54, "magic");
  assert.equal(rustConst("HEADER"), 32, "header size");
  assert.equal(rustConst("GROUP_STRIDE"), 8, "group record");
  assert.equal(rustConst("ITEM_STRIDE"), 8, "item record");
  assert.equal(rustConst("DIR_STRIDE"), 4, "directory entry");
});

test("the pool cap the builder enforces is the one the module enforces", () => {
  const source = readFileSync(new URL("./search-index.ts", import.meta.url), "utf8");
  const m = /const MAX_POOL = (\d+)/.exec(source);
  assert.ok(m, "search-index.ts no longer declares MAX_POOL");
  assert.equal(
    Number(m![1]),
    rustConst("MAX_POOL"),
    "the builder would emit an index the module refuses to load",
  );
});
