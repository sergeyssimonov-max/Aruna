# The XML contract

What this project may do to the corpus, what it must preserve, and where every
kind of XML data has to end up when these manuscripts become PDF.

Every number here was measured, and the command that produced it is given. None
of it is estimated.

Amended 2026-09-30: §2, §4 and §5, by the owner's decisions of 2026-09-30 on
questions 3 and 5 of §5; the measurement is in §2. First entry of a log this
file had not kept; earlier changes stand dated where they were made, and each
place changed today is marked below.

Amended 2026-09-30, second entry: §5 question 5, how the shift of `text:tab`
is drawn, by the owner's decision of the same day on question 4 of the design
note. Marked below.

Amended 2026-10-06: §3.3, the opening of §4, the authenticity options of §4
and the heading of §5 – by the owner's decision of 2026-10-06 on authenticity
(option 2), and to state that the converter exists. Written on 2026-10-08.
Marked below.

Amended 2026-10-27: §4, two rows of the table and the new part "What a page
shows" – by the measurement of 2026-10-26 (the task markup-leak-fix: the
markup of notes and `cu` printed as letters, the signs of the edition lost)
and the owner's decisions of 2026-10-26 and 2026-10-27. Marked below.

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
inventory links every placed document once and nothing else – at its PDF, or
at its XML where no PDF was built (owner's decision of 2026-10-19) – both
directions. 34 documents
need a suffix because their siglum is already taken inside their group — the
`disambiguated` line of `example export_beta` below — and a collision the suffix
cannot resolve stops the build rather than overwriting.

### The same input gives the same output

Two builds of the archive produce the same package, byte for byte: **23 940
files each time, 0 present in one and not the other, 0 with the same path and
different bytes.** Measured again on 2026-09-18.

The count read 23 938 from 2026-08-23 until 2026-09-17, when the packaged font
and the text of its terms went into the package (`4d2caf2`); those two are the
whole of the difference.

Before 2026-09-06 it read 24 601 here, and had been wrong since 2026-08-23: it
included a page for each of the 663 CTH folders, and those pages were given up
that day. The property never changed — two builds are identical — but the
number stating it described a package this program had stopped producing.

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
| deepest nesting | 80 (`CTH 420_XML_TLH/KBo 59.74.xml`, one of the 206 this parser refuses); 14 among the 23 713 the document model reads (`CTH 592_XML_HFR/IBoT 4.62+.xml`, the root counted as 1, counted on the model by `tests/document_model.rs`) |
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
| entity references beyond the five predefined and character references | 0 |
| XInclude | 0 |

The eight namespaces are `http://hethiter.net/ns/AO/1.0`,
`http://hethiter.net/ns/hpm/1.0`, `http://purl.org/dc/elements/1.1/`,
`http://www.w3.org/1999/xlink`, and four OpenDocument ones
(`drawing`, `meta`, `table`, `text`). ~~The OpenDocument four are not decoration:
documents carry real table structure.~~

*Amended 2026-09-30.* The struck sentence described markup the corpus does not
contain. Measured over all 23 936 documents of the 2.6.2 package on 2026-09-30
(fourth PDF trial): the four namespaces are declared on every root, and no
element of `table` or `drawing` occurs anywhere — 0 documents. What occurs of
OpenDocument is `text:tab` (63 documents, 212 occurrences, all among the 23 713
the document model reads), `text:line-break` (6 documents, all six refused by
the model), `text:h` (4) and `text:bookmark` (1). `text:tab` is a trace of
tabulation carried over from an editor: it shifts the rest of a line towards a
column of the tablet, and it defines no cells and no rows. **By the owner's
decision of 2026-09-30 a `text:tab` is rendered as a shift of the line**, the
view "rows" of the trial, never as a table (§4, §5 question 5).

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

### Documents a parser has something to say about

Three numbers, and they are three different questions: what this program could
not read, what breaks XML, and what breaks Namespaces in XML. Only the first has
a consequence for a reader — those documents are the ones a conversion cannot
take. Re-measured 2026-09-10 with `xmllint --noout` over the exported package;
the manifest's `xml.totals` now carries all three, each with a sentence saying
what it counts.

| | documents | what it is |
|---|---|---|
| unread here | **206** | refused by this crate's parser — `quick-xml`, no recovery. Named with reason, line and column in `manifest.json` under `not_well_formed_documents`. Says nothing by itself about XML: it is this program's boundary. |
| not well-formed XML | **210** | the 206 plus 4 more: a raw `<` inside an attribute value, which `quick-xml` takes for an ordinary character. `xmllint --noout` exits non-zero on exactly these 210. |
| not well-formed XML, or invalid under Namespaces in XML | **223** | the 210 plus 13 more: an element name of the form `<AO:-…>`, a qualified name with no local part. **Those 13 are well-formed XML** — the word for them is namespace-invalid, not incorrect XML. |

The 206 are a subset of the 210 and the 210 of the 223 — checked in both
directions, by name, on 2026-09-10.

**The 13 are the correction of a three-week error.** They are legal XML 1.0:
`:` and `-` are both name characters, so `AO:-LineNrExpl` is a well-formed
`Name`. *Namespaces in XML* is what they break, and `libxml2` reports that as a
`namespace error` and **exits zero** — so a measurement that read the exit
status never saw them, and the table that stood here counted them inside the 210
and made the arithmetic balance by mis-stating the tag-mismatch class as 54.

First-error blame over the 223, which is what `xmllint` prints and the only
figure it prints:

| class | documents | kind |
|---|---|---|
| attribute name that is not a name | 82 | parser error |
| start and end tag do not match | 65 | parser error |
| raw `<` inside an attribute value | 44 | parser error |
| attributes construct error | 15 | parser error |
| `Failed to parse QName 'AO:'` | 13 | namespace error |
| `expected '>'` | 3 | parser error |
| attribute given twice | 1 | parser error |

Counting whole documents rather than first errors, **121** have tags that do not
balance; most of those also have an attribute error earlier, which is what
`xmllint` stops on. That figure is asserted by
`tests/corpus.rs::the_documents_whose_tags_do_not_balance_are_the_ones_already_known`.

**The 17 are now produced by this crate, not copied from `xmllint`.**
`xml_wellformed::beyond_the_parser` scans for both classes and gives file, line
and column; `tests/corpus.rs::the_documents_beyond_this_parser_are_the_seventeen_xmllint_names`
holds it to the seventeen names `xmllint` blames, in both directions. A number
that no code produces cannot notice a new edition of the corpus, and for three
weeks this one did not.

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
| `DROP_PI xml` | the declaration, replaced by a canonical one | 0 |
| `DROP_PI xml-stylesheet` | `HPMxml.css` is not part of the package | 8 424 |
| `ADD declaration` | `<?xml version="1.0" encoding="UTF-8"?>` | 23 494 |
| `REFLOW prologue whitespace` | between prologue instructions, to one newline | 812 |

**The counts are of instructions, not of documents, and two of these rows were
read the other way until 2026-09-10.**

`DROP_PI xml` stood here as 442, which is how many documents declare an encoding
at all — 23 936 less the 23 494 that get a declaration written for them. The
counter has always been 0 and is right to be: all 442 declare
`<?xml version="1.0" encoding="UTF-8"?>` byte for byte, `verify::compare` sees
that instruction in the output as well as the source, and counts it kept rather
than dropped and re-added. Nothing changed in those documents, so nothing is
counted. Measured over the archive on 2026-09-10: 442 declarations, none of them
differing from the canonical one by a byte.

`DROP_PI xml-stylesheet` is 8 424 instructions in **8 423** documents — one
document, `CTH 475_XML_HFR/KBo 43.119.xml`, opens with two byte-identical copies
and both are removed. That is why §2's table of what the corpus contains says
8 423 and this one says 8 424: different questions, one apart.

The row's key is the permit list's spelling and never the document's. Targets
are matched without regard to case, so a document opening `<?XML-STYLESHEET …?>`
is removed under this rule — and until 2026-09-10 it was counted under
`DROP_PI XML-STYLESHEET`, a key the `permitted` list above never offers. This
corpus contains no such spelling; the property is held by
`verify::a_dropped_instruction_is_counted_under_the_permitted_name` rather than
by that fact.

**Everything after the prologue is byte-identical.** Not "equivalent", not "the
same once both sides are normalised" — identical. Comparing normalised forms
would hide exactly the corruption this exists to catch, and 2 127 documents in
this corpus are not in NFC (measured 04.10.2026, Python 3.9.6, Unicode 13.0;
see the next section for what the manifest's 78 counts instead).

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

**The version is held the same way since 2026-09-22** (`47dbf1e`): a source
declaring any version other than `1.0` is refused, because the canonical
declaration would silently change what its line ends and permitted characters
mean. All 442 declarations in the corpus say `1.0`.

**Two exceptions to "the document stays in the package", neither of them a
parse defect** (owner's decision, recorded 2026-09-25; findings T2 and T3 of the
reliability run of 2026-09-22). A document whose declaration contradicts the
canonical one, an encoding other than UTF-8 or a version other than 1.0, is not
copied under the canonical declaration: `verify::compare` refuses it and the
whole build stops with `ArunaError::ExportDistorted`, so nothing is published.
Since 2026-09-24 (`e297f15`) every document is checked, none of the refused ones
is written, and the refusal names each of them, not only the first in archive
order. And an entry whose first 16 KiB carry no manuscript markup in ASCII, a
well-formed UTF-16 document among them, is not a package document and is listed
under `source.not_manuscripts`. Neither occurs in TLHdig Beta 0.3 (442
declarations, all `1.0` and UTF-8; no UTF-16). The decision is revisited with
the first such document.

### 3.3 ~~The future PDF~~ The PDF

~~§4. This level does not exist yet and must not be faked.~~

*Amended 2026-10-06.* The converter exists since the task pdf-converter-2
(2026-09-30): the module `cli/src/pdf/` builds one PDF beside each XML document
of the package, in a phase of `export` (§4). In release 2.6.3 it is off by
default, in the window and in the console, and runs only when asked for
explicitly (by a console switch, removed on 2026-10-18) – the path of the release gate,
`pdf_gate` and the tests. On by default returns with the window in ~~2.7.0~~
3.0.0.

*Amended 2026-10-11.* The next release is 3.0.0, not 2.7.0 (owner's decision
of 2026-10-08). On by default is back in the console since 2026-10-11 by the
owner's decision of that day; the window asks for no PDF until the third part
of the transfer, in 3.0.0.

*Amended 2026-10-18.* From 3.0.0 every package carries a PDF of each document,
in the window and in the console alike (owner's decision of 2026-10-18):
nothing in either leaves them out. Without fonts no package is built – the
refusal is `font_missing` or `font_altered`.

*Amended 2026-10-19.* The inventory links each document once, on its name: at
its PDF, and at its XML where no PDF was built (owner's decision of
2026-10-19). The XML of every document stays in the package and in the
manifest; a document with a PDF has no link to its XML in the inventory.

---

## 4. XML → internal model → PDF

~~There is no PDF converter.~~ *Amended 2026-10-06:* there is a PDF converter
since 2026-09-30, the module `cli/src/pdf/`, off by default in release 2.6.3
(§3.3). There is, since 2026-09-13, a **document model**:
`cli/src/document.rs`, over the parser adopted on 2026-09-05 (§5), with no new
dependency. It is the XML Information Set of one document — line ends and
attribute values normalised, the predefined entities and character references
replaced, adjacent character data one node — and it holds nothing the document
does not: no siglum, no CTH number, no editor, no default for anything absent.
~~Nothing consumes it yet.~~ *Amended 2026-10-06:* the PDF phase consumes it –
each document is read back into the model before it is laid out. The inventory fields are still produced by a scan of
the first 16 KiB, and deriving them from the model is the next step, together
with the semantic manifest of `PDF-ACCEPTANCE.md` §2.

The model is built for **23 713** documents and refused for **223**: the 206 this
parser refuses, with the classifier's reason, and the seventeen of §2 it accepts
— §5 sends both to the same place. Over all 23 713 it was compared with
`xsltproc`, one line per node in document order, and **25 857 289 lines are
identical** (`tests/document_model.rs`, 2026-09-13; a 52-document sample of the
same comparison runs with every test run).

| XML construct | in the corpus | internal model today | destination in the PDF |
|---|---|---|---|
| root `AOxml` | 23 936 | `Document::root` | document frame |
| element tree | 4.88 M elements | `Kind::Element`, in document order | layout structure |
| attributes | 6.37 M | `Element::attributes`, in the order written; the scan still reads 7 named fields | metadata block; editorial attributes visible — *amended 2026-10-27:* the attributes a rule of "What a page shows" names; no other is shown |
| namespaces | 8, everywhere | `Element::namespaces` as declared; `Name::namespace` resolved | qualified names resolved before layout ~~; ODF `table:` renders as a table~~ — amended 2026-09-30: no ODF table markup occurs (§2) |
| `text:tab` | 63 docs, 212 | an element without children, in place | a shift of the line, never a table cell — owner's decision 2026-09-30 (§5 question 5) |
| mixed content | 23 616 docs | `Kind::Text` between elements, whitespace kept | **inline runs must stay inline** — the single largest layout risk |
| text nodes | — | `Kind::Text` | body text |
| `docID` / siglum | 23 936 | element `docID`; `sigla` from the scan | running head and heading |
| CTH number / folder | **0 as XML**; the folder name, 23 936 | nothing — see below; `cth`, `cth_num` from the folder | group, bookmark, table of contents |
| `AO:InvNr` | 2 774 docs | element `AO:InvNr`; `inv` from the scan | metadata block |
| editor, date | 23 646 docs carry an `editor` attribute | the attributes as written; `authorship`, `year` from the scan | metadata block and PDF metadata |
| `lg` language codes | 23 711 docs | the attributes as written; `lang` from the scan | metadata block; script selection |
| empty markers (`lb`, `gap`, `parsep`) | most | elements without children | line and section breaks — layout, not nothing |
| comments | 3 docs | `Kind::Comment` | ~~technical appendix; **not dropped**~~ omitted: not shown in the PDF — owner's decision 2026-09-30 (§5 question 3) |
| processing instructions | 8 423 docs | `Kind::Instruction`, in place | the stylesheet PI is dropped by rule; any other PI goes to the appendix |
| XML declaration | 442 docs | `Document::declaration`, `None` when absent | not shown; recorded in the manifest |
| ids | 600 duplicated | attributes as written; no uniqueness assumed | anchors; duplicates cannot be resolved by id alone and need the document path as well |
| unknown elements | possible | an element like any other — the model has no vocabulary to be unknown to | **must be visible in the appendix, never silently dropped** — *amended 2026-10-27:* what it holds is shown and the tag is not ("What a page shows"); there is no appendix |
| CDATA | 0 today | text, joined with its neighbours | text, as written |
| entity references beyond the five predefined | 0 today | the document is refused: no policy yet | expansion is a decision, recorded when made |
| DTD | 0 today | the document is refused: nothing fetched | not fetched, ever |
| XInclude | 0 today | an element like any other; not followed | not followed, ever |
| the original file | 23 936 | copied verbatim | see below |

**Nothing is dropped silently.** Where a construct is not displayed, the row
says where it goes instead, or names the decision that leaves it out of the
PDF — as for comments since 2026-09-30 (§5 question 3). *Amended 2026-09-30:*
this read "No category is 'ignored'" until the first such decision.

**Three rows of this table were wrong until 2026-09-13**, and building the model
is what showed it; measured over the archive that day.

- **No document carries a CTH number as XML.** The row said `CTHNr`, 23 936. No
  element or attribute of that name occurs anywhere; the CTH number comes from
  the folder the archive files a document under, which is what `parse` has
  always read. A `cth` element exists in 748 documents and is something else. So
  the requirement "group, bookmark, table of contents" rests on the archive's
  layout, not on the document, and the model rightly has nothing to say about
  it. This is a defect of the contract, not of the model: a value XML does not
  carry cannot be read out of XML.
- **`AO:InvNr` is in 2 774 documents**, not "most".
- **`parlbk` occurs nowhere.** The section markers of this corpus are `parsep`
  (72 989) and `parsep_dbl` (3 126).

### What a page shows

*Added 2026-10-27*, by the task markup-leak-fix and the owner's decisions of
2026-10-26 and 2026-10-27. Until then a page showed the text nodes of a
document and nothing else: the markup of a note was printed as letters, and
the signs of the edition the corpus writes as empty elements and attributes
were lost (`ZAG?-ni` printed `ZAG-ni`). The rules below are applied by
`cli/src/pdf/layout.rs` to one stream of events that `cli/src/pdf/markup.rs`
makes of the body, of notes and of `cu`, so a tag shows the same in all
three.

**Notes and `cu` are markup.** The corpus writes the text of a note in the
attribute `c` of an empty `note`, and a line of cuneiform in the `cu` of `lb`,
the markup inside escaped: on TLHdig Beta 0.3, 3 086 documents have markup or
an entity in a note and 84 in `cu`. Both are read by the parser of the
documents, as a fragment inside a root that binds `AO:` and `text:`. A
fragment that parser will not read – crossed tags, as in `CTH 434/KBo 20.82`,
a tag cut short – goes to a lenient reader: a tag is `<`, a name, anything but
`<` up to `>`, and everything else is text. Neither drops text, and neither
fails the document. A tag whose name has what is not a name in it – the
one in the corpus is `<wsepc="𒀹"/>` in the `cu` of `CTH 470/CHDS 7.200`,
which the parser of the documents, not checking names, takes for an element
named `wsepc="𒀹"` – is named by its run of name characters, and what
follows stands as text in its place, less `=`, quotes, `/` and whitespace:
the sign `𒀹` is shown (owner's decision of 2026-10-27, a rule for every
document, no exception for one).

`check` lists, as a warning and not a failure, every document whose notes,
`cu` or shown values carry a tag name that no rule and no name of
`VOCABULARY` knows, with the names and their counts (owner's decision of
2026-10-27). On TLHdig Beta 0.3 it lists one: `CTH 456/KUB 39.52+`, whose
`gap c="</w> <parsep/> <w>"` carries `parsep`, a structural tag of the body
that has no rule inside a value.

The corpus writes the same markup between `〈` and `〉` (U+2329, U+232A) as
well, in 216 documents. There a name of `VOCABULARY` (`markup.rs`; 84 names
measured in notes, 4 in `cu`, 4 added by the owner) is a tag; anything else
between the brackets is text, an emendation: `ku-〈e〉-da`.

**The value a rule shows is markup too** where it carries it – 56 values of
`c` of `gap`, `materlect` and `corr` on TLHdig Beta 0.3, `corr c="<ras_in/>"`
in `CTH 51/KBo 1.1+`. It is read on its own, apart from its line, and what
the rules make of it stands where the rule puts the value.

**The source of the signs.** This contract was silent on them. The rules come
from `HPMxml.css`, the stylesheet every document of the archive names in its
`xml-stylesheet` instruction (184 copies in the archive, byte for byte the
same, SHA-256 `f3b95c0c…`): how the portal itself shows a document. Where the
stylesheet is silent too, the owner decided (2026-10-27, C1–C7).

| construct | shown | source |
|---|---|---|
| a text node | its text | §4 above |
| `AO:TxtPubl` whose text is the `docID` | nothing | owner, 2026-10-26 (header) |
| `AO:DirectJoin` / `AO:InDirectJoin` | ` + ` / ` (+) ` | owner, 2026-10-27 (C3) |
| `lb` | a new line | §4 above |
| `cu` of `lb` | a line of its own, read as markup | owner, 2026-10-26 (notes) |
| `lnr` of `lb` | at the head of the next line, a space after it | `HPMxml.css` `lb:after`; owner, 2026-10-27 |
| `parsep`, `parsep_dbl` | a new section | §4 above |
| `AO:ParagrNr`, its `c` | a line of its own | owner, 2026-10-27 (C2) |
| `note`, its `c` | a footnote at its anchor, read as markup | owner, 2026-10-26 (notes) |
| the end of `P_f_…` in a note | a line break | owner, 2026-10-27 (paragraphs) |
| `del_in` / `del_fin` | `[` / `]` | `HPMxml.css` |
| `laes_in` / `laes_fin` | `⸢` / `⸣` | `HPMxml.css` |
| `corr`, its `c` | raised, 80 % | `HPMxml.css` `corr[…]:after` |
| `materlect`, its `c` | raised, 80 % | `HPMxml.css` `materlect::after` |
| `subscr`, its `c` | lowered, 90 % | `HPMxml.css` `subscr`; owner, 2026-10-27 |
| `surpl`, its `c` | `〈〈` value `〉〉` | `HPMxml.css` `surpl:before` |
| `gap`, its `c` | in the line; with `t="line"` a line of its own | owner, 2026-10-27 (C1) |
| `ras_in` … `ras_fin` | `⟦` … `⟧`; with nothing shown between, `(Rasur)`; one left open or closed alone, its bracket | owner, 2026-10-27 (C4) |
| `ras_X` | `(Rasur)` | C4, read for a mark with nothing inside |
| `add_in` / `add_fin` | `〈` / `〉` | owner, 2026-10-27 (C5) |
| `QUOT_HurInHit_in` / `_fin` | nothing; the words between as always | owner, 2026-10-27 (C6) |
| `wsep`, its `c` | the sign | owner, 2026-10-27 (C7) |
| `text:tab` | a shift, no character | §5 question 5 |
| any other element, a wrapper of a note, a name no rule knows | nothing; what it holds is shown | owner, 2026-10-26 (q1-2) |
| an entity in a value read as markup | its character | §4 above |
| comments, processing instructions | nothing | §5 question 3 |

The determinative `d` stays in the line, unraised, though `HPMxml.css`
raises it: the body is set without type styles, and a note as the body
(owner, 2026-10-26). Hurrian quotations wait for their own decision, with the
language of the PDF (`PROJECT-SPEC.ru.md` 7.3).

### Authenticity: the original beside the PDF

**Decided 2026-10-06 by the owner: option 2.** The XML stands next to the PDF
in the same folder, and the manifest records the pairing plus the SHA-256 of
the source. Options 1 and 3 are rejected, not to be revisited.

~~Three options, to be decided before the converter is written, not during:~~
*Amended 2026-10-06.*

1. ~~**Embed the XML as a PDF file attachment.** One file to move; every reader
   can extract it; nothing is lost. Costs about the size of the XML again
   (339 MB across the corpus) and not every viewer surfaces attachments.~~
   *Rejected 2026-10-06, not to be revisited.*
2. **Ship the XML next to the PDF** in the same folder, as the package already
   does, and record the pairing plus SHA-256 in the manifest. Costs nothing new
   — the package is already this — but the two can be separated.
   *Decided 2026-10-06.*
3. ~~**Reference only**: PDF metadata carries the source path and its SHA-256.
   Smallest, and useless if the corpus is not to hand.~~
   *Rejected 2026-10-06, not to be revisited.*

Recommendation is (2) with the digest, because the package already holds both
and the manifest already names both~~, and (1) as an option for single-document
export~~. This is written down here rather than chosen silently. *Amended
2026-10-06: the option for single-document export is rejected with option 1.*

Each PDF must in any case allow its source to be identified: original path,
SHA-256, converter version, run identifier, template version, and the status of
the completeness check.

### What the manifest says about the PDF and its source

*Amended 2026-10-11*, task pre-3.0.0-hardening, by the owner's decision of
2026-10-08 on option 2. Until this day the contract did not say what the
manifest carries for the PDF at all; the code did, and this is it, schema 3.

**The head.**

- `"schema": 3`. It was 1 from 2026-08-19 and there was never a 2: the PDF
  entries of 2026-09-30 were added under 1, since adding a field does not bump
  it; the jump to 3 is the owner's, for the shape of 3.0.0.
- `"run"`: `"id"`, `"aruna"` – the version of the program – and
  `"template"` – the version of the PDF template. The identifier is the
  SHA-256, in lower-case hex, of the UTF-8 text

  ```
  aruna-run-v1\n<archive_md5>\n<version of Aruna>\n<version of the template>\n
  ```

  with `archive_md5` as `"source"` gives it, every line ended by one `\n`,
  the last too. It is deterministic: one archive, one program, one template –
  one identifier, so two builds remain one package byte for byte. The template's
  version is the same with PDFs and without them, and the block is written in
  both modes, so the identifier can be computed again from the manifest alone.
  For TLHdig Beta 0.3 (`f9acbc8db3111cc7dd88d82f7819a912`), Aruna 2.6.3 and
  `aruna-pdf-template 1` it is
  `a55712da507f510c27c40fef5e80e7321d945b22b3ad957e55616c8be9fa32fd`,
  computed with `shasum -a 256` and held by
  `manifest::tests::the_run_identifier_is_the_sha256_of_its_canonical_form`.
- `"pdfs"`, only when the build made PDFs: `"template"`, `"built"`,
  `"refused"` – the tally of the phase; `built + refused` is the number of
  documents.

**Each document**, in `"groups"`:

- `"file"` – the XML in the package, and `"href"`, its link;
- `"sha256"` – the SHA-256 of that XML as written into the package, the
  normalised copy of §3.2;
- `"source_sha256"` – the SHA-256 of the entry as the archive gave it, before
  normalisation. The two differ wherever the prologue was rewritten – in TLHdig
  Beta 0.3 that is all 23 936 documents (measured 2026-10-11) – and would agree
  only where normalisation changed nothing. Both are taken from the bytes in hand as they are read and written,
  never by reading a file again; both are written always, with PDFs and
  without them;
- `"source_entry"` – *added 2026-10-13, by the owner's decision of
  2026-10-11*: the name of the archive entry whose bytes `"source_sha256"` is
  the digest of, exactly as the zip's catalogue spells it – no Unicode
  normalisation, `/` between components, the archive's own top folder
  included (`TLHbasisONLINE25_1_ZENODO_Beta_03/CTH 5_XML_HFR/…`). Written
  always, with PDFs and without them, and unique: two documents never name
  one entry. The name is the catalogue's bytes read as UTF-8, which every name
  of TLHdig Beta 0.3 is, although the archive sets the UTF-8 flag on none of
  its entries; a name that is not UTF-8 would be read as CP437, as the zip
  format says, and none is. The key is not `"source"`: the head's `"source"`
  describes the archive as a whole – its label, its MD5, the entries turned
  away – and a document's key of that name would read as a second
  description of it. `source_entry` pairs with `source_sha256`: the name and
  the bytes of one entry;
- `"pdf"` – with PDFs only: the path of the PDF, in the folder of the XML,
  when it was built, or `{ "refused": "<why>" }` when it was not. A refused document –
  the model does not read it, Typst refuses it, two repeated clusters in a row
  (owner's decision of 2026-10-02) – gets this record, and the build goes on
  (`PDF-ACCEPTANCE.md` §5). A broken invariant of this program is not a
  refusal: it stops the whole build and names the document and the invariant
  (owner's decision 11 of 2026-09-30);
- `"completeness"` – *added 2026-10-13, by the owner's decisions 1 and 2 of
  2026-10-11*: with PDFs, at each document whose PDF was built and only there,
  one line after `"pdf"`:

  ```
  "completeness": { "world": "passed", "labels": "passed", "clusters": "passed", "credit": "passed" },
  ```

  The checks (a)–(d) the export runs on every PDF (specification 4.10), under
  stable keys, with their outcome: `world` – Typst asked the world for
  `/main.typ` and `/doc.json` and nothing else; `labels` – every label found
  its place in the PDF; `clusters` – the ActualText wrapper stands at exactly
  every repeated cluster the general rule finds; `credit` – the footnote
  crediting UllikummiA was predicted from the font stack exactly where the
  subset is embedded. **The outcome is always `"passed"`**, and that is the
  rule, not the luck of a corpus: a failure of any of the four is a broken
  invariant of this program and stops the whole build (owner's decision 11 of
  2026-09-30), so a manifest with a failed check is never written. What the
  object records is which checks ran. The keys are the same four at every
  PDF: a document with no label gives `labels` nothing to place, and one with
  no combining mark of `U+0300–036F` gives `clusters` nothing to find – the
  post-processing returns such a PDF as Typst wrote it, and the property holds
  all the same. A refused document has no such object, and a build without
  PDFs has none at all. The semantic completeness of `PDF-ACCEPTANCE.md` §2 –
  the code points of the text, the tag tree and the logical text against the
  model – is not this check: it runs in the tests and at the release gate,
  and the limit of the PDF phase, 135 s, does not change.

~~**The outcome of the completeness check is not written yet.** The owner's
decision of 2026-10-08 places it in the entry of each document with a PDF, and
an incomplete PDF is to be a refusal of that document with its record, not a
stop. Which check it is, the decision does not say: the export runs only the
checks of near-zero cost, (a)–(d) of the specification's 4.10, and their
failure is a broken invariant, never a refusal; the semantic completeness of
`PDF-ACCEPTANCE.md` §2 runs in the tests and at the release gate only. The
choice and its price against the limit of the PDF phase are put to the owner
(specification 7.3).~~

*Amended 2026-10-13*, by the owner's decisions 1 and 2 of 2026-10-11: the
outcome is written – `"completeness"` above, the checks (a)–(d), always
passed. **An incomplete PDF** is what already refuses a document with
`"pdf": { "refused": … }`: Typst refuses it, the model does not read it, or it
holds two repeated clusters in a row. No new kind of refusal is introduced.

**Every package carries its PDF record** (owner's decision of 2026-10-18).
The manifest always has `"pdfs"`, and every document a `"pdf"` entry – the path
of its PDF with `"completeness"`, or why it was refused. The package check
refuses a manifest without the `"pdfs"` section, or with fewer or more reasons
for a refused PDF than documents without one
(`validate::a_manifest_without_its_pdf_record_fails`).

Where the items of the paragraph above now are: the original path –
~~**not written**: the manifest names the group, the siglum and the file in the
package, not the entry of the archive it came from (`root/CTH 5_XML_HFR/…`);
`"source_sha256"` identifies the entry's bytes, not its name, and whether the
name is wanted is put to the owner with the completeness check~~
`"source_entry"` (*amended 2026-10-13*); the relative
path – `"file"`; the SHA-256 – `"sha256"` and `"source_sha256"`; the converter
version – `"run"."aruna"` and the PDF's `creator`; the run identifier –
`"run"."id"`; the template version – `"run"."template"` and the PDF itself;
the status of the completeness check – ~~open, as above~~ `"completeness"`
(*amended 2026-10-13*).

---

## 5. ~~Policy needed before the converter is written~~ Policy for the converter

*Amended 2026-10-06.* The converter is written (§3.3); the questions below are
kept with their decisions.

Open questions, each of which changes what the converter does:

1. ~~**The 210 documents that are not well-formed.** Refuse and report? Recover
   with a lenient parser and mark the PDF as recovered? Produce a placeholder
   page naming the fault? Not deciding means deciding by accident.~~

   **Decided by measurement, 2026-09-05 and 2026-09-06: refuse and report.**
   Deterministic byte-level repair fixes none of the 206 this project's parser
   sees; error recovery corrupts four documents provably and says nothing; and
   the one class that looked local is five causes, of which exactly one repairs
   unambiguously. So the parser is strict on the conversion path and never
   recovers. The document itself stays in the package — a byte mirror needs no
   parse — and its name, reason and position are written into the manifest, so
   a reader sees what will not be converted and why. A lenient mode returns only
   with a measurement showing it does not alter text in silence. The
   measurements are in `PROJECT-SPEC.ru.md` §4.13; the requirement they replaced
   is struck in `PDF-ACCEPTANCE.md` §0, requirement 3.

   **Extended 2026-09-10 to the seventeen this parser accepts.** The
   decision above covers the 206 it refuses; the other seventeen would have
   reached the converter as ordinary documents and been mis-typeset without a
   word. They are now scanned for, listed in the manifest under
   `beyond_this_parser` with file, line and column, and the converter is to
   treat them exactly as the 206: refuse and report.

   **A known limit, recorded 2026-09-25.** "The model refuses a document if and
   only if the manifest names it" is proven on the corpus, not on any input. A
   document with a `DOCTYPE` or an entity reference is refused by the model and
   accepted by the classifier, so the manifest does not name it (finding T4,
   2026-09-22). `examples/fuzz_xml.rs` (`9c0fa32`) measured the limit wider: of
   200 000 byte-level mutations of a well-formed manuscript, fixed seed, the
   model alone refuses 6 879, in five classes: no root, content outside the
   root, an undeclared prefix, a construct with no policy, and 2 132 with a
   broken reference in an attribute value, which the model unescapes and the
   classifier does not (`Refusal::Unexplained`). No document of the corpus
   falls in any of them. **Owner's decision, 2026-09-25: widening the
   classifier waits for a release that moves the package's checksum anyway.** A
   new class is a new manifest key, and the manifest is part of the checksum.
2. **Entity expansion.** None appear today. When one does: expand and record,
   or refuse?
3. ~~**Comments.** Three documents carry them. Editorial or incidental?~~

   **Decided by the owner, 2026-09-30: omitted, not shown in the PDF.** The
   samples of the fourth PDF trial answered the question: all three —
   `CTH 627/IBoT 4.193`, `CTH 664/KBo 67.110`, `CTH 670/Privat 148` — sit in
   the header, in `AOHeader/meta/neu`, as the first node of the revision log,
   and all three are editors' notes about the file (`NN::/…`); no comment
   stands in the body. They stay in the package byte for byte; the PDF does not
   carry them (§4, comments row).
4. **The 600 duplicated ids.** Anchors need to be unique per PDF; the document
   path disambiguates them, and the manifest must say so.
5. ~~**OpenDocument table markup.** Rendered as tables, or as the transliteration
   rows they encode?~~

   **Decided by the owner, 2026-09-30: there are no tables; `text:tab` is a
   shift of the line.** The question assumed table markup the corpus does not
   contain — no element of the `table` namespace occurs (§2). The only
   OpenDocument markup that could be read as tabular is `text:tab`, and the
   trial's sample, `CTH 746/KUB 28.15`, set as a table showed what it is: five
   empty cells and the text in the sixth, a shift towards the right-hand column
   of the tablet rather than columns of data. It is rendered in the view
   "rows" as a shift of the line, never as table cells, and no character is
   added to the text for it (§2, §4). ~~How wide the shift is drawn is still
   open: the trial's "rows" view did not draw it at all (`PROJECT-SPEC.ru.md`
   7.3, question 4 of the design note).~~

   *Amended 2026-09-30, the owner's decision of that day:* each `text:tab` is
   drawn as a horizontal indent of fixed width, Typst's `h()`, which is space
   and not a character — nothing enters the text layer. The width is chosen
   against the trial's picture of `KUB 28.15` and recorded as a number in
   `PROJECT-SPEC.ru.md` 4.10.

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
- **78 documents carry combining diacritical marks (`U+0300–036F`).** A
  renderer that assumes composed diacritics will place marks wrongly in them.
  This is the number the manifest publishes as `documents_not_in_nfc`, and the
  name says more than the count: 13 of the 78 are in NFC (marks with no
  composed form), and **2 127 documents are not in NFC** in all, 2 062 of them
  without a single combining mark – mostly `U+2329`/`U+232A`, which NFC
  replaces (`docs/PDF-ACCEPTANCE.md` §2). Measured 04.10.2026 on the archive,
  Python 3.9.6, Unicode 13.0. The field keeps its name, since renaming it would
  change the manifest. Normalising the text would change it and is therefore
  not an option; the renderer must handle both forms.

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
| covered by the **declared** stack | 256 / 645 (40 %) | **639 / 645** |
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
| **Noto Serif Hebrew** | 1 sign — `U+05C3`, Hebrew punctuation, at most 26 documents. Not a system font; it ships in `cli/resources/fonts/`, OFL 1.1, `fsType` 0. |

**`Arial` stood in that last row until 2026-09-10 and is not a face of this
project any more.** It drew the same single sign, but it is commercial:
embedding it in a PDF this project distributes needs a licence from its owner,
and one code point out of 648 is not a reason to seek one. The replacement is
free, embeddable without condition, and in the same family as the main face.
Owner's decision 2026-08-30, `docs/PROJECT-SPEC.ru.md` §3.9; the code and this
table caught up on 2026-09-10. Coverage did not move: 639 of 645 before and
after, the same six left over.

The four above are the stack **both outputs share**. The main face is not among
them and is not the same on both sides: in HTML it is the system face, in PDF it
is `Noto Serif`, which ships in `cli/resources/fonts/` and is declared in §3.9
of the specification. Embedding a web font in a package of 24 000 files was
refused on the same day, so the two outputs differ in the main face on purpose
and agree on everything below it — see `docs/FONTS.md`.

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
