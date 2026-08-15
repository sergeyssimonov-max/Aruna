/**
 * The alias table makes claims about people, so it is checked against the
 * catalog that is actually shipped rather than trusted.
 *
 * Two failure modes are worth catching. A spelling that no longer occurs means
 * the table has rotted — TLHdig republished, and an entry now groups something
 * that is not there. A short form whose letters are not the initials of the
 * full name means an entry was added on a hunch, which is the rule this table
 * exists to keep.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { gunzipSync } from "node:zlib";
import { parseWire } from "./arun.ts";
import { EDITOR_ALIASES, searchableEditor } from "./editor-aliases.ts";
import { buildSearchIndex } from "./search-index.ts";
import type { Wire } from "./inventory.ts";

function realWire(): Wire {
  const gz = readFileSync(new URL("../../public/data/inventory.bin.gz", import.meta.url));
  const raw = gunzipSync(gz);
  const buf = raw.buffer.slice(raw.byteOffset, raw.byteOffset + raw.byteLength) as ArrayBuffer;
  return parseWire(buf);
}

/** Every editor value in the shipped catalog, lowercased. */
function editorsInCatalog(): Set<string> {
  const wire = realWire();
  const seen = new Set<string>();
  for (const [, rows] of wire.g) {
    for (const row of rows) seen.add((wire.p[row[1]!] ?? "").toLowerCase());
  }
  return seen;
}

test("every spelling in the table is one the corpus actually uses", () => {
  const editors = editorsInCatalog();
  for (const alias of EDITOR_ALIASES) {
    for (const spelling of alias.spellings) {
      assert.ok(
        editors.has(spelling.toLowerCase()),
        `${spelling} is grouped as an alias but no manuscript is credited to it — ` +
          `the archive changed, and the entry now describes nobody`,
      );
    }
  }
});

test("a short spelling is the initials of the full name, not a guess", () => {
  const initials = (name: string) =>
    name
      .split(/[\s.]+/)
      .filter(Boolean)
      .map((word) => word[0]!)
      .join("")
      .toUpperCase();

  for (const alias of EDITOR_ALIASES) {
    const full = alias.spellings.filter((s) => s.includes(" "));
    assert.equal(full.length, 1, `${alias.spellings.join("/")}: expected exactly one full name`);
    for (const short of alias.spellings.filter((s) => !s.includes(" "))) {
      assert.equal(
        short.toUpperCase(),
        initials(full[0]!),
        `${short} is not the initials of ${full[0]} — an alias needs evidence, not resemblance`,
      );
    }
    assert.ok(alias.evidence.length > 0, "an entry states why these are one person");
  }
});

test("a query for either spelling reaches the other", () => {
  const text = searchableEditor("DS").toLowerCase();
  assert.match(text, /^ds\n/, "the document's own spelling comes first");
  assert.match(text, /daniel schwemer/);
  assert.match(searchableEditor("Daniel Schwemer").toLowerCase(), /\bds\b/);
  // A name that is nobody's alias is returned untouched.
  assert.equal(searchableEditor("LS"), "LS");
  assert.equal(searchableEditor("—"), "—");
});

test("the shipped index carries the aliases, not just the table", () => {
  const blob = buildSearchIndex(realWire());
  assert.ok(blob, "the real inventory builds an index");
  // The author pool is stored as raw UTF-8; the alias must be in those bytes,
  // or the page would still fail to find initials by surname.
  const bytes = new TextDecoder().decode(new Uint8Array(blob));
  assert.ok(
    bytes.includes("ds\ndaniel schwemer"),
    "the pooled editor entry does not carry the person's other spelling",
  );
});

test("the display model is untouched by aliasing", () => {
  // parseWire is what the catalog and the table are built from; the alias must
  // exist only inside the search index.
  const wire = realWire();
  assert.ok(
    wire.p.includes("Daniel Schwemer") && wire.p.includes("DS"),
    "both spellings survive in the catalog exactly as the documents wrote them",
  );
  assert.ok(
    !wire.p.some((s) => s.includes("\n")),
    "no catalog value carries an alias — the table shows what the document says",
  );
});
