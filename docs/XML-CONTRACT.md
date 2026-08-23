# The XML contract

What this project may do to the corpus, what it must preserve, and where every
kind of XML data has to end up when these manuscripts become PDF.

Every number here was measured, and the command that produced it is given. None
of it is estimated.

---

## 1. The source is not ours to change

The TLHdig corpus is the source of truth. This project reads it and writes
somewhere else. Concretely, and without exception:

- The archive is opened read-only. Its digest is taken before and after every
  run that touches it and compared.
- No original XML is rewritten, renamed, moved, re-encoded, re-indented,
  reordered, repaired, or deleted.
- Nothing is written beside an original. Output goes to a destination directory
  that the exporter creates and owns.
- A malformed document is reported, never fixed.

Checked by `tests/corpus.rs` (the whole archive, digest before and after) and
`tests/xml_contract.rs::no_fixture_is_written_to_by_anything_this_program_does`
(every fixture, byte for byte, after the full pipeline).

Digests: **SHA-256** for fixtures (`fixtures/xml/SHA256SUMS`), **MD5** for the
archive — the archive's MD5 is what Zenodo publishes and what the cache is keyed
by, so a second algorithm there would be a second thing to keep in step.

```sh
cd cli
shasum -a 256 -c fixtures/xml/SHA256SUMS
shasum -a 256 fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip
# c845a23223bb9461eeb215f5ede0e223c8871473873c6123eadaeb72114fcd36
```

### Relocation

The export files every document under `CTH N/<siglum>.xml`. Only the path
changes; the bytes do not, beyond a prologue rewrite named in §3. Checked:
23 936 in, 23 936 out, no duplicates, no silent replacement (`create_new` on
every file), the manifest names each output and its source group, and the
inventory links exactly the set that was placed — both directions. 34 documents
need a suffix because their siglum is already taken inside their group — the
`disambiguated` line of `example export_beta` below — and a collision the suffix
cannot resolve stops the build rather than overwriting.

### The same input gives the same output

Two builds of the archive produce the same package, byte for byte: **24 601
files each time, 0 present in one and not the other, 0 with the same path and
different bytes.**

```sh
cd cli
cargo run --release --example determinism -- fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip
```

This is contract-grade rather than incidental. A converter that maps 23 936
documents to 23 936 PDFs has to place each one where the last run placed it —
otherwise every re-run rewrites the whole corpus and nothing incremental is
possible. `tests/reliability.rs` holds the property against a synthetic archive
shaped like the awkward parts of this one, so it is checked without the 71 MiB.

---

## 2. What the corpus actually contains

```sh
cd cli && cargo run --release --example corpus_inventory -- \
  fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip [codepoints.json]
```

TLHdig Beta 0.3, MD5 `f9acbc8db3111cc7dd88d82f7819a912`:

| | |
|---|---|
| documents accepted | 23 936 |
| CTH groups | 663 |
| `.xml` entries the gates refuse | 644 |
| total text | 339.5 MB |
| size min / p50 / p95 / max | 807 B / 5 634 B / 49 010 B / 897 320 B |
| elements | 4 882 576 |
| attributes | 6 372 963 |
| deepest nesting | 80 (`CTH 420_XML_TLH/KBo 59.74.xml`) |
| most elements in one document | 9 171 (`CTH 561_XML_HDivT/KUB 5.1+.xml`) |
| longest single text run | 261 bytes |
| distinct code points | 648 |

Structure, by number of documents carrying it:

| construct | documents |
|---|---|
| namespaces (8 distinct, all in every document) | 23 936 |
| mixed content | 23 616 |
| processing instructions (the stylesheet) | 8 423 |
| XML declaration | 442 |
| comments | 3 |
| BOM | 0 |
| DOCTYPE / DTD | 0 |
| CDATA | 0 |
| entity references | 0 |
| XInclude | 0 |

The eight namespaces are `http://hethiter.net/ns/AO/1.0`,
`http://hethiter.net/ns/hpm/1.0`, `http://purl.org/dc/elements/1.1/`,
`http://www.w3.org/1999/xlink`, and four OpenDocument ones
(`drawing`, `meta`, `table`, `text`). The OpenDocument four are not decoration:
documents carry real table structure.

Collisions: 132 file names repeat across folders, 0 differ only by case, 600 ids
are used by more than one element, 0 symbolic links.

The group count is written down because it was once got wrong in a way nothing
caught. The corpus files one group under several folders — `CTH 5_XML_HFR` and
`CTH 5_XML_TLH` are one group — so counting adjacent runs of the label instead
of distinct labels reported 826. It is 663, and the count is now
order-independent and tested as such. `cargo run --release --example export_beta
-- fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip` prints both the progress line
and the summary; they agree.

Series, by leading letters of the file name: KBo 14 033, KUB 4 072, EBo 1 373,
CHDS 1 060, IBoT 571, DAAM 441, DBH 404, ABoT 391, Bo 289, FHL 169, UBT 160,
VSNF 126, and a tail of smaller ones.

### Documents that are not well-formed XML

**210 of 23 936 (0.88 %).** Measured with `xmllint --noout` over the exported
package. `xmllint` stops at the first error in each document and blames:

| class | documents |
|---|---|
| attribute name that is not a name | 82 |
| raw `<` inside an attribute value | 44 |
| start and end tag do not match | 54 |
| qualified name with an empty local part | 13 |
| other (`expected '>'`, attribute construct) | 17 |

Counting whole documents rather than first errors, **121** have tags that do not
balance; most of those also have an attribute error earlier, which is what
`xmllint` stops on. That figure is asserted by
`tests/corpus.rs::the_documents_whose_tags_do_not_balance_are_the_ones_already_known`.

This is recorded, not repaired. **The next stage must decide a policy before it
meets these documents, not after** — see §5.

---

## 3. Three levels of preservation

### 3.1 The source bytes

Unchanged, always. §1.

### 3.2 The normalised copy

The export writes a normalised copy into the package. The permit list is the
whole of what normalisation may do, and it is defined in exactly one place —
`cli/src/export/verify.rs` — from which the normaliser, the manifest's
`permitted` array and the counters in `applied` are all derived.

| rule | what it does | applied, whole corpus |
|---|---|---|
| `DROP_BOM` | a leading U+FEFF | 0 |
| `DROP_PI xml` | the declaration, replaced by a canonical one | 442 |
| `DROP_PI xml-stylesheet` | `HPMxml.css` is not part of the package | 8 424 |
| `ADD declaration` | `<?xml version="1.0" encoding="UTF-8"?>` | 23 494 |
| `REFLOW prologue whitespace` | between prologue instructions, to one newline | 812 |

**Everything after the prologue is byte-identical.** Not "equivalent", not "the
same once both sides are normalised" — identical. Comparing normalised forms
would hide exactly the corruption this exists to catch, and 78 documents in this
corpus are not in NFC.

Enforced before each document is written, not after: a document that fails stops
the build (`ArunaError::ExportDistorted`) rather than being published with the
rest of the corpus around it.

One further rule, added because a fixture found the hole: **the declaration may
be replaced but not contradicted.** A source declaring `encoding="ISO-8859-1"`
would have kept every byte and changed what all of them mean, and the byte
comparison would have called that no distortion — because by that measure it is
none. Such a document is now refused. No document in this corpus declares
anything but UTF-8 (442 do; 23 494 declare nothing, which XML already reads as
UTF-8), so the rule costs nothing today and closes the guarantee.

### 3.3 The future PDF

§4. This level does not exist yet and must not be faked.

---

## 4. XML → internal model → PDF

There is no PDF converter. There is also **no XML parser**: the dependency list
is `dirs`, `memchr`, `thiserror`, `ureq`, `zip`. What this project calls parsing
is a scan for seven fields in the first 16 KiB of a document; the body is never
interpreted, only copied.

So the middle column below is mostly empty today, and that emptiness is the
finding: **a real parser is a prerequisite for the PDF stage, not an
optimisation.**

| XML construct | in the corpus | internal model today | destination in the PDF |
|---|---|---|---|
| root `AOxml` | 23 936 | — | document frame |
| element tree | 4.88 M elements | — | **layout structure — needs a parser** |
| attributes | 6.37 M | 7 named fields only | metadata block; editorial attributes visible |
| namespaces | 8, everywhere | — | qualified names resolved before layout; ODF `table:` renders as a table |
| mixed content | 23 616 docs | — | **inline runs must stay inline** — the single largest layout risk |
| text nodes | — | — | body text |
| `docID` / siglum | 23 936 | `sigla` | running head and heading |
| `CTHNr` / folder | 23 936 | `cth`, `cth_num` | group, bookmark, table of contents |
| `AO:InvNr` | most | `inv` | metadata block |
| editor, date | most | `authorship`, `year` | metadata block and PDF metadata |
| `lg` language codes | most | `lang` | metadata block; script selection |
| empty markers (`lb`, `gap`, `parlbk`) | most | — | line and section breaks — layout, not nothing |
| comments | 3 docs | — | technical appendix; **not dropped** |
| processing instructions | 8 423 docs | counted | the stylesheet PI is dropped by rule; any other PI goes to the appendix |
| XML declaration | 442 docs | rewritten by rule | not shown; recorded in the manifest |
| ids | 600 duplicated | — | anchors; duplicates cannot be resolved by id alone and need the document path as well |
| unknown elements | possible | — | **must be visible in the appendix, never silently dropped** |
| CDATA | 0 today | — | text, as written |
| entity references | 0 today | — | expansion is a decision, recorded when made |
| DTD | 0 today | — | not fetched, ever |
| XInclude | 0 today | — | not followed, ever |
| the original file | 23 936 | copied verbatim | see below |

**No category is "ignored".** Where a construct is not displayed, the row says
where it goes instead.

### Authenticity: the original beside the PDF

Three options, to be decided before the converter is written, not during:

1. **Embed the XML as a PDF file attachment.** One file to move; every reader
   can extract it; nothing is lost. Costs about the size of the XML again
   (339 MB across the corpus) and not every viewer surfaces attachments.
2. **Ship the XML next to the PDF** in the same folder, as the package already
   does, and record the pairing plus SHA-256 in the manifest. Costs nothing new
   — the package is already this — but the two can be separated.
3. **Reference only**: PDF metadata carries the source path and its SHA-256.
   Smallest, and useless if the corpus is not to hand.

Recommendation is (2) with the digest, because the package already holds both
and the manifest already names both, and (1) as an option for single-document
export. This is written down here rather than chosen silently.

Each PDF must in any case allow its source to be identified: original path,
SHA-256, converter version, run identifier, template version, and the status of
the completeness check.

---

## 5. Policy needed before the converter is written

Open questions, each of which changes what the converter does:

1. **The 210 documents that are not well-formed.** Refuse and report? Recover
   with a lenient parser and mark the PDF as recovered? Produce a placeholder
   page naming the fault? Not deciding means deciding by accident.
2. **Entity expansion.** None appear today. When one does: expand and record,
   or refuse?
3. **Comments.** Three documents carry them. Editorial or incidental?
4. **The 600 duplicated ids.** Anchors need to be unique per PDF; the document
   path disambiguates them, and the manifest must say so.
5. **OpenDocument table markup.** Rendered as tables, or as the transliteration
   rows they encode?

---

## 6. Fonts and Unicode

Measured from the corpus: **648 distinct code points, 382 of them outside the
Basic Multilingual Plane.**

| block | distinct code points | documents (max) |
|---|---|---|
| Cuneiform | 376 | 19 021 |
| Basic Latin | 96 | 23 936 |
| Latin-1 Supplement | 48 | 13 385 |
| Latin Extended-A | 23 | 20 460 |
| General Punctuation | 18 | 22 400 |
| Enclosed Alphanumerics | 16 | 7 474 |
| Combining Diacritical Marks | 12 | 21 |
| Latin Extended Additional | 12 | 16 299 |
| Superscripts and Subscripts | 11 | 8 363 |
| Spacing Modifier Letters | 8 | 1 991 |
| Miscellaneous Technical | 6 | 2 088 |
| Supplementary Private Use Area-B | 6 | 976 |
| Supplemental Punctuation | 3 | 26 |
| Greek and Coptic | 3 | 1 |
| Block Elements (`▒`) | 1 | 23 216 |
| Arrows (`→`) | 1 | 19 259 |
| Currency Symbols | 1 | 1 725 |
| Hebrew, Number Forms, Control Pictures, Math Operators, Misc Math-A | 1–2 each | ≤ 26 |

Consequences for the font choice, in order of how much trouble they cause:

- **376 cuneiform signs above the BMP.** Most text fonts have none of them. A
  dedicated cuneiform face is required and must be selected by coverage, not by
  looks.
- **Supplementary Private Use Area-B in 976 documents.** No general font can
  render these: they mean whatever the corpus decided. Either the corpus's own
  font is obtained and licensed, or these need a documented fallback that is
  visible rather than a blank box.
- **Enclosed Alphanumerics (`① Ⓐ`) in 7 474 documents.** Missing from most text
  faces and a classic source of silent tofu.
- **78 documents are not in NFC.** A renderer that assumes composed diacritics
  will place marks wrongly on the rest. Normalising the text would change it and
  is therefore not an option; the renderer must handle both forms.

Before any font is bundled: check the licence for redistribution **and** for
embedding, and record both. Do not convert text to outlines — it destroys
search, copying and accessibility and inflates the file.

### What the installed fonts actually cover

```sh
cd cli
cargo run --release --example font_coverage -- \
  fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip [report.json]
```

Measured, not sampled: the `cmap` table of every font file on the machine, read
against every code point the corpus really uses. There is no list of "the
Hittitological characters" anywhere in that program — the code points come out
of the archive.

The program reads the declared stack out of the built canonical section rather
than carrying its own copy of it — a second list would be a second statement of
the same decision, and the two drifting apart is precisely what this exists to
detect.

On macOS 13.7.8, 366 font files:

| | before | now |
|---|---|---|
| covered by the **declared** stack | 259 / 648 (40 %) | **642 / 648** |
| covered outside it, by system fallback | 389 | **6** |
| covered only by a font this machine happens to have | 1 | **0** |
| covered by nothing anywhere | 5 | 5 |

**`docs/FONTS.md` is the specification** — every face, its source, its SHA-256,
its licence, its `fsType` embedding bit, and how to reproduce the environment on
another machine. What follows here is why the corpus needs what it needs.

### The stack was wrong, and this is what fixed it

The first column is what this project shipped until 2026-08-22. **Three fifths
of the corpus rendered only because macOS silently substituted a font nobody had
chosen** — including the whole of the cuneiform, 376 signs across 19 021
documents. Fallback is a property of one operating system: it differs between
machines, and a PDF engine need not do it at all. "It looks right here" was the
entire basis for believing those documents would print.

Four faces close it, and all four are named in the stack now:

| | |
|---|---|
| **Noto Sans Cuneiform** | 376 signs — every standard cuneiform character the corpus uses. System font, OFL, `fsType` 0. |
| **UllikummiA** | 1 sign — `U+100000`, in 927 places. A cuneiform sign with no Unicode code point, allocated in the private use area by S. Vanséveren and published in the Hittite Sign List. Nothing else draws it. Not a system font; `docs/FONTS.md` says where to get it and how to check it. |
| **STIX Two Math** | 6 signs: `U+24F5`–`U+24F8`, the double-circled digits used as editorial marks, and `U+27E8`/`U+27E9`, the angle brackets. System font, `fsType` 0. |
| **Arial** | 1 sign — `U+05C3`, Hebrew punctuation, at most 26 documents. |

`cli/src/style.rs::the_font_stack_names_what_the_corpus_needs` holds the stack to
this. It is a source-level assertion because that is where the failure is
visible: a missing family breaks no page, no build and no test — the document
renders, on this machine, through a font nobody chose.

**`Hiragino Sans GB` is deliberately not named**, though it would close one more
point. It "covers" `U+E83A` in the sense that its private-use area happens to
hold a Chinese glyph at that number. Naming it would make an unrelated sign the
official rendering of a TLHdig character, which is worse than the empty box it
would replace. The same test refuses it.

### What is left, and why

Six code points are outside the declared stack, and every one of them is private
use. There is no longer any ordinary character relying on fallback.

Five are cuneiform signs TLHdig encodes privately — `U+100001`, `U+100003`,
`U+100005`, `U+100006`, `U+100009`, the last in 2 379 lines. **No font draws
them**: not the official Ullikummi package from the portal, not Semiramis, not
any of the 366 faces macOS ships. Nor are they in the Hittite Sign List, which
allocates exactly three private-use points and of which this corpus uses one.
TLHdig's allocation goes beyond what its font provider has published, and only
the TLHdig editors can say what these signs are.

The sixth is `U+E83A`, twice, and it is **not cuneiform at all** — it sits in a
German footnote about a photograph collation, not in any `cu=` attribute. Almost
certainly a leftover from a legacy encoding.

None of the six comes out blank on macOS: `LastResort.otf` draws a placeholder
box for the whole private-use range, which is what those characters look like on
this machine and on the code-point websites. That is a marker meaning *nothing
can render this*, not a rendering — `docs/FONTS.md` explains why the audit keeps
the two apart, and records that failing to read that font at all was a defect in
the audit until 2026-08-22.

Both are in the source, and the source is not ours to correct. Substituting a
similar sign would be worse than the gap: a wrong sign that renders beats nothing
only until somebody reads it.

### The three findings that remain

**Five code points no font here draws.** `U+100001`, `U+100003`, `U+100005`,
`U+100006`, `U+100009` — private use, and not covered by Ullikummi A/B/C or
Semiramis Unicode 3 either, the Hittitological fonts on this machine.
`U+100009` alone is in **976 documents**; it is in the shipped package, e.g.
`CTH 572/KBo 58.64.xml`. A PDF will have a hole at each of them, whatever
engine renders it. This is a question about the corpus rather than about this
program: these code points mean whatever TLHdig assigned them, and no font
outside the project knows it.

**One that works only here.** `U+100000` is drawn by `UllikummiA.ttf` in
`~/Library/Fonts` and by nothing else — tofu on any other Mac, unless that font
is licensed and bundled.

**One that renders the wrong sign.** `U+E83A` is "covered" — by Hiragino Sans
GB, a Chinese font whose private-use slot holds an unrelated glyph. That is
worse than an empty box: it draws something plausible, and nothing reports it.

All three are private use, and all three are the same question: these code
points mean whatever TLHdig assigned them, and no font outside the project
knows it. Nothing in this program can decide that.

The fourth finding — that three fifths of the corpus rendered through fallback,
cuneiform included — was fixed on 2026-08-22 by naming the two faces above. It
is recorded here rather than deleted because it is the kind of defect worth
remembering: nothing failed. Every page rendered, every test passed, every
linter was quiet, and the documents were one operating system away from being
unreadable.

`manifest.json` carries `fonts.private_use_points` so a renderer has the list
without re-running this.
