/**
 * The note the page shows when search is running on the fallback engine claims
 * to know why. It is seen exactly when something has gone wrong, so a wrong
 * explanation is worse than none — and it went wrong once already: the text
 * blamed "more than 64 distinct editors" for months after the ceiling became
 * 255 and while the corpus held 45.
 *
 * Two things keep it honest. The reason is decided by the worker, from what
 * actually happened, and travels in the message. And no explanation may name a
 * number that lives in the code, because that is the kind of fact that moves
 * without anyone remembering the sentence that quotes it.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const HEADER = new URL("../components/inventory-header.tsx", import.meta.url);
const PROTOCOL = new URL("./search-protocol.ts", import.meta.url);

const header = readFileSync(HEADER, "utf8");
const protocol = readFileSync(PROTOCOL, "utf8");

/** The reasons the protocol defines, read from its union type. */
function declaredReasons(): string[] {
  const m = /export type FallbackReason =([^;]+);/.exec(protocol);
  assert.ok(m, "search-protocol.ts no longer declares FallbackReason");
  return [...m[1]!.matchAll(/"([a-z]+)"/g)].map((x) => x[1]!);
}

/** The reasons the header knows how to explain, read from its lookup table. */
function explainedReasons(): string[] {
  const m = /const EXPLANATION: Record<FallbackReason, string> = \{([\s\S]*?)\n\};/.exec(header);
  assert.ok(m, "inventory-header.tsx no longer maps reasons to explanations");
  return [...m[1]!.matchAll(/^\s{2}([a-z]+):/gm)].map((x) => x[1]!);
}

test("every reason the worker can send has an explanation", () => {
  assert.deepEqual(
    explainedReasons().sort(),
    declaredReasons().sort(),
    "a reason with no text would show the reader an empty tooltip; " +
      "an explanation for a reason nobody sends is dead prose",
  );
});

test("no explanation quotes a limit that lives in the code", () => {
  const m = /const EXPLANATION: Record<FallbackReason, string> = \{([\s\S]*?)\n\};/.exec(header);
  const text = m![1]!;
  const numbers = [...text.matchAll(/\b\d+\b/g)].map((x) => x[0]);
  assert.deepEqual(
    numbers,
    [],
    `an explanation names ${numbers.join(", ")} — the pool cap, the id width and the corpus ` +
      "all move, and the sentence quoting them will not move with them",
  );
});

test("the note says something for each reason, and says it plainly", () => {
  const m = /const EXPLANATION: Record<FallbackReason, string> = \{([\s\S]*?)\n\};/.exec(header);
  const quoted = [...m![1]!.matchAll(/"([^"]{10,})"/g)].map((x) => x[1]!);
  assert.equal(quoted.length, declaredReasons().length, "one sentence per reason");
  for (const sentence of quoted) {
    assert.match(
      sentence,
      /Results are the same/,
      "the reader's first question is whether the answers changed",
    );
  }
});
