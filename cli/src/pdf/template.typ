// The one template of the PDF (specification 4.10). Static: the document
// arrives as data in /doc.json, read by the one data loader below, never spliced into
// markup. What it sets, and the tag each part becomes (reading 1 of the
// structural criterion, PDF-ACCEPTANCE §2):
//
//   the credit, when UllikummiA is used   a float at the foot of page one   P
//   folder CTH and its title              heading, level 1                  H1
//   file name, the siglum                 heading, level 2                  H2
//   a run between parsep markers          a one-column grid of one cell     Div > Div
//   a line at lb, and its cu              par                               P
//   corr, materlect / subscr              super / sub                       (none)
//   note, by its c, at its anchor         footnote without a number         Note
//     a paragraph of the note             a line, linebreak between         (none)
//   a code point no face draws            a label, an image                 Figure
//   text:tab                              h(2em), space and not a character
//
// Series of signs with no break opportunity are boxed sixteen at a time
// (group16, owner's decision of 2026-09-28); no character is put into the
// text for it.
#let d = json("/doc.json")
#set document(title: d.plain-name, keywords: d.template, date: none)
#set page(paper: "a4", margin: 2cm)
#set text(
  font: ("Noto Serif", "Noto Sans Cuneiform", "UllikummiA", "STIX Two Math", "Noto Serif Hebrew"),
  fallback: false,
  overhang: false,
  size: 10pt,
)
#let label(r) = box(
  stroke: 0.4pt + luma(35%),
  inset: (x: 0.12em),
  outset: (y: 0.18em),
  box(baseline: r.d * 1em, image(bytes(r.svg), format: "svg", height: r.h * 1em)),
)
#let series(s) = s.clusters().chunks(16).map(c => box(c.join())).join()
#let runs(rs) = rs.map(r => if r.k == "u" { label(r) } else if r.k == "s" { series(r.s) } else { r.s }).join()
#if d.credit != none {
  place(bottom, float: true, clearance: 1.2em, block(width: 100%, {
    line(length: 25%, stroke: 0.4pt)
    v(0.3em, weak: true)
    text(size: 8pt, d.credit)
  }))
}
#set footnote(numbering: n => [])
#set footnote.entry(separator: line(length: 25%, stroke: 0.4pt))
// Raised and lowered as HPMxml.css sets corr, materlect (80 %, super) and
// subscr (90 %, bottom): a box shifted off the baseline – its frame counts
// in the line's height, so a lowered letter on the last line stays inside
// the margin – and no super() or sub(), which tag a Span.
#let inline(r) = if r.k == "sup" { box(baseline: -0.35em, text(size: 0.8em, runs(r.r))) } else if r.k == "sub" { box(baseline: 0.2em, text(size: 0.9em, runs(r.r))) } else { runs((r,)) }
#let piece(r) = if r.k == "n" {
  footnote(r.p.map(p => p.map(inline).join()).join(linebreak()))
} else if r.k == "tab" { h(2em) } else { inline(r) }
#heading(level: 1, runs(d.head))
#heading(level: 2, runs(d.name))
#for s in d.sections {
  grid(columns: 1, s.map(l => par(hanging-indent: 1.5em, l.map(piece).join())).join())
}
