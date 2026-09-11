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

648 distinct code points, of which **645 need a glyph** and 382 are above the
Basic Multiligual Plane. Measured 2026-09-11.

| | |
|---|---|
| **drawn by a file of this repository** | **639 / 645** |
| also drawn by one of the four faces the shared stack names | 632 / 645 |
| drawn only by a font this machine happens to have | 0 |
| drawn by no font that draws characters | 5 |
| drawn only by a face this project refuses to name | 1 |

The six left over — the five and the one — are the whole of the remaining
problem and have a section of their own at the end. Everything else is settled.

**The number that matters is the first row, and it changed meaning on
2026-09-11.** Until that day three of the faces came from macOS, and what the
audit reported was what *this desk* could draw. Every face is now a file in
`cli/resources/fonts/`, the audit counts those files and nothing else, and the
row therefore answers the question the whole exercise is for: what will another
machine draw.

**Three code points left both sides of the ratio, and none of them is a
character.** The corpus contains `U+0009`, `U+000A` and `U+000D` — tab, newline
and carriage return. No font of this stack maps them and none should: they are
instructions to a layout engine, not glyphs. They counted as covered until
2026-09-11 because the stack resolved `system-ui` to macOS's own files, several
of which — `Geneva.ttf` among them — map them to a blank. So the old **642 of
648** and this **639 of 645** describe the same corpus: three non-characters
left the numerator and the denominator together, and **the six that cannot be
drawn are the same six as before.**

### The residual: seven characters that ride on the main face

**Seven characters are covered by the repository and by none of the four faces
the shared stack names.** They belong to the main face, which is `Noto Serif` in
PDF and the system face in HTML — so in the exported page those seven are drawn
by whatever the reader's system provides. It follows from the owner's decision
of 2026-08-30 not to embed a web font in a package of 24 000 files, and it is
recorded here rather than left to be discovered.

Counted over the corpus archive on 2026-09-11:

| | occurrences | documents | of the corpus |
|---|---:|---:|---:|
| `U+02FD` modifier letter shelf | **13 260** | **1 991** | **8.3 %** |
| `U+2093` subscript small x | 339 | 133 | 0.6 % |
| `U+2E22` top left half bracket | 36 | 26 | 0.1 % |
| `U+2E23` top right half bracket | 35 | 25 | 0.1 % |
| `U+2E17` double oblique hyphen | 6 | 1 | 0.004 % |
| `U+0341` combining acute tone mark | 2 | 1 | 0.004 % |
| `U+206F` nominal digit shapes | 2 | 1 | 0.004 % |

**One of the seven is the whole of the problem, and it is not one of the ones
the project worried about.** `U+02FD` is in 1 991 documents — one in twelve of
the corpus — and occurs 13 260 times. That is **more often than any cuneiform
sign this document has ever discussed**: `U+100009`, the most frequent of the
five nothing draws, occurs 2 715 times in 976 documents, and `U+100000`, the
sign the repository carries a whole font for, occurs 927 times in 370.

The tempting sentence about it is that `U+02FD` is an ordinary modifier letter,
present in most text faces, so in practice it will almost certainly render.
**Measured on this machine, that sentence is false.** Of the 358 font files
macOS 13 ships, 23 carry `U+02FD` — and the interface font is not among them.
`system-ui` on macOS resolves to San Francisco, `SFNS.ttf` and its siblings, and
San Francisco does not have this character. It reaches the page only because the
browser walks past the main face into some other installed font — Geneva,
Monaco, one of the Arials. Which one, the reader never learns and nobody chose.

"Almost certainly renders" is exactly the formulation this project abandoned on
2026-08-22, when the audit showed that three fifths of the corpus was being
drawn by substitutions nobody had selected. It is not a better sentence for
being about one character instead of 389.

**The other six are rare, and the half brackets are the thinnest thread of all.**
Together they account for 420 occurrences across 187 documents — against the
13 260 of the first. By frequency the half brackets `U+2E22`/`U+2E23` were
feared out of proportion: 71 occurrences in 51 documents. By availability they
are the opposite of safe — **exactly one file in the whole of macOS 13 carries
either of them, `Geneva.ttf`**, a face from the bitmap era that survives for
compatibility. One deletion upstream and the half brackets of this corpus stop
rendering on every Mac.

The rest are drawn by something on this system: `U+0341` by 60 files, `U+2093`
by 22 — and it is the one character of the seven that San Francisco does carry —
`U+2E17` by 18, `U+206F` by 12.

### What can be proved about each output, and what cannot

This is the part worth stating plainly, because it is a property of the shape of
the decision rather than of any font.

**In PDF the question is closed by measurement.** The face is embedded: the file
travels with the document, it carries its own `cmap`, and asking whether a
character will be drawn is asking what is in a file that is right there. The
answer is the same on every machine that opens it, forever, and
`cli/examples/font_coverage` gives it today.

**In HTML there is no such question to ask.** The main face is the reader's
system face, chosen by their operating system and its version, and the fallback
chain after it is chosen by their browser. Nothing in this repository can
measure it, no test can hold it, and no audit run here says anything about the
machine the page is read on. **A hole in the exported page becomes known only if
a reader writes to say they see a box.**

That asymmetry is not an argument against the decision of 2026-08-30. Its ground
has not moved: the package holds some 24 000 files and a web font would add
weight to every one of them. What has moved is the price. The decision was made
about the appearance of running prose — a serif here, the system's face there,
a difference a reader would call typography. It now also covers characters of
the scholarly notation: a half bracket is not a matter of taste, and an editor
who sees `⸢` as an empty box has lost a distinction the transliteration is
making. The decision stands; what it costs is written down.

`docs/PROJECT-SPEC.ru.md` §7.3 carries this as an open position with a condition
that measurement can close: whether the seven are covered by the system fonts of
the platforms the inventory actually reaches. macOS is measured above and all
seven are drawable there; Windows and the common distributions are not, and
until they are the answer is partial.

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

**None of the four comes from the operating system any more.** Until 2026-09-11
three of them did — the cuneiform, the editorial marks and, before it was
replaced, `Arial` — and `docs/XML-CONTRACT.md` names that arrangement as the
reason this work exists: "it looks right here" is not evidence about anybody
else's machine. All four are files of this checkout now, and the audit counts
nothing else.

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
| file | `NotoSansCuneiform-Regular.ttf`, version 2.001 |
| where | **not a system font any more** — `cli/resources/fonts/`, since 2026-09-11 |
| SHA-256 | `aad6f345a2f3150aeb51706ecf1d6f62eec299ee215cb77e76f0c33e1419bba2` |
| copyright | The Noto Project Authors, 2022 |
| licence | SIL Open Font License 1.1, text beside the file in `cli/resources/fonts/OFL-NotoSansCuneiform.txt` |
| `fsType` | `0x0000` — Installable: embeddable in a PDF without condition |
| origin | monthly release `noto-monthly-release-2026.05.01`; the licence text from `notofonts/cuneiform` |

**Version 2.001 from the release, not the 2.000 macOS carries, and the choice
was measured rather than assumed.** Both files were read and compared against
the corpus on 2026-09-11: each covers 378 of its code points, of which 376 are
cuneiform, and **the two sets are identical — no code point is in one and not
the other.** So the choice costs nothing in coverage and buys two things. The
release version is traceable to a tag, like the other three Noto files here, and
it does not change when the operating system updates: `2.000;GOOG;noto-source:20181019`
is what this Mac happens to ship in 2026, and the next `softwareupdate` is free
to make it something else without telling anyone.

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
| file | `STIXTwoMath-Regular.otf`, version 2.13 b171 |
| where | **not a system font any more** — `cli/resources/fonts/`, since 2026-09-11 |
| SHA-256 | `3a5f3f26f40d5698b3c62dd085d48d6663696a3f80825aab8b553d5097518e8c` |
| copyright | The STIX Fonts Project Authors, 2001–2021; *STIX Fonts* is a trademark of the IEEE |
| licence | SIL Open Font License 1.1, text beside the file in `cli/resources/fonts/OFL-STIXTwo.txt` |
| `fsType` | `0x0000` — Installable |
| origin | release `v2.13b171` of the STIX project |

**The release file and the system file are the same bytes.** Both were hashed on
2026-09-11 and both are
`3a5f3f26f40d5698b3c62dd085d48d6663696a3f80825aab8b553d5097518e8c`: macOS ships
the project's own file without rebuilding it. So moving this face into the tree
changes nothing about what is drawn, and everything about what the claim rests
on — a file with a release tag beside it rather than a file that arrives with an
operating system and leaves with it.

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

## The seven files this repository carries

Everything in `cli/resources/fonts/`, with what verifies it. **Since 2026-09-11
the stack takes nothing from the operating system**: every face below is a file
of this checkout, and `cli/src/style.rs::the_font_stack_names_what_the_corpus_needs`
fails the build if any of them is absent or if the stack names a family no file
here answers to.

| file | version | SHA-256 | licence, text beside it | `fsType` |
|---|---|---|---|---|
| `UllikummiA.ttf` | 1.003 | `2ca4357d66d7cde6b0785be22f4c3ed3427289fdb0330eceabe89da24c4041cf` | Hethitologie-Portal Mainz terms, `UllikummiA-TERMS.txt` | **`0x0008`** — Editable: embedding and subsetting permitted |
| `NotoSansCuneiform-Regular.ttf` | 2.001 | `aad6f345a2f3150aeb51706ecf1d6f62eec299ee215cb77e76f0c33e1419bba2` | OFL 1.1, `OFL-NotoSansCuneiform.txt` | `0x0000` |
| `STIXTwoMath-Regular.otf` | 2.13 b171 | `3a5f3f26f40d5698b3c62dd085d48d6663696a3f80825aab8b553d5097518e8c` | OFL 1.1, `OFL-STIXTwo.txt` | `0x0000` |
| `NotoSerif-Regular.ttf` | 2.015 | `19e72cd8d595fae5bd74a5206f5d938512e1183d4fed7abb1ec1be1d7efa5f88` | OFL 1.1, `OFL-NotoSerif.txt` | `0x0000` |
| `NotoSerif-Italic.ttf` | 2.015 | `749e80e313ef711f9373c6cce17c72297ef05490b3dcda7967d1d5d90bf1183f` | OFL 1.1, `OFL-NotoSerif.txt` | `0x0000` |
| `NotoSerif-Bold.ttf` | 2.015 | `96656aa5cec8f1d6fd0e804c1fad397e1a1cfa082e6642124e0bda68cd8363ce` | OFL 1.1, `OFL-NotoSerif.txt` | `0x0000` |
| `NotoSerifHebrew-Regular.ttf` | 2.004 | `dfd5a6aefe97a99f68fe43388342913d50bb9fbf6d3afc4d2c7725661bc4a2b1` | OFL 1.1, `OFL-NotoSerifHebrew.txt` | `0x0000` |

**One of the seven permits embedding conditionally and six without condition.**
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

**Nothing has to be installed, and that is the point of 2026-09-11.** All seven
files come with the checkout, the audit reads them from `cli/resources/fonts/`,
and a clone on a machine that has never heard of Hittitology reports the same
639 of 645 as this one. Before that day three faces came from macOS and the
audit's answer was a property of the desk it ran on.

Installing remains useful for exactly one thing, and it is worth keeping the two
apart:

```sh
# only to read the exported HTML in a browser on this machine
cp cli/resources/fonts/UllikummiA.ttf              ~/Library/Fonts/
cp cli/resources/fonts/NotoSansCuneiform-Regular.ttf ~/Library/Fonts/
cp cli/resources/fonts/STIXTwoMath-Regular.otf     ~/Library/Fonts/
cp cli/resources/fonts/NotoSerifHebrew-Regular.ttf ~/Library/Fonts/
```

A browser resolves a CSS family against the fonts the system offers it; it has
no idea this repository exists. So the four faces of the shared stack have to be
installed for the exported page to look on this machine the way the stack says
it should — and that is a statement about reading the page here, not about the
package being complete. The three `NotoSerif-*.ttf` are deliberately not on that
list: they are the main face of the PDF stage, nothing in the HTML path consults
them, and installing them would suggest they belong to the shared stack.

The upstream route below is not an install path either; it is the cross-check —
how to confirm that what this repository carries is what the portal publishes.

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
# expect: "BY THE REPOSITORY'S FILES     639 of 645"
```

Anything below 639 means a file is missing or has changed, and the program names
which code points went with it. A face the stack names that resolves to a system
file rather than to one of ours is reported as a failure with a non-zero exit,
not as a pass: it is the state those files exist to end.

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
