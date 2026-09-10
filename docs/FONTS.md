# The font environment

What has to be installed for this corpus to render correctly, where each face
comes from, and how to check that the copy you have is the copy this was
verified against.

Everything below was measured, not assumed. Reproduce it with:

```sh
cd cli
cargo run --release --example font_coverage -- fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip
```

That program reads the `cmap` table of every font installed on the machine and
compares it against every code point the archive actually contains. It takes the
declared stack out of the built canonical section rather than carrying its own
copy, so the specification below and the documents cannot drift apart.

## Coverage

648 distinct code points, 382 of them above the Basic Multilingual Plane.

| | |
|---|---|
| drawn by a face the stack names | **642 / 648** |
| drawn only by a font this machine happens to have | 0 |
| drawn by no font that draws characters | 6 |

The six are the whole of the remaining problem and have a section of their own
at the end. Everything else is settled.

**How much of the corpus each touches**, counted over the 23 936 documents the
package holds (2026-08-24):

| | documents |
|---|---|
| carry cuneiform proper, U+12000–U+1247F | 23 581 |
| carry one of the five no font draws | **990** — and 976 of those are U+100009 alone |
| carry U+100000, which only an installed `UllikummiA` draws | 370 |
| carry U+E83A, which only a face this stack refuses would draw | 1 |

**Rendered rather than inferred, 2026-08-24.** The `cmap` audit says a glyph
exists; it does not say a browser picks it. Both were checked in Chrome against
a document of the real corpus: cuneiform renders identically with the declared
stack and with no font declared at all — macOS falls back to Noto Sans
Cuneiform, which is a system face — so the package's XML documents display
correctly even though they carry no stylesheet. The five uncovered points
render as LastResort's striped box, exactly as the audit predicts.

The wording of the last row is deliberate. On macOS those six are not blank:
`LastResort.otf` puts a placeholder box in their place, which is a marker
meaning *nothing can render this* rather than a rendering. The audit keeps that
apart from coverage and says so for each gap — see *What a reader sees instead*
below.

## The four faces of the shared stack, and why each is there

These four are the stack both outputs share. The main face is not among them —
it is the one thing HTML and PDF are allowed to differ in, and it has a section
of its own below.

The stack is declared in **two** places, and both must name the same four faces:

| where | what it covers |
|---|---|
| `frontend/src/inventory/canonical.css` | the exported HTML — the source of truth, carrying the full reasoning and the measurements |
| `frontend/src/app.css` | the application window, as the `--corpus` custom property |

`--corpus` is composed into all three of the frontend's stacks — `--sans`,
`--heading` and `--mono` — because every font declaration in that application
goes through those variables and cuneiform can appear under any of them. It sits
before the generic family in each, so a machine without the faces degrades to
fallback rather than refusing to render.

> The mirror moved on 2026-08-23. It used to live in `desktop/src/app.css`,
> which was deleted with `desktop/`; for the length of that day the new
> application declared no cuneiform faces at all and its window would have shown
> platform substitutions. Restored the same day.

`cli/examples/font_coverage.rs` is what verifies a machine actually has the
faces. In the order a browser consults them:

### Noto Sans Cuneiform

| | |
|---|---|
| covers | 376 signs — every standard cuneiform character in the corpus, across 19 021 documents |
| CSS family | `Noto Sans Cuneiform` |
| file | `NotoSansCuneiform-Regular.ttf` |
| where | `/System/Library/Fonts/Supplemental/` — ships with macOS 13 |
| licence | SIL Open Font License 1.1 |
| `fsType` | `0x0000` — Installable: embeddable in a PDF without condition |

Named first among the cuneiform faces deliberately. It is what these documents
render with today, so naming it changes nothing about how they look; putting the
Hittitological face first instead would change the appearance of 19 021
documents, which is a decision about the corpus rather than part of fixing
coverage.

### UllikummiA

| | |
|---|---|
| covers | 1 sign — `U+100000`, in 927 places across 913 lines |
| CSS family | `UllikummiA` |
| file | `UllikummiA.ttf`, version 1.003 |
| where | **not a system font** — it ships in this repository, `cli/resources/fonts/UllikummiA.ttf`, since 2026-09-09 |
| author | Sylvie Vanséveren, 2007; the font's own `name` table says "All rights reserved" and carries no licence field |
| licence | Hethitologie-Portal Mainz terms, quoted verbatim in `cli/resources/fonts/UllikummiA-TERMS.txt`: **may not** be modified, distributed in modified form, or distributed commercially; **may** be used for academic and research purposes, in scientific publications and in websites for scholarly purposes, with a credit |
| `fsType` | `0x0008` — Editable: the font permits embedding in a document, and permits subsetting |

`U+100000` is a cuneiform sign with no Unicode code point. Vanséveren allocated
it in the Supplementary Private Use Area and documented it in the Hittite Sign
List that accompanies the fonts, where it appears in the table beside ordinary
`U+12xxx` signs. **No other font on this machine draws it** — not Semiramis, not
Noto, not any of the 366 faces macOS ships.

The two questions that matter for the PDF stage are answered rather than
deferred. The written terms permit use "in scientific publications", and the
font's own `fsType` bit — the machine-readable statement PDF tools read — is
`0x0008`, Editable Embedding, which permits embedding *and* subsetting. A
scholarly, non-commercial PDF that embeds a subset of this face is within both.

`UllikummiB` and `UllikummiC` are part of the same package and are **not**
needed: neither covers a single code point this corpus uses that `UllikummiA`
does not.

### STIX Two Math

| | |
|---|---|
| covers | 6 signs — `U+24F5`–`U+24F8`, the double-circled digits used as editorial marks, and `U+27E8`/`U+27E9`, the angle brackets |
| CSS family | `STIX Two Math` |
| file | `STIXTwoMath.otf` |
| where | `/System/Library/Fonts/Supplemental/` — ships with macOS 13 |
| licence | SIL Open Font License 1.1 |
| `fsType` | `0x0000` — Installable |

### Noto Serif Hebrew

| | |
|---|---|
| covers | 1 sign — `U+05C3`, Hebrew punctuation sof pasuq, in at most 26 documents |
| CSS family | `Noto Serif Hebrew` |
| file | `NotoSerifHebrew-Regular.ttf`, version 2.004 |
| where | **not a system font** — it ships in this repository, `cli/resources/fonts/`, since 2026-09-10 |
| SHA-256 | `dfd5a6aefe97a99f68fe43388342913d50bb9fbf6d3afc4d2c7725661bc4a2b1` |
| copyright | The Noto Project Authors, 2022 |
| licence | SIL Open Font License 1.1, text beside the file in `cli/resources/fonts/OFL-NotoSerifHebrew.txt` |
| `fsType` | `0x0000` — Installable: embeddable in a PDF without condition |
| origin | monthly release `noto-monthly-release-2026.05.01`; the licence text from `notofonts/hebrew`, the repository the font's own rights field names |

A narrow file — 146 code points — and that is the whole of what is wanted from
it. Checked in its own `cmap` rather than assumed: `U+05C3` is there, and so are
all 27 letters of the alphabet, so a Hebrew word appearing in editorial prose
one day would render rather than half-render.

**It replaced `Arial` on 2026-09-10, and `Arial` is gone rather than demoted.**
Arial drew the same single sign and its `fsType` did permit embedding, but it is
a commercial face: embedding it in a PDF this project distributes needs a
licence from its owner, and one code point out of 648 does not buy that
conversation. The replacement is in the same family as the main face below, is
under OFL, and carries `fsType` 0. The decision is the owner's, 2026-08-30, and
it is recorded in `docs/PROJECT-SPEC.ru.md` §3.9; the code caught up on
2026-09-10.

### Noto Serif — the main face, and only for PDF

| | |
|---|---|
| covers | the running text: Cyrillic and the Latin diacritics of the transliteration |
| CSS family | `Noto Serif` |
| files | `NotoSerif-Regular.ttf`, `NotoSerif-Italic.ttf`, `NotoSerif-Bold.ttf`, all version 2.015 |
| where | **not a system font** — `cli/resources/fonts/`, since 2026-09-10 |
| SHA-256 | Regular `19e72cd8d595fae5bd74a5206f5d938512e1183d4fed7abb1ec1be1d7efa5f88` · Italic `749e80e313ef711f9373c6cce17c72297ef05490b3dcda7967d1d5d90bf1183f` · Bold `96656aa5cec8f1d6fd0e804c1fad397e1a1cfa082e6642124e0bda68cd8363ce` |
| copyright | The Noto Project Authors, 2022 |
| licence | SIL Open Font License 1.1, text in `cli/resources/fonts/OFL-NotoSerif.txt` |
| `fsType` | `0x0000` — Installable, all three |
| origin | monthly release `noto-monthly-release-2026.05.01`; the licence text from `notofonts/latin-greek-cyrillic` |

**This face is not in the stack above, and its absence there is a decision, not
an omission.** The stack above is the one both outputs share. The main face is
the one they are allowed to differ in: in HTML it is the system face —
`system-ui` and its platform aliases — and in PDF it is `Noto Serif`. Embedding
a web font into a package of some 24 000 files was refused by the owner on
2026-08-30, because it would add weight to every one of them for an appearance
most machines already give.

So the answer to "where is the PDF's main face declared, if not in the
stylesheet" is: in `docs/PROJECT-SPEC.ru.md` §3.9, which states it, and in this
table, which names the files, their sums and their licence. Not in
`canonical.css`, and the test in `cli/src/style.rs` says so in as many words so
that a reader of the code cannot conclude it was forgotten.

Chosen for a reason rather than a taste: `Noto Serif` is related to
`Noto Sans Cuneiform`, already in the stack, so the step from ordinary text into
cuneiform does not jump in weight or x-height, and its coverage of Cyrillic and
extended Latin diacritics is complete. Three cuts ship because a scholarly page
needs italic for sigla and bold for headings; nothing else of the family is
here.

## The five files this repository carries

Everything in `cli/resources/fonts/`, with what verifies it:

| file | version | SHA-256 | licence, text beside it | `fsType` |
|---|---|---|---|---|
| `UllikummiA.ttf` | 1.003 | `2ca4357d66d7cde6b0785be22f4c3ed3427289fdb0330eceabe89da24c4041cf` | Hethitologie-Portal Mainz terms, `UllikummiA-TERMS.txt` | **`0x0008`** — Editable: embedding and subsetting permitted |
| `NotoSerif-Regular.ttf` | 2.015 | `19e72cd8d595fae5bd74a5206f5d938512e1183d4fed7abb1ec1be1d7efa5f88` | OFL 1.1, `OFL-NotoSerif.txt` | `0x0000` |
| `NotoSerif-Italic.ttf` | 2.015 | `749e80e313ef711f9373c6cce17c72297ef05490b3dcda7967d1d5d90bf1183f` | OFL 1.1, `OFL-NotoSerif.txt` | `0x0000` |
| `NotoSerif-Bold.ttf` | 2.015 | `96656aa5cec8f1d6fd0e804c1fad397e1a1cfa082e6642124e0bda68cd8363ce` | OFL 1.1, `OFL-NotoSerif.txt` | `0x0000` |
| `NotoSerifHebrew-Regular.ttf` | 2.004 | `dfd5a6aefe97a99f68fe43388342913d50bb9fbf6d3afc4d2c7725661bc4a2b1` | OFL 1.1, `OFL-NotoSerifHebrew.txt` | `0x0000` |

**One of the five permits embedding conditionally and four without condition.**
`UllikummiA` carries `0x0008`, Editable Embedding — embedding *and* subsetting
are permitted, which is what a PDF needs, and the written terms beside it permit
scholarly use with a credit. The four Noto files carry `0x0000`, Installable,
which permits everything. Read out of each file's own `OS/2` table on
2026-09-10, not taken from a catalogue.

## Deliberately not named

**`Hiragino Sans GB`** would close one more code point, `U+E83A`, and must not be
used for it. It "covers" that number only in the sense that its own private-use
area holds an unrelated Chinese glyph there. Naming it would make a foreign sign
the official rendering of a TLHdig character — a plausible-looking wrong answer,
which is worse than the placeholder box it would replace, and which no check
would ever report. `cli/src/style.rs::the_font_stack_names_what_the_corpus_needs`
refuses it.

**`UllikummiB`, `UllikummiC`, `Semiramis Unicode 3`** are absent because they add
nothing: measured against this corpus, they cover no code point the four faces
above do not.

## Installing — reproducing this environment on another machine

Two of the four faces ship with macOS 13 and need nothing. **Two have to be
installed, and both come with the checkout:**

```sh
cp cli/resources/fonts/UllikummiA.ttf          ~/Library/Fonts/
cp cli/resources/fonts/NotoSerifHebrew-Regular.ttf ~/Library/Fonts/
```

That is the whole of it on a fresh machine. The three `NotoSerif-*.ttf` are
deliberately **not** on that list: they are the main face of the PDF stage, they
are read from the repository by whatever renders, and nothing in the HTML path
consults them — installing them would change nothing and would suggest they are
part of the shared stack.

Measured on 2026-09-10, and worth stating because it is the shape of the
failure: with `NotoSerifHebrew-Regular.ttf` present in the repository but not
copied into `~/Library/Fonts`, the audit reports **641 of 648** and names the
missing face. The one point that goes with it is `U+05C3`. Copying the file
restores 642 — the same 642 as before the face changed, and the same six left
over. The upstream route below is no
longer the install path; it is the cross-check — how to confirm that what this
repository carries is what the portal publishes.

```sh
curl -LO https://hethport.net/cuneifont/download/Ullikummi.zip
shasum -a 256 Ullikummi.zip
# 28f8bb7ebc572009760066373edbf730c5bbcc2e974ec85109a6a44e5a2e55c7
unzip Ullikummi.zip
shasum -a 256 UllikummiA.ttf cli/resources/fonts/UllikummiA.ttf   # must agree
```

Verify what you installed:

| file | SHA-256 |
|---|---|
| `Ullikummi.zip` | `28f8bb7ebc572009760066373edbf730c5bbcc2e974ec85109a6a44e5a2e55c7` |
| `UllikummiA.ttf` | `2ca4357d66d7cde6b0785be22f4c3ed3427289fdb0330eceabe89da24c4041cf` |
| `UllikummiB.ttf` | `1c9213f771712192dc2a121e128bfc32c5c5e1bc1c5ee1d2b16ce7120775d6e3` |
| `UllikummiC.ttf` | `ee2ccaa1a1449e1f97af739a301e680fcd555b56b466f8e019488ca5b2c4506e` |
| `NotoSerifHebrew-Regular.ttf` | `dfd5a6aefe97a99f68fe43388342913d50bb9fbf6d3afc4d2c7725661bc4a2b1` |

Then confirm the machine is correct rather than trusting the copy:

```sh
cd cli
cargo run --release --example font_coverage -- fixtures/…zip
# expect: "by the declared font stack   642 of 648"
```

Anything below 642 means a face is missing, and the program names which code
points went with it.

**`UllikummiA.ttf` is committed to this repository as of 2026-09-09, and the
sentence that stood here said the opposite.** It read: "the licence permits use,
not redistribution". That was an overreading of the terms rather than a reading
of them. The portal's Terms of Use — quoted verbatim in
`cli/resources/fonts/UllikummiA-TERMS.txt`, read at `hethport.net/cuneifont/` on
2026-09-09 — prohibit three things: modification, distribution in any modified
form, and commercial distribution. They grant four: academic purposes, research
purposes, scientific publications, and websites for scholarly purposes.
Distribution of the *unmodified* font for a non-commercial scholarly purpose is
in neither list.

The owner decided on 2026-09-09 that this copy falls inside the terms: the use
is academic and scholarly, this repository is non-commercial, the file is
unmodified — SHA-256 `2ca4357d…41cf`, the same digest as the upstream package
and as the copy installed on the machine that recorded it — and the required
credit travels with it, in `README.md`, in the terms file and in this document.
That is the owner's reading, recorded with its date and reasons, not a legal
finding and not a permission granted to us in writing. If the author or the
portal says otherwise, the file goes and the `curl` route below is again the
only route.

`cli/tests/fonts.rs` holds the file to its length and digest on every run: the
terms forbid modifying it, and an edited font would breach them in silence.

**The exported inventory tells its reader, and the package carries no font.**
The summary block of `TLHdig_Beta_0.3.html` names the sign, says no operating
system draws it, points at `cli/resources/fonts/` and carries the credit the
terms require. Nothing beyond that line, and the reason is a measurement rather
than a preference: the inventory holds **no** occurrence of `U+100000` and no
cuneiform character at all — checked 2026-09-09 by emitting the catalogue of all
23 936 manuscripts and counting — because all 927 occurrences live in the `cu`
attribute of `<lb>` inside the documents themselves. Those the package mirrors
byte for byte, and a byte mirror carries no stylesheet, so an `@font-face` rule
would have styled a sign that is nowhere on the page it was added to. The reader
installs the font, or does not, and the line says which it is.

The other three faces — `UllikummiB`, `UllikummiC`, `Semiramis Unicode 3` — are
still not here, for the reason given above: they cover nothing this corpus uses.

The credit the terms require:

> Fonts created by Sylvie Vanséveren, available on the Hethitologie Portal Mainz

**Network note.** `hethport.uni-wuerzburg.de` refuses the TLS handshake from
this machine (LibreSSL 3.3.6, macOS 13). Two routes work and serve the same
content:

- **`hethport.net`** — the portal under its other name, reachable and complete;
- the DARIAH mirror, `https://smaw.de.dariah.eu/cuneifont/download/Ullikummi.zip`,
  which served a package with the identical SHA-256.

The copy already installed here matches both, byte for byte.

## The six that cannot be resolved here

These are the whole of what is left, and none of them is a defect in this
program.

### Five undocumented cuneiform signs

`U+100001`, `U+100003`, `U+100005`, `U+100006`, `U+100009`.

They appear inside the `cu=` attribute — the sign-by-sign cuneiform rendering of
a line — interleaved with ordinary `U+12xxx` signs. So they are cuneiform
characters that TLHdig encodes privately because Unicode has no code point for
them.

| | occurrences | lines |
|---|---|---|
| `U+100009` | 2 715 | 2 379 |
| `U+100003` | 13 | 13 |
| `U+100006` | 3 | 3 |
| `U+100001` | 1 | 1 |
| `U+100005` | 1 | 1 |

**No font draws them.** Not the official Ullikummi package downloaded from the
portal, not Semiramis, not any of the 366 faces on this machine. And they are
not in the Hittite Sign List either: that document allocates exactly three
private-use points — `U+100000`, `U+100007`, `U+10000A` — and the corpus uses
only the first of them.

#### What a reader sees instead: LastResort

They do not come out blank. macOS ships `/System/Library/Fonts/LastResort.otf`,
and it is what draws them — which is also why `fileformat.info` and
`compart.com` show a box for `U+100009` rather than nothing.

It is not coverage, and the distinction is the whole point. Its `cmap` is four
groups, and one of them maps **`U+E000..U+10FFFF` — 1 056 768 code points — to a
single glyph**, the same glyph it uses for `U+0000..U+D7FF`. The file is 2 468
bytes. What it draws is a placeholder meaning *nothing here can render this*.
Counting it as coverage would report 648 of 648 for any corpus and any font
stack, and mean nothing at all.

So `font_coverage` reads it, keeps it in a category of its own, and names it
beside each gap:

```
no font here draws these:
  U+100009  → shown as a placeholder by LastResort.otf
```

**This was a defect in the audit and is worth recording as one.** Until
2026-08-22 the program parsed only `cmap` formats 4 and 12 and dismissed the
rest with a comment claiming they carried nothing this corpus uses. Format 13 —
whose purpose in the OpenType specification is precisely last-resort fonts — was
among them, and `LastResort.otf` is the only file on the system that uses it
*exclusively*. It was therefore the one font of 366 the audit could not see at
all. Measured after the fix: every other file carrying a legacy format (0, 2, 6
or 14) also carries a format 4 or 12 subtable, so nothing else was missed.

For the PDF stage this matters twice over. A renderer that falls back to
LastResort produces a PDF with placeholder boxes — honest, and better than a
blank, but not the sign. A renderer that does *not* have it produces nothing at
all. Either way the five signs are absent, and the question below is unchanged.

So TLHdig's private-use allocation goes beyond what its font provider has
published. **This can only be answered by the TLHdig editors**: what these five
signs are, and which font renders them. Until it is answered a PDF will have a
hole in 2 379 lines, and no amount of work in this repository will change that.

`docs/TLHDIG-ANFRAGE.de.md` is a drafted enquiry to them, in German, carrying
the counts and the cited passages. When it is answered, the answer belongs
here.

What must *not* be done: substituting a similar-looking sign, or dropping the
characters. The source XML is not ours to change, and a wrong sign that renders
is worse than a missing one that does not.

### One stray character in editorial prose

`U+E83A`, twice, and — unlike the five above — **not in any `cu=` attribute**.
It sits in a footnote, in German prose about a photograph collation. It is
almost certainly a leftover from a legacy font encoding rather than a sign
anybody intended.

It is left alone for the same reason: it is in the source, and the source is not
ours to correct. It renders as nothing, which is the honest outcome, and the one
font that would draw something there would draw the wrong thing.

## Sources

- [Unicode Fonts for Cuneiform](https://www.hethport.uni-wuerzburg.de/cuneifont/) — Sylvie Vanséveren, Hethitologie-Portal Mainz
- [The same, DARIAH mirror](https://smaw.de.dariah.eu/cuneifont/) — reachable when the first is not
- [TLHdig](https://www.hethport.uni-wuerzburg.de/TLHdig/) — the corpus this describes
- `SignLists/HittiteSignList.pdf`, in the portal's `SignLists.zip` — where the private-use allocation is published
- [notofonts/latin-greek-cyrillic](https://github.com/notofonts/latin-greek-cyrillic) — `Noto Serif` and its OFL text, named in the font's own rights field
- [notofonts/hebrew](https://github.com/notofonts/hebrew) — `Noto Serif Hebrew` and its OFL text, likewise
- `noto-monthly-release-2026.05.01` — the release the four files here were taken from
