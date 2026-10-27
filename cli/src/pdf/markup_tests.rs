//! What a page shows of the markup (`XML-CONTRACT.md` §4, "What a page
//! shows"): one test for each rule, in the body and, where the corpus writes
//! the construct there, in a note and in `cu`. A page is written out as a
//! line of text per line, `^{…}` raised, `_{…}` lowered, `{note: … ¶ …}` a
//! note and its paragraphs, an empty line between sections.

use super::layout::{sections, Piece, Section};
use super::markup::{self, VOCABULARY};
use crate::document::Document;

const NS: &str = "xmlns:AO=\"http://hethiter.net/ns/AO/1.0\" \
                  xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\"";

/// The sections of a document whose header has `docID` `KBo 0.1` and whose
/// body is `body`.
fn shown(body: &str) -> String {
    shown_with("<AOHeader><docID>KBo 0.1</docID></AOHeader>", body)
}

fn shown_with(header: &str, body: &str) -> String {
    let xml = format!("<AOxml {NS}>{header}<body><text>{body}</text></body></AOxml>");
    let d = Document::read(xml.as_bytes()).unwrap_or_else(|e| panic!("{e}: {xml}"));
    written(&sections(&d))
}

fn written(sections: &[Section]) -> String {
    sections
        .iter()
        .map(|s| s.iter().map(|l| line(l)).collect::<Vec<_>>().join("\n"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn line(pieces: &[Piece]) -> String {
    pieces
        .iter()
        .map(|p| match p {
            Piece::Text(t) => t.clone(),
            Piece::Sup(t) => format!("^{{{t}}}"),
            Piece::Sub(t) => format!("_{{{t}}}"),
            Piece::Note(paragraphs) => format!(
                "{{note: {}}}",
                paragraphs
                    .iter()
                    .map(|p| line(p))
                    .collect::<Vec<_>>()
                    .join(" ¶ ")
            ),
            Piece::Tab => "→".into(),
        })
        .collect()
}

/// The text of an attribute: `<`, `>`, `&` and `"` as the source escapes them.
fn attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A note whose `c` is `markup`.
fn note(markup: &str) -> String {
    format!("<note c=\"{}\"/>", attr(markup))
}

/// The page of one line that is one note whose `c` is `markup`.
fn in_note(markup: &str) -> String {
    shown(&format!("<lb/>{}", note(markup)))
}

// ---------------------------------------------------------------------------
// Notes and cu: markup read, not printed
// ---------------------------------------------------------------------------

/// Every name of the vocabulary in one note: as an element with content,
/// empty, and nested in its neighbour; the first and the last tag at the
/// note's edges. Nothing of the markup is shown, every content is, in order.
#[test]
fn a_note_with_every_tag_of_the_corpus_shows_its_text_and_no_markup() {
    let names: Vec<&str> = VOCABULARY
        .iter()
        .copied()
        .filter(|n| !n.contains('"'))
        .collect();
    assert_eq!(names.len(), 89, "the vocabulary less its one broken name");
    let mut c = String::new();
    for (i, n) in names.iter().enumerate() {
        // Content, then the empty form; every other one inside its successor.
        if i % 2 == 0 {
            c.push_str(&format!("<{n}>t{i} </{n}><{n}/>"));
        } else {
            c.push_str(&format!("<{n}>t{i} <{n}/></{n}>"));
        }
    }
    let s = in_note(&c);
    let text = s.trim_start_matches("KBo 0.1\n");
    assert!(!text.contains(['<', '>', '\u{2329}', '\u{232A}']), "{text}");
    let mut at = 0;
    for (i, name) in names.iter().enumerate() {
        let token = format!("t{i}");
        let found = text[at..]
            .match_indices(&token)
            .map(|(k, _)| at + k + token.len())
            .find(|end| !text[*end..].starts_with(|c: char| c.is_ascii_digit()))
            .unwrap_or_else(|| panic!("t{i} of {name} missing or out of order: {text}"));
        at = found;
    }
    // The signs of the tags that are signs stand among them.
    for sign in ["[", "]", "\u{2E22}", "\u{2E23}", "(Rasur)"] {
        assert!(text.contains(sign), "{sign}: {text}");
    }
}

#[test]
fn the_vocabulary_is_the_corpus_s_and_the_owner_s() {
    assert_eq!(
        VOCABULARY.len(),
        84 + 4 + 4 - 2,
        "two names are in both notes and cu"
    );
    for added in ["Akk", "AO:SIGN", "Numeral", "AkkGRAM"] {
        assert!(VOCABULARY.contains(&added), "{added}");
    }
}

#[test]
fn a_note_of_paragraphs_breaks_its_lines_where_words_would_have_joined() {
    // The seven places of the corpus where `</P_f_…><P_f_…>` joined two
    // words (`KBo 2.9+`, `KBo 30.1+`), before and after.
    for (before, after) in [
        ("al periodo precedente.", "Si noti"),
        ("version A :", "B13'. na-aš-ta"),
        ("ha-at-ta-re-eš-na-aš pa-ra-(a pé-e-da)-i", "B15’. na-an"),
        ("MU(ŠEN.GAL KASKAL-ši)", "B14’. ha-at-ta"),
        ("š(i-pa-an)-ti", "§"),
        ("§", "D10’. … ú-iz-zi"),
        ("PA-NI …", "D11’. … UŠ-KE-EN-NU"),
    ] {
        let c =
            format!("<P_f_Footnote>{before}</P_f_Footnote><P_f_Footnote>{after}</P_f_Footnote>");
        assert_eq!(
            in_note(&c),
            format!("KBo 0.1\n{{note: {before} ¶ {after}}}"),
        );
    }
}

#[test]
fn two_notes_in_a_row_stay_two() {
    assert_eq!(
        shown(&format!(
            "<lb/>a{}{} b",
            note("<d>D</d>10"),
            note("x <sGr>GAL</sGr>")
        )),
        "KBo 0.1\na{note: D10}{note: x GAL} b"
    );
}

#[test]
fn a_line_of_cuneiform_shows_its_signs_and_no_markup() {
    // `<w/>` and `<c>` as the 84 documents of the corpus write them.
    assert_eq!(
        shown(&format!(
            "<lb cu=\"{}\"/>a",
            attr("𒀀<w/>𒀭 <c>𒈗</c><wsep/>▒")
        )),
        "KBo 0.1\n𒀀𒀭 𒈗▒\na"
    );
}

#[test]
fn entities_are_their_characters() {
    // `CTH 321/KBo 3.7`: `&amp;amp;` in the source, `&amp;` in the value.
    assert_eq!(
        shown("<lb/>a<note c=\"Pecchioli &amp;amp; Polvani &amp;lt;x&amp;#x2E22;\"/>"),
        "KBo 0.1\na{note: Pecchioli & Polvani <x\u{2E22}}"
    );
}

#[test]
fn a_tag_no_rule_knows_is_not_shown_and_what_it_holds_is() {
    assert_eq!(shown("<lb/>a <neu>b</neu> <neu/>c"), "KBo 0.1\na b c");
    assert_eq!(
        in_note("x <unbekannt a=\"1\">y</unbekannt> z"),
        "KBo 0.1\n{note: x y z}"
    );
}

#[test]
fn crossed_tags_lose_no_text_and_show_no_tag() {
    // `CTH 434/KBo 20.82`, as the source crosses them.
    let c = "〈SP_f_AO_3a_SumGRAM〉LUGAL〈/sGr〉 〈sGr〉GAL〈/SP_f_AO_3a_SumGRAM〉 \
             〈SP_f_AO_3a_Hittite〉A〈del_in/〉š-mu〈/SP_f_AO_3a_Hittite〉〈del_fin/〉";
    assert_eq!(in_note(c), "KBo 0.1\n{note: LUGAL GAL A[š-mu]}");
    let lenient = markup::lenient("a<b>c</d>e<f/>g<h");
    assert_eq!(
        lenient
            .iter()
            .filter_map(|e| match e {
                markup::Event::Text(t) => Some(t.as_ref()),
                _ => None,
            })
            .collect::<String>(),
        "aceg<h"
    );
}

#[test]
fn the_vocabulary_between_angle_brackets_is_markup_and_an_emendation_is_text() {
    assert_eq!(
        in_note(
            "〈P_f_Footnote〉il verbo 〈SP_f_AO_3a_Hittite〉išhai-〈/SP_f_AO_3a_Hittite〉 ku-〈e〉-da 〈〈x〉〉〈/P_f_Footnote〉"
        ),
        "KBo 0.1\n{note: il verbo išhai- ku-〈e〉-da 〈〈x〉〉}"
    );
    // Owner's decision K1: the four names added on 2026-10-27.
    assert_eq!(
        in_note("〈Numeral〉1〈/Numeral〉-〈AkkGRAM〉EN〈/AkkGRAM〉 〈Akk〉manzāzu〈/Akk〉 〈AO:SIGN〉MA〈/AO:SIGN〉"),
        "KBo 0.1\n{note: 1-EN manzāzu MA}"
    );
    // A name outside the vocabulary is not markup.
    assert_eq!(in_note("〈Unbekannt〉x"), "KBo 0.1\n{note: 〈Unbekannt〉x}");
}

#[test]
fn a_tag_shows_the_same_in_the_body_and_in_a_note() {
    let markup =
        "<del_in/>ZAG<corr c=\"?\"/>-ni<del_fin/> <laes_in/>wa<subscr c=\"a\"/><laes_fin/> \
                  <surpl c=\"aš\"/> <ras_in/>nu<ras_fin/> <add_in/>NINDA<add_fin/> <gap c=\"…\"/>";
    let body = shown(&format!("<lb/>{markup}"));
    let in_note = shown(&format!("<lb/>{}", note(markup)));
    let body_line = body.trim_start_matches("KBo 0.1\n");
    assert_eq!(in_note, format!("KBo 0.1\n{{note: {body_line}}}"));
}

// ---------------------------------------------------------------------------
// Signs and letters of the edition
// ---------------------------------------------------------------------------

#[test]
fn the_signs_of_the_edition_stand_in_the_body_and_in_a_note() {
    let s = "<del_in/>ZAG<corr c=\"?\"/>-ni<del_fin/> <laes_in/>an<laes_fin/>-da<corr c=\"!\"/> \
             a<del_fin/><del_in/>b <laes_in/><del_in/>x<del_fin/><laes_fin/>";
    let want = "[ZAG^{?}-ni] ⸢an⸣-da^{!} a][b ⸢[x]⸣";
    assert_eq!(shown(&format!("<lb/>{s}")), format!("KBo 0.1\n{want}"));
    assert_eq!(in_note(s), format!("KBo 0.1\n{{note: {want}}}"));
}

#[test]
fn letters_of_attributes_are_shown_as_the_stylesheet_places_them() {
    // subscr – lowered; materlect – raised; surpl – 〈〈…〉〉 (HPMxml.css).
    assert_eq!(
        shown("<lb/><w>-wa<subscr c=\"a\"/>-a kán<materlect c=\"an\"/>-na ú-<surpl c=\"te\"/>-tén</w>"),
        "KBo 0.1\n-wa_{a}-a kán^{an}-na ú-〈〈te〉〉-tén"
    );
    // Empty: nothing raised or lowered, the empty surplus still marked.
    assert_eq!(
        shown("<lb/>a<subscr c=\"\"/>b<materlect c=\"\"/>c<surpl c=\"\"/>"),
        "KBo 0.1\nabc〈〈〉〉"
    );
}

#[test]
fn a_gap_shows_its_text_in_the_line_or_as_a_line() {
    // C1.
    assert_eq!(
        shown(
            "<lb/>a <gap c=\"(Rasur)\"/> b<lb/><gap t=\"line\" c=\"Text bricht ab\"/><gap c=\"\"/>"
        ),
        "KBo 0.1\na (Rasur) b\nText bricht ab"
    );
}

#[test]
fn a_paragraph_number_is_a_line_of_its_own() {
    // C2.
    assert_eq!(
        shown("<lb/>a<parsep_dbl/><AO:ParagrNr c=\"Kolophon\"/><lb/>b"),
        "KBo 0.1\na\n\nKolophon\nb"
    );
}

#[test]
fn an_erasure_is_bracketed_and_an_empty_one_named() {
    // C4.
    assert_eq!(
        shown("<lb/>É<ras_in/>ḫi<corr c=\"!\"/>-lam<ras_fin/>-ni <ras_in/><ras_fin/> nu <ras_X/>x <ras_fin/>"),
        "KBo 0.1\nÉ⟦ḫi^{!}-lam⟧-ni (Rasur) nu (Rasur)x ⟧"
    );
    // One left open is still opened.
    assert_eq!(shown("<lb/>a<ras_in/>b"), "KBo 0.1\na⟦b");
}

#[test]
fn an_addition_is_in_angle_brackets() {
    // C5.
    assert_eq!(shown("<lb/><add_in/>NINDA<add_fin/>"), "KBo 0.1\n〈NINDA〉");
}

#[test]
fn a_hurrian_quotation_shows_its_words_and_no_border() {
    // C6.
    assert_eq!(
        shown("<lb/>a <QUOT_HurInHit_in/> <w>ku-un-zi-ip</w> <QUOT_HurInHit_fin/> b"),
        "KBo 0.1\na ku-un-zi-ip b"
    );
}

#[test]
fn a_word_separator_shows_its_sign() {
    // C7.
    assert_eq!(shown("<lb/>a <wsep c=\"𒀹\"/> b"), "KBo 0.1\na 𒀹 b");
}

#[test]
fn a_line_number_stands_at_the_head_of_its_line() {
    // lnr: lb:after { content: " " attr(lnr) " " } (HPMxml.css), after the
    // line of cuneiform.
    assert_eq!(
        shown("<lb lnr=\"Vs. 1′\" cu=\"𒀀\"/>a-na<lb lnr=\"\"/>b<lb/>c"),
        "KBo 0.1\n𒀀\nVs. 1′ a-na\nb\nc"
    );
}

// ---------------------------------------------------------------------------
// The heading's sigla
// ---------------------------------------------------------------------------

#[test]
fn a_publication_equal_to_the_document_is_not_repeated() {
    assert_eq!(
        shown_with(
            "<AOHeader><docID>KBo 28.24</docID> <AO:Manuscripts><AO:TxtPubl>KBo 28.24</AO:TxtPubl></AO:Manuscripts></AOHeader>",
            "<lb/>a"
        ),
        "KBo 28.24\na"
    );
}

#[test]
fn a_join_lists_its_fragments_with_their_signs() {
    // C3.
    assert_eq!(
        shown_with(
            "<AOHeader><docID>KBo 26.64+</docID> <AO:Manuscripts><AO:TxtPubl nr=\"€1\">KBo 26.64</AO:TxtPubl> \
             <AO:DirectJoin/> <AO:TxtPubl nr=\"€2\">KUB 33.87</AO:TxtPubl> <AO:InDirectJoin/> \
             <AO:TxtPubl>KUB 36.12</AO:TxtPubl></AO:Manuscripts></AOHeader>",
            "<lb/>a"
        ),
        "KBo 26.64+ KBo 26.64 + KUB 33.87 (+) KUB 36.12\na"
    );
}

#[test]
fn markup_in_a_value_the_page_shows_is_read_as_markup() {
    // `KBo 1.1+`: `corr c="&lt;ras_in/&gt;"`; `gap c` and `materlect c` with
    // the signs of the edition; a value cut short by a tag, read leniently.
    assert_eq!(
        shown(
            "<lb/>m<corr c=\"&lt;ras_in/&gt;\"/>Ni<corr c=\"&lt;ras_fin/&gt;\"/>-ru \
             <gap c=\"&lt;del_in/&gt;x x&lt;del_fin/&gt;\"/> kán<materlect c=\"&lt;laes_in/&gt;ÍD!&lt;laes_fin/&gt;\"/> \
             a<corr c=\"(over&lt;/w&gt; &lt;w&gt;erasure)\"/>"
        ),
        "KBo 0.1\nm^{\u{27E6}}Ni^{\u{27E7}}-ru [x x] kán^{\u{2E22}ÍD!\u{2E23}} a^{(over erasure)}"
    );
}

#[test]
fn a_line_number_and_a_broken_tag_lose_no_sign() {
    // `KUB 30.24+`: markup in `lnr`; `KP24_467C_PL2_SF65`: `!!` before the
    // attribute's name in a tag of `cu`.
    assert_eq!(
        shown(
            "<lb lnr=\"{€4} Vs.&lt;/w&gt; &lt;w&gt;I&lt;/w&gt; &lt;w&gt;1′\" \
             cu=\"𒌍 !!&lt;wsep !!c=&quot; 𒑱 !!&quot;/&gt; 𒂊\"/>a"
        ),
        "KBo 0.1\n𒌍 !! 𒑱 !! 𒂊\n{€4} Vs. I 1′ a"
    );
}

/// The events of the lenient reader, written out: `<name a="v">`, `</name>`,
/// text as it is.
fn lenient(s: &str) -> String {
    markup::lenient(s)
        .iter()
        .map(|e| match e {
            markup::Event::Start(t) => {
                let attrs: String = t
                    .attributes
                    .iter()
                    .map(|(k, v)| format!(" {k}=\"{v}\""))
                    .collect();
                let prefix = t
                    .prefix
                    .as_deref()
                    .map(|p| format!("{p}:"))
                    .unwrap_or_default();
                format!("<{prefix}{}{attrs}>", t.local)
            }
            markup::Event::End(n) => format!("</{n}>"),
            markup::Event::Text(t) => t.to_string(),
        })
        .collect()
}

#[test]
fn the_lenient_reader_reads_tags_attributes_and_entities_and_keeps_the_rest() {
    // Entities: the five, decimal and hexadecimal references; an unknown one
    // and a bare `&` stay text.
    assert_eq!(
        lenient("a&amp;b &lt;&gt;&quot;&apos; &#x41;&#X42;&#67; &bogus; & x"),
        "a&b <>\"' ABC &bogus; & x"
    );
    // A prefixed tag, its end, a closed one; values with their entities.
    assert_eq!(
        lenient("<AO:LIT n=\"1\" c=\"&quot;q&quot;\">Haas</AO:LIT><d/>"),
        "<AO:LIT n=\"1\" c=\"\"q\"\">Haas</LIT><d></d>"
    );
    // Not a tag: `<` before a space or a digit, a tag cut by another `<`, one
    // never closed.
    assert_eq!(lenient("1 < 2 <3 <a <b>x"), "1 < 2 <3 <a <b>x");
    // An attribute without quotes or with an open quote ends the attributes.
    assert_eq!(lenient("<t c=x d=\"y\"/>z"), "<t></t>z");
    assert_eq!(lenient("<t c=\"x/>z"), "<t></t>z");
    // A value after a name with stray characters before it.
    assert_eq!(lenient("<wsep !!c=\" 𒑱 \"/>"), "<wsep c=\" 𒑱 \"></wsep>");
}

#[test]
fn a_note_with_nothing_to_show_is_no_note() {
    assert_eq!(shown("<lb/>a<note c=\"  \"/> b"), "KBo 0.1\na b");
    assert_eq!(in_note("<P_f_Footnote> </P_f_Footnote>"), "KBo 0.1");
}

#[test]
fn a_sign_written_into_a_tag_s_name_is_kept() {
    // `CTH 470/CHDS 7.200`: `<wsepc="𒀹"/>` in `cu`, read by the parser of
    // the documents; the same cut by a `<` is read leniently.
    assert_eq!(
        shown("<lb cu=\"&lt;wsepc=&quot;𒀹&quot;/&gt;𒉿𒅈\"/>a"),
        "KBo 0.1\n𒀹𒉿𒅈\na"
    );
    assert_eq!(in_note("x<wsepc=\"𒀹\"/>y<z"), "KBo 0.1\n{note: x𒀹y<z}");
}

#[test]
fn names_no_rule_knows_outside_the_body_are_counted_and_the_body_s_are_not() {
    let xml = format!(
        "<AOxml {NS}><body><text><lb cu=\"&lt;frei/&gt;𒀀\"/><neu>a</neu>{}{}</text></body></AOxml>",
        note("<neu>x</neu><d>y</d><neu/><AO:LIT>z</AO:LIT><wsepc=\"𒀹\"/>"),
        "<corr c=\"&lt;anders/&gt;?\"/>"
    );
    let d = Document::read(xml.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
    let unknown = super::layout::unknown_names(&d);
    assert_eq!(
        unknown.into_iter().collect::<Vec<_>>(),
        [
            ("anders".to_string(), 1),
            ("frei".to_string(), 1),
            ("neu".to_string(), 2)
        ]
    );
}

#[test]
fn a_parsep_inside_a_value_is_a_new_section_as_in_the_body() {
    // `CTH 456/KUB 39.52+`: `gap c="</w> <parsep/> <w>"` (owner's decision
    // of 2026-10-27).
    let in_body = shown("<lb/>a <parsep/> b");
    let in_value = shown("<lb/>a <gap c=\"&lt;/w&gt; &lt;parsep/&gt; &lt;w&gt;\"/> b");
    assert_eq!(in_body, "KBo 0.1\na\n\nb");
    assert_eq!(in_value, in_body);
    let xml = format!(
        "<AOxml {NS}><body><text><lb/>a <gap c=\"&lt;/w&gt; &lt;parsep/&gt; &lt;w&gt;\"/> b</text></body></AOxml>"
    );
    let d = Document::read(xml.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
    assert!(super::layout::unknown_names(&d).is_empty());
}
