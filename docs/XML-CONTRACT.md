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
inventory links exactly the set that was placed — both directions.

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
