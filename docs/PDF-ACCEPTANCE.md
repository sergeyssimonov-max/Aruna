# PDF: what will have to be true

There is no converter. This document is the specification the future one will be
held to, written before it exists so that its tests are a contract rather than a
description of whatever it happened to do.

Nothing here has been implemented, and no placeholder test has been written for
it. An `#[ignore]` that never runs is not coverage.

**How this file changes.** A requirement written before the work is worth
keeping only if it cannot be quietly bent to fit whatever was achieved. So this
file is amended on one ground: a measurement showed a requirement to be
unmeetable, harmful, or answering a question that turned out to be the wrong
one. Every amendment says which measurement, on what date, and what it replaced
— an amendment that cannot name one is a description of convenience and does not
belong here. Requirements untouched by measurement stay as written, including
the ones that have proved uncomfortable.

Amended 2026-09-08: §0 requirement 3, §1 tooling, §2 method. Each is marked
below.

Corrected 2026-09-09: two sentences the amendment left behind still described
a dependency list without the parser adopted three lines below them.
`cli/Cargo.toml` carries six crates, `quick-xml` among them, and `cargo deny`
and `cargo-machete` were run on that tree the same day and are green. A
correction of fact, not an amendment — no requirement changed.

---

## Before any of it: two things that have to be true first

Reviewed 2026-08-25, and the answer was "not yet". Neither of the following is
about how a converter should be built; both are about whether one should be
started, and both are cheap to check.

*Settled 2026-09-02: the owner recorded the first trigger — PDF is a direction
this project takes, and the package stays the reference result rather than the
end of the pipeline. The second is still open; see requirement 3 below for what
three measurements did to it.*

**Someone has to be able to read the result.** A PDF is a deliverable only if
there is a viewer, a print route or a reader waiting for one. Absent that, the
XML package *is* the end state — 663 folders a person can open, an inventory
that links every one of them — and it is a finished thing rather than a stage on
the way to a document nobody opens. So this work starts on one of two triggers:
a viewer exists, or the project records the opposite decision explicitly, that
the package is where the pipeline ends and PDF is closed rather than pending.
Until one of those is written down, this file is a specification held in
reserve.

**And an XML crate must not cost the build what it is worth.** The dependency
list is six crates since the parser was adopted, `cargo deny` holds licences,
sources and advisories,
`cargo-machete` fails on anything unused, and the tree is audited on every push.
A parser is the largest dependency this project would have taken; the way to
find out what it costs is to add the candidate, run the whole gate set, and read
what breaks — before a line of conversion is written, not after. A parser that
cannot pass those checks is not a parser this project can adopt, whatever it
does with documents.

## 0. Prerequisite: a real XML parser

The current pipeline reads seven metadata fields out of the first 16 KiB of a
document and copies the rest byte for byte. It still has no element tree: the
parser adopted below reads events in a stream, for the classification step and
nothing else. The dependency list is `dirs`, `memchr`, `quick-xml`, `thiserror`,
`ureq`, `zip`.

Converting these manuscripts without losing their content therefore begins with
choosing a parser, not a PDF library. The parser was adopted on 2026-09-05:
`quick-xml`, pinned exactly, default features off. Requirements, in order:

1. **Entity expansion off, DTD fetching off, XInclude off by default**, and no
   filesystem or network access from within parsing. Today these hold because
   nothing resolves anything; a parser makes them a configuration to get right.
   `cli/tests/xml_hostile.rs` already checks them and will keep doing so.
2. **Streaming**, so a document is not held twice. The corpus is 339.5 MB of
   text and the largest document is 897 KB.
3. ~~**Recoverable**: 210 of 23 936 documents are not well-formed. A parser that
   can only refuse turns 0.88 % of the corpus into a hole.~~

   **Amended 2026-09-08. Strict, never recovering.** Three measurements retired
   the requirement above, and the decision it now carries is its opposite.

   *2026-09-05, classification.* Of the 206 documents this project's parser
   refuses, not one is repairable by a deterministic byte-level rule — not a
   minority, zero. Under every visible defect sits one structural fault, markup
   inside an attribute value: fix the byte outside and the same place fails
   differently. All 206 failures are inside the body, none in the prologue, so
   any silent repair edits the transliteration rather than the packaging.

   *2026-09-06, recovery.* `xml5ever` was run over the same 206, the tree
   serialised back and compared with the source. Four documents are provably
   corrupted — their text contains a sequence the parser produced in none of the
   23 730 well-formed documents — and the parser says nothing. What corruption
   looks like is markup read as text: file fragments land in the
   transliteration, where a reader meets them as a Hittite word.

   *2026-09-06, evening.* The one class that had looked local — 77 documents —
   is five causes, and the two largest have no single correct repair: one byte
   sequence admits several well-formed readings that differ in word membership,
   and a word is the unit the inventory groups by. Exactly one document of 77
   repairs unambiguously.

   **What the parser must do instead.** Refuse, and say where. A refused
   document stays in the package — the package is a byte-for-byte mirror and
   needs no parse — and its name, reason and position are recorded in the
   package manifest, so a reader sees what will not be converted and why.
   Recovery, silent repair and lenient modes are out, and adding one back
   requires a measurement showing it does not alter text without saying so.

   Verified against an independent implementation on 2026-09-07: `xmllint`
   counts 210, this parser 206, the four names differing are only ever on
   `xmllint`'s side. The parser is nowhere stricter than the reference, and its
   blind spot — a raw `<` inside an attribute value, an empty local name — is
   named in the manifest rather than left for a reader to discover.
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

What must hold is unchanged; **the tooling naming it was amended 2026-09-08**,
because the list below named programs this environment will not have. Homebrew
is not used here, and tools requiring a Java runtime are barred outright, which
removes veraPDF and anything like it. A requirement whose only named instrument
is unavailable is a requirement nobody checks.

The properties:

- the file parses
- the cross-reference table is intact
- no incomplete write is ever visible under the final name
- pages exist and are the declared size
- every font is embedded and subset
- no external resource is referenced
- a second, independent reader opens it

How to check them without installing anything foreign:

| property | instrument |
|---|---|
| structure, page count and size, embedded fonts | a PDF-reading crate in Rust as a dev-dependency — candidates not yet chosen |
| a second, independent reader | a crate binding a renderer other than the one that produced the file; this is the one property no in-tree Rust answers on its own, and if no candidate passes the environment's constraints, say so rather than drop the property |
| structural comparison of two results | canonicalisation, `xmllint --c14n`, which ships with macOS |
| an independent extraction for §2 | `xsltproc`, which ships with macOS |

`xmllint` and `xsltproc` are the only implementations in this contour not
written by this project, and both cost nothing to have. Neither goes anywhere
near the working path: examples and tests only.

**Their absence must never be read as a pass.** That rule survives the
amendment unchanged, and now has teeth: if the second-reader property has no
instrument, it is unmet and recorded unmet, not quietly dropped.

**PDF/A is not a requirement** and must not be introduced as one without a
separate decision. Note what the Java bar does to that decision: the common
validator is a Java program, so were PDF/A ever adopted, the means of checking
it would have to be settled first, and before the requirement — not after.

## 2. Semantic completeness

Text extraction alone does not prove this and must not be presented as if it
does. Each document gets a **semantic manifest** — the expected content, in
order, generated from the document model — and the check compares extraction
against that manifest, not against a blob of text.

**Amended 2026-09-08: a text criterion needs a structural one beside it.** The
77-document measurement of 2026-09-06 caught this on XML before it could be
made on PDF. The criterion in use there — "the sequence of non-whitespace
characters matches" — passed every one of several readings that differed in
structure, because comparing characters cannot see where the boundaries are.
Applied to a repair, it would have accepted an arbitrary choice among readings
that group words differently.

So the two questions are separate and neither answers the other. *Is anything
lost?* — text comparison against the semantic manifest. *Is it the same
structure?* — comparison of canonical forms, which fixes attribute order, empty
element form and line endings, so two documents whose canonical forms agree are
one tree written two ways. Canonicalisation needs well-formed input, so it
compares two results, never a result against a malformed source.

A further reason not to lean on extraction alone: the manifest is generated by
this project's own document model, so extraction checked against it measures
this project against itself. `xsltproc` extracting the same fields from the
well-formed source is an independent second opinion, in the way `xmllint` was
for the count of malformed documents.

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
