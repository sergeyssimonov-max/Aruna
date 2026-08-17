/**
 * The corpus is credited to the same people, in the same words, in both
 * inventories.
 *
 * The page builds its credit line from `corpus-authors.ts`; the standalone HTML
 * the CLI writes builds its own from `CORPUS_AUTHORS` in `cli/src/lib.rs`. A
 * TypeScript module and a Rust constant cannot import each other, so this reads
 * the Rust one and checks the two name the same people, with the same cities,
 * in the same order.
 *
 * Order is part of the comparison, unlike the editor aliases next door. This
 * list is printed as a sentence rather than looked up, and it is printed in the
 * order the Zenodo record names its creators — a list that quietly reordered
 * itself on one side would be a different credit, not a different index.
 *
 * The seam is quiet when it breaks: both documents keep rendering, they just
 * credit the corpus differently, and only a reader holding both would notice.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { CORPUS_AUTHORS, corpusAuthorsLine } from "./corpus-authors.ts";

const LIB_RS = "../../cli/src/lib.rs";
const source = readFileSync(new URL(LIB_RS, import.meta.url), "utf8");

/** The pairs the Rust constant carries, as written. */
function rustAuthors(): Array<{ name: string; city: string }> {
  const declaration = /pub const CORPUS_AUTHORS: \[\(&str, &str\); \d+\] = \[([\s\S]*?)\];/.exec(
    source,
  );
  assert.ok(declaration, `${LIB_RS} no longer declares a CORPUS_AUTHORS constant this test can read`);

  const pairs = [...declaration[1]!.matchAll(/\("([^"]*)",\s*"([^"]*)"\)/g)].map((m) => ({
    name: m[1]!,
    city: m[2]!,
  }));
  assert.ok(pairs.length > 0, `${LIB_RS} declares CORPUS_AUTHORS in a shape this test cannot read`);
  return pairs;
}

test("both inventories credit the corpus to the same people", () => {
  assert.deepEqual(
    rustAuthors(),
    CORPUS_AUTHORS.map(({ name, city }) => ({ name, city })),
    `${LIB_RS} and corpus-authors.ts credit the corpus differently`,
  );
});

/**
 * Matching lists are not a matching credit.
 *
 * Each side assembles the sentence itself — `corpus_authors_line` in Rust,
 * `corpusAuthorsLine` here — from the same people. The test above compares the
 * people and would stay green through a separator changed on one side only, or
 * a bracket turned into a dash, which is a difference every reader holding both
 * documents would see and no test would.
 *
 * The shape is written out here rather than imported from either side, so this
 * checks the rule instead of asking one implementation to agree with itself.
 */
test("both inventories print the credit as the same sentence", () => {
  const expected = rustAuthors()
    .map(({ name, city }) => `${name} (${city})`)
    .join(", ");
  assert.equal(
    corpusAuthorsLine(),
    expected,
    "the site and the standalone HTML word the credit differently",
  );
});

test("every author is named with a city", () => {
  for (const { name, city } of CORPUS_AUTHORS) {
    assert.ok(name.trim().length > 0, "an author has no name");
    assert.ok(city.trim().length > 0, `${name} is named without a city`);
    // The city goes in the brackets and nothing else does. An institution
    // slipping back in is the mistake this list exists to have already made.
    assert.ok(
      !/\b(university|universität|institute|academy|hochschule)\b/i.test(city),
      `${name} is credited to an institution rather than a city: ${city}`,
    );
  }
});
