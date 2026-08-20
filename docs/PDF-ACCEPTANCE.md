# PDF: what will have to be true

There is no converter. This document is the specification the future one will be
held to, written before it exists so that its tests are a contract rather than a
description of whatever it happened to do.

Nothing here has been implemented, and no placeholder test has been written for
it. An `#[ignore]` that never runs is not coverage.

---

## 0. Prerequisite: a real XML parser

The current pipeline reads seven metadata fields out of the first 16 KiB of a
document and copies the rest byte for byte. It has no element tree, because it
has no XML parser — the dependency list is `dirs`, `memchr`, `thiserror`,
`ureq`, `zip`.

Converting these manuscripts without losing their content therefore begins with
choosing a parser, not a PDF library. Requirements, in order:

1. **Entity expansion off, DTD fetching off, XInclude off by default**, and no
   filesystem or network access from within parsing. Today these hold because
   nothing resolves anything; a parser makes them a configuration to get right.
   `cli/tests/xml_hostile.rs` already checks them and will keep doing so.
2. **Streaming**, so a document is not held twice. The corpus is 339.5 MB of
   text and the largest document is 897 KB.
3. **Recoverable**: 210 of 23 936 documents are not well-formed. A parser that
   can only refuse turns 0.88 % of the corpus into a hole.
4. **Position reporting** — line and column — because an error without one is
   not actionable across 23 936 files.

The PDF library is a separate choice and must be compared on fixtures before
being adopted, not picked from a list. Do not couple the document model to it.

## The boundary to keep

```
XML bytes → safe parser → document model → layout model → PDF renderer → validator
```

The document model must not know about the renderer, and no PDF type may reach
the domain. The layout model is what visual tests read; the document model is
what semantic tests read. Both must be checkable without producing a PDF.

---

## 1. Structural validity

Checked with external tools, none of which is installed here — installing them
is a decision for whoever runs this, and their absence must never be read as a
pass.

- the file parses (`qpdf --check`)
- the cross-reference table is intact
- no incomplete write is ever visible under the final name
- pages exist and are the declared size (`pdfinfo`)
- every font is embedded and subset (`pdffonts` — no `no` in the *emb* column)
- no external resource is referenced
- a second, independent reader opens it (MuPDF as well as Poppler)

**PDF/A is not a requirement** and must not be introduced as one without a
separate decision.

## 2. Semantic completeness

Text extraction alone does not prove this and must not be presented as if it
does. Each document gets a **semantic manifest** — the expected content, in
order, generated from the document model — and the check compares extraction
against that manifest, not against a blob of text.

Per document:

- every section present, in source order
- the CTH identifier, the fragment siglum, the series
- headings at the right level
- body text complete: no truncation, no repetition, no text from another document
- notes present and attached to the right anchor
- metadata: editor, year, languages, inventory number
- special characters unsubstituted — no `?`, no `□`, no dropped diacritic
- every construct in the XML → PDF map of `XML-CONTRACT.md` reaching the place
  that table says it goes

## 3. Visual correctness

Rendered pages compared against references, for a fixed set of documents.

**Never pixel-perfect across environments.** Renderer version, font version and
operating system all move pixels without moving meaning. Either pin the
environment in a container and compare exactly, or compare with a tolerance and
back it with structural checks — text box positions, line counts, page counts —
which are stable where pixels are not.

The reference set, chosen for what breaks layout rather than for typicality:

| reference | why |
|---|---|
| shortest document (807 B) | a page that is almost empty |
| typical document (5.6 KB) | the common case |
| largest document (897 KB) | many pages, sustained |
| deepest nesting (80 levels) | nesting must not become 80 indents |
| heaviest mixed content | inline runs staying inline |
| most notes | note block against page break |
| rare characters, cuneiform, private use | glyph coverage |
| longest identifiers | running head overflow |
| document with an OpenDocument table | table rendering |
| a document that ends one line onto a new page | widow and orphan handling |

Each must show: no clipped text, no overlap, nothing outside the margins,
correct hyphenation, sensible page breaks, no unexplained blank page, correct
heading hierarchy, stable running heads and page numbers, correct Unicode with
no missing glyph, correct diacritic placement, no rasterised text.

## 4. Authenticity

For every PDF it must be possible to name: the source XML, its relative path,
its SHA-256, the converter version, the run identifier, the template version,
and the outcome of the completeness check.

Golden tests must normalise the run identifier and any timestamp, or they will
fail on the second run for no reason.

---

## 5. Batch behaviour

The batch is the whole corpus: 23 936 documents into 663 folders.

Must: write atomically, never leave a damaged file under a final name, never
touch the source, continue past a single document's failure where that is safe,
end with a summary that distinguishes success, warning, error and cancellation,
be safe to run again, not convert a document twice without reason, keep memory
and concurrency bounded, and produce results in a stable order.

Scenarios to test: empty corpus; one document; a few groups; the whole corpus;
one malformed document among good ones; repeated file names; a name collision
after normalisation; PDFs already present; an unwritable output directory; a
full disk, simulated safely and never by actually filling one; cancellation at
the start, the middle and the end; a crash after the temporary file exists but
before the rename; a re-run after a partial one; an XML changed between
inventory and conversion; an XML deleted mid-run; a bounded number of parallel
tasks.

Two of these are already solved for the export and the same shapes should be
reused: `Staging` (a half-built package that removes itself unless published)
and `Replaced` (the previous package moved aside and put back if the publish
fails).

---

## 6. The Scandinavian minimal style, as tokens

"Scandinavian minimalism" is not a checkable requirement. These are:

- light neutral ground, restrained black-to-grey palette
- at most one accent colour, and only if something actually needs one
- generous whitespace, quiet typography, clear but unemphatic hierarchy
- no decorative noise, and specifically no parchment texture, no cracks, no
  seals, no archaeological pastiche
- legibility of scholarly text before everything else
- technical identifiers set carefully rather than hidden

Fix these as configurable tokens before writing any layout code, and do not pick
final numbers without looking at rendered pages:

`page size · margins (inner, outer, top, bottom) · body face · face for special
signs · heading sizes · body size · leading · space between blocks · measure
(line length) · palette · rule weights · running head and foot · page numbering ·
widow and orphan rules · block-breaking rules`

Then check the reference pages of §3 for widows, orphans, a heading alone at the
foot of a page, text set too tight, and gaps with no reason.

---

## 7. Fonts

The corpus uses **648 distinct code points, 382 of them above the BMP**, of
which 376 are cuneiform. See `XML-CONTRACT.md` §6 for the full table and the
four consequences that decide the font choice.

Before bundling anything: verify the licence permits both redistribution **and**
embedding, and record both. Check that every code point in the corpus has a
glyph — the list is produced by `corpus_inventory`. Do not convert text to
outlines.
