//! Whether a document is well-formed XML, and if not, why.
//!
//! **This decides nothing about the package.** Every document the archive holds
//! is copied into the package, well-formed or not: the package is a byte mirror
//! of the corpus, copying needs no parser, and dropping 206 documents from a
//! mirror would lose data to buy nothing. What this module produces is a
//! *statement about* each document, written into the manifest beside it. The
//! parser is strict on the conversion path — the stage that turns a document
//! into a PDF cannot proceed on markup it cannot read — and that is where a
//! refusal belongs, not here.
//!
//! The reasons below are the ones three measurements found, not a taxonomy
//! invented for the occasion.
//!
//! **How many documents fall to each of them is not written here.** It was,
//! until 2026-09-06: a table of eight counts, headed "as of 2026-09-06", left
//! over from the measurement of the evening before — 33 / 88 / 28 and so on,
//! against the 21 / 95 / 33 this classifier now produces. The dispatch had
//! changed underneath it (an unterminated start tag is asked about before
//! nesting), the sum stayed 206, and nothing failed, because no test holds a
//! table in a comment. A reader looking here to learn how the corpus is shaped
//! would have learnt it wrong.
//!
//! So the counts live in one place, `docs/PROJECT-SPEC.ru.md` §4.13, beside the
//! date they were measured on; what belongs here is which reasons exist and why
//! they are told apart. `the_breakdown_adds_up_to_the_total` holds the sum
//! against the manifest, which is a property rather than a number.
//!
//! Five of the reasons — everything from `ElementNeverClosed` down to
//! `NoSuchElement` — are one refusal as far as `quick-xml` is concerned: an end
//! tag that closes the wrong element. They are separated here because a
//! measurement on 2026-09-06 showed the single class is five different defects
//! wanting five different repairs, and a manifest that called them one thing
//! would be hiding the finding that made the project stop trying to repair them.
//!
//! **A document that fits none of them is [`Reason::Unclassified`], and that is
//! deliberate.** Widening a reason until everything falls into one is how a
//! classification stops carrying information.
//!
//! **The parser has blind spots, and they are recorded rather than worked
//! around.** `quick-xml` accepts a raw `<` inside an attribute value and an
//! empty local name (`<AO:>`), both of which XML forbids; four documents of the
//! corpus are called well-formed here that `xmllint` refuses. Closing them
//! would take a second parser, and the one measured for the purpose was
//! rejected on 2026-09-06 for damaging the transliteration in silence. Four
//! documents named wrongly in a manifest is the smaller cost, and naming the
//! limits here is what keeps them known rather than surprising.
//!
//! What is *not* left to the parser: an input that ends with elements still
//! open, and a double hyphen inside a comment. `quick-xml` is silent on the
//! first and, until asked, on the second; both are checked here.

use quick_xml::errors::{Error, IllFormedError, SyntaxError};
use quick_xml::events::attributes::AttrError;
use quick_xml::events::Event;
use quick_xml::Reader;

/// Why a document is not well-formed XML.
///
/// Ordered as the classifier tries them, which is not arbitrary: an
/// unterminated start tag makes every later judgement about nesting unreliable,
/// so it is asked about first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reason {
    /// A tag was begun and no `>` closes it before the next `<`.
    ///
    /// The largest defect in the corpus and the one that cannot be repaired:
    /// `<w trans="x" <w …` leaves the following bytes belonging to neither tag,
    /// and three different repairs produce three different words. Measured
    /// 2026-09-05 over all 206 refusals.
    UnterminatedStartTag,
    /// An attribute name is not followed by `=`.
    ///
    /// `<kap c-"-ul">` — a hyphen where the equals sign belongs. Distinct from
    /// [`Self::UnterminatedStartTag`] even though both were one `quick-xml`
    /// class on 2026-09-05: this one has a `>` where it should be.
    AttributeNotSeparated,
    /// An attribute value opens a quote that never closes.
    ///
    /// The rest of the document becomes the value, so there is no meaningful
    /// position past this point.
    AttributeValueUnclosed,
    /// The same attribute appears twice on one element.
    AttributeGivenTwice,
    /// An element is opened and never closed anywhere in the document.
    ElementNeverClosed,
    /// An end tag arrives for an element that is open, but not innermost, and
    /// the innermost one is closed later.
    ///
    /// Overlapping markup: `<w><g></w><w></g></w>`. XML cannot express it at
    /// all, so there is no repair that keeps both elements — which is why this
    /// is its own reason rather than a variety of the one above.
    CrossingElements,
    /// An end tag for an element that was already opened and closed earlier.
    DuplicateEndTag,
    /// An end tag naming an element the document never opens.
    NoSuchElement,
    /// An end tag whose name differs from the open element only in its prefix.
    ///
    /// `<AO:TxtPubl>…</TxtPubl>`. Kept apart from [`Self::NoSuchElement`]
    /// because it is the one defect in the corpus with an unambiguous repair —
    /// one document, and naming it is how anyone who wants to fix that one
    /// finds it.
    MismatchedEndTagName,
    /// Not well-formed, and none of the above fits.
    ///
    /// Deliberately not a catch-all that gets widened. A document here is an
    /// invitation to measure, not to stretch a neighbouring reason over it.
    Unclassified,
}

impl Reason {
    /// The name the manifest and the window use.
    ///
    /// Stable: it is a key in a published file. Chosen to say what the document
    /// is, not what anyone did to it — nothing here was broken by this program,
    /// and nothing is refused a place in the package for it.
    pub fn key(self) -> &'static str {
        match self {
            Reason::UnterminatedStartTag => "unterminated-start-tag",
            Reason::AttributeNotSeparated => "attribute-not-separated",
            Reason::AttributeValueUnclosed => "attribute-value-unclosed",
            Reason::AttributeGivenTwice => "attribute-given-twice",
            Reason::ElementNeverClosed => "element-never-closed",
            Reason::CrossingElements => "crossing-elements",
            Reason::DuplicateEndTag => "duplicate-end-tag",
            Reason::NoSuchElement => "no-such-element",
            Reason::MismatchedEndTagName => "mismatched-end-tag-name",
            Reason::Unclassified => "unclassified",
        }
    }

    /// Every reason, for the manifest to list the ones with a count of zero too.
    ///
    /// A breakdown that omits the empty reasons cannot be told from one where
    /// the classifier never tried them.
    pub const ALL: [Reason; 10] = [
        Reason::UnterminatedStartTag,
        Reason::AttributeNotSeparated,
        Reason::AttributeValueUnclosed,
        Reason::AttributeGivenTwice,
        Reason::ElementNeverClosed,
        Reason::CrossingElements,
        Reason::DuplicateEndTag,
        Reason::NoSuchElement,
        Reason::MismatchedEndTagName,
        Reason::Unclassified,
    ];
}

/// What is wrong with one document, and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Finding {
    pub reason: Reason,
    /// Line of the first error, counted from 1.
    ///
    /// In the document **as the package ships it**, not as the archive holds
    /// it: normalisation writes a declaration on its own line, so the two
    /// disagree. Someone reading this opens the package copy.
    pub line: usize,
    /// Column of the first error, counted from 1 in characters.
    ///
    /// Characters rather than bytes: the corpus is dense with cuneiform, and a
    /// byte column would be three or four times the number an editor shows.
    pub column: usize,
}

/// Classify one document. `None` means it is well-formed XML.
///
/// Strict: no recovery, nothing repaired, nothing expanded. Entities, DTDs and
/// XInclude stay unexpanded because no call that expands them is made — not
/// because a flag says so.
pub fn classify(bytes: &[u8]) -> Option<Finding> {
    let (kind, at) = first_refusal(bytes)?;
    // A tag begun and never finished is asked about before anything else, and
    // for every kind of refusal alike. `quick-xml` does not refuse it — a `<`
    // inside a tag becomes part of an attribute value — so it surfaces later
    // and elsewhere, under whatever name the wreckage happens to produce. Three
    // different `quick-xml` classes carried it on 2026-09-05, and it is one
    // defect.
    let reason = match kind {
        Refusal::UnclosedValue => Reason::AttributeValueUnclosed,
        _ if unterminated_start_tag(bytes, at).is_some() => Reason::UnterminatedStartTag,
        Refusal::NotSeparated => Reason::AttributeNotSeparated,
        Refusal::NeverClosed => Reason::ElementNeverClosed,
        Refusal::Unterminated => Reason::UnterminatedStartTag,
        Refusal::Duplicated => Reason::AttributeGivenTwice,
        Refusal::Mismatched { end, open } => mismatch_reason(bytes, at, &end, &open),
        Refusal::Other => Reason::Unclassified,
    };
    let (line, column) = position(bytes, at);
    Some(Finding {
        reason,
        line,
        column,
    })
}

/// Which of the five defects a mismatched end tag is.
///
/// The order is the one a Python scanner used on 2026-09-06 to produce the
/// counts in this module's table; keeping it identical is what lets the two be
/// compared. That comparison is the classifier's acceptance: the scanner was
/// checked to find no mismatch at all in the 23 730 documents `quick-xml`
/// accepts.
fn mismatch_reason(bytes: &[u8], at: usize, end: &str, open: &[String]) -> Reason {
    // Nothing open at all: the end tag stands outside every element, and only
    // the document's own history says whether it is a second copy or a name
    // that never appeared.
    let Some(innermost) = open.last() else {
        return if find_start_tag(bytes, end, at) {
            Reason::DuplicateEndTag
        } else {
            Reason::NoSuchElement
        };
    };
    // An end tag for an element that is open further out: everything between is
    // unclosed. Whether that is a crossing or a plain omission depends on
    // something no parser state can answer — whether the inner element's own
    // end tag turns up later.
    if open.iter().rev().skip(1).any(|name| name == end) {
        return if find_end_tag(bytes, innermost, at).is_some() {
            Reason::CrossingElements
        } else {
            Reason::ElementNeverClosed
        };
    }
    // Nothing open by that name. Either it was opened and closed already, or
    // the document never had it — and if the innermost open element differs
    // only by a prefix, that is the one defect worth naming on its own.
    if same_local_name(end, innermost) {
        return Reason::MismatchedEndTagName;
    }
    if find_start_tag(bytes, end, at) {
        Reason::DuplicateEndTag
    } else {
        Reason::NoSuchElement
    }
}

/// Whether two qualified names differ only in their prefix.
fn same_local_name(a: &str, b: &str) -> bool {
    let local = |name: &str| name.rsplit(':').next().unwrap_or(name).to_string();
    a != b && local(a) == local(b)
}

/// Where a start tag is begun and not finished before the next `<`, left of `at`.
///
/// Scanned over the bytes rather than asked of the parser, because `quick-xml`
/// does not refuse it: a `<` inside a tag is taken as part of an attribute
/// value, and the refusal surfaces later and elsewhere. Comments, CDATA and
/// processing instructions are stepped over; quoted attribute values are walked
/// through, since `>` needs no escaping inside one and this corpus uses it.
fn unterminated_start_tag(bytes: &[u8], at: usize) -> Option<usize> {
    let mut i = 0usize;
    while i < bytes.len() {
        let start = i + memchr::memchr(b'<', &bytes[i..])?;
        if start >= at {
            return None;
        }
        let rest = &bytes[start..];
        if rest.starts_with(b"<!--") {
            i = memchr_after(bytes, start + 4, b"-->");
            continue;
        }
        if rest.starts_with(b"<![CDATA[") {
            i = memchr_after(bytes, start + 9, b"]]>");
            continue;
        }
        let end = end_of_tag(bytes, start);
        if !rest.starts_with(b"<!") && !rest.starts_with(b"<?") {
            if let Some(next) = unquoted_lt(bytes, start) {
                if next < end.saturating_sub(1) {
                    return Some(start);
                }
            }
        }
        i = end.max(start + 1);
    }
    None
}

/// The next `<` after `start` that is not inside an attribute value.
///
/// The distinction is the whole of it. `<w trans="x" <w>` has a second `<`
/// where a tag cannot have one — that tag was never finished. `<kap c="a<b">`
/// has one too, and there it is ordinary text inside a value that XML happens
/// to forbid unescaped. Counting both made 119 documents "unterminated" on
/// 2026-09-06 when 88 are, and buried the second defect of the corpus under the
/// first.
fn unquoted_lt(bytes: &[u8], start: usize) -> Option<usize> {
    let mut quote: Option<u8> = None;
    let mut i = start + 1;
    while i < bytes.len() {
        let byte = bytes[i];
        match quote {
            Some(open) if byte == open => quote = None,
            Some(_) => {}
            None if byte == b'"' || byte == b'\'' => quote = Some(byte),
            None if byte == b'<' => return Some(i),
            None if byte == b'>' => return None,
            None => {}
        }
        i += 1;
    }
    None
}

/// One past the `>` that closes the tag beginning at `start`.
fn end_of_tag(bytes: &[u8], start: usize) -> usize {
    let mut quote: Option<u8> = None;
    let mut i = start + 1;
    while i < bytes.len() {
        let byte = bytes[i];
        match quote {
            Some(open) if byte == open => quote = None,
            Some(_) => {}
            None if byte == b'"' || byte == b'\'' => quote = Some(byte),
            None if byte == b'>' => return i + 1,
            None => {}
        }
        i += 1;
    }
    bytes.len()
}

/// One past `needle` searched from `from`, or the end of input.
fn memchr_after(bytes: &[u8], from: usize, needle: &[u8]) -> usize {
    if from >= bytes.len() {
        return bytes.len();
    }
    match crate::xml_scan::find_exact(&bytes[from..], needle) {
        Some(offset) => from + offset + needle.len(),
        None => bytes.len(),
    }
}

/// Whether `</name>` occurs at or after `from`.
fn find_end_tag(bytes: &[u8], name: &str, from: usize) -> Option<usize> {
    let needle = format!("</{name}>");
    crate::xml_scan::find_exact(bytes.get(from..)?, needle.as_bytes())
}

/// Whether `<name` is opened anywhere before `before`.
fn find_start_tag(bytes: &[u8], name: &str, before: usize) -> bool {
    let needle = format!("<{name}");
    let window = &bytes[..before.min(bytes.len())];
    crate::xml_scan::find_exact(window, needle.as_bytes()).is_some()
}

/// What the parser refused on, reduced to the distinctions this module makes.
enum Refusal {
    NotSeparated,
    NeverClosed,
    Unterminated,
    UnclosedValue,
    Duplicated,
    Mismatched { end: String, open: Vec<String> },
    Other,
}

/// The first thing `quick-xml` refuses, and where.
///
/// Two kinds of refusal, and the difference decides what may be counted. A
/// reader-level error ends the document: the parser's state is no longer
/// trustworthy and anything after it would be invented. An attribute error does
/// not — `Attributes` is a lazy iterator over a tag the reader has already read
/// whole. Either way only the first is returned: everything after the first is
/// the parser walking through wreckage.
fn first_refusal(bytes: &[u8]) -> Option<(Refusal, usize)> {
    let mut reader = Reader::from_reader(bytes);
    // Строже, чем по умолчанию: двойной дефис внутри комментария XML запрещает,
    // а `quick-xml` его пропускает, пока не попросят. Разбор здесь обязан быть
    // строгим – это записано решением 4.13, – и настройка, которую можно
    // забыть, ставится один раз рядом с разборщиком.
    reader.config_mut().check_comments = true;
    let mut buf = Vec::new();
    let mut open: Vec<String> = Vec::new();

    loop {
        let start = reader.buffer_position() as usize;
        buf.clear();
        match reader.read_event_into(&mut buf) {
            // Вход кончился, а элементы остались открыты. `quick-xml` об этом
            // молчит, и молчал бы и здесь: стек знает только вызывающий, и
            // спросить его – его же обязанность. Без этой ветки документ,
            // обрезанный посередине, назывался бы корректным.
            Ok(Event::Eof) if !open.is_empty() => return Some((Refusal::NeverClosed, bytes.len())),
            Ok(Event::Eof) => return None,
            Ok(event) => {
                if let Event::Start(ref tag) | Event::Empty(ref tag) = event {
                    for attribute in tag.attributes() {
                        if let Err(err) = attribute {
                            let (refusal, offset) = match err {
                                AttrError::ExpectedEq(at) => (Refusal::NotSeparated, at),
                                AttrError::ExpectedValue(at) => (Refusal::NotSeparated, at),
                                AttrError::ExpectedQuote(at, _) => (Refusal::NotSeparated, at),
                                AttrError::UnquotedValue(at) => (Refusal::NotSeparated, at),
                                AttrError::Duplicated(at, _) => (Refusal::Duplicated, at),
                            };
                            // `AttrError` counts from the first byte after `<`.
                            return Some((refusal, start + 1 + offset));
                        }
                    }
                }
                match event {
                    Event::Start(tag) => open.push(tag.name().into_inner().to_owned()),
                    Event::End(_) => {
                        open.pop();
                    }
                    _ => {}
                }
            }
            Err(err) => {
                let at = reader.error_position() as usize;
                let refusal = match err {
                    Error::IllFormed(IllFormedError::MismatchedEndTag { expected, found }) => {
                        // `open` still holds what was open when the end tag
                        // arrived: the reader refuses before popping.
                        let _ = expected;
                        Refusal::Mismatched {
                            end: found,
                            open: open.clone(),
                        }
                    }
                    // Both quote styles, and only these: an unclosed value
                    // swallows the rest of the file, which is a different
                    // defect from a tag that simply runs out of input.
                    Error::Syntax(SyntaxError::UnclosedDoubleQuotedAttributeValue)
                    | Error::Syntax(SyntaxError::UnclosedSingleQuotedAttributeValue) => {
                        Refusal::UnclosedValue
                    }
                    // At end of input with elements still open, which is what
                    // "never closed" means when nothing later contradicts it.
                    Error::IllFormed(IllFormedError::MissingEndTag(_)) => Refusal::NeverClosed,
                    // Тег начат и кончился вход. Тот же дефект, что ловит
                    // `unterminated_start_tag`, только сканеру его не найти:
                    // там признак – следующий `<`, а здесь за тегом нет
                    // ничего.
                    Error::Syntax(SyntaxError::UnclosedTag) => Refusal::Unterminated,
                    Error::IllFormed(IllFormedError::UnmatchedEndTag(found)) => {
                        Refusal::Mismatched {
                            end: found,
                            open: open.clone(),
                        }
                    }
                    _ => Refusal::Other,
                };
                return Some((refusal, at));
            }
        }
    }
}

/// Line and column of a byte offset, counted in one pass.
fn position(bytes: &[u8], at: usize) -> (usize, usize) {
    let at = at.min(bytes.len());
    let line = 1 + memchr::memchr_iter(b'\n', &bytes[..at]).count();
    let start = memchr::memrchr(b'\n', &bytes[..at]).map_or(0, |i| i + 1);
    let column = 1 + String::from_utf8_lossy(&bytes[start..at]).chars().count();
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document with nothing wrong is not a finding.
    ///
    /// The first thing to hold: a classifier that reports on well-formed
    /// documents would put 23 730 entries in the manifest and mean nothing.
    #[test]
    fn a_well_formed_document_has_no_finding() {
        let good = br#"<AOxml><AOHeader><docID>KBo 1.1</docID></AOHeader><body><text><w>nu</w></text></body></AOxml>"#;
        assert_eq!(classify(good), None);
    }

    fn reason(xml: &str) -> Reason {
        classify(xml.as_bytes())
            .unwrap_or_else(|| panic!("expected a finding in {xml}"))
            .reason
    }

    /// Every reason has a document that produces it, and only it.
    ///
    /// Written as one table rather than ten tests because the property being
    /// held is that the reasons are *distinct*: a change that makes two of them
    /// collide fails here, where ten separate tests would still pass nine.
    #[test]
    fn each_reason_has_a_document_that_shows_it() {
        let cases: [(Reason, &str); 9] = [
            (
                Reason::UnterminatedStartTag,
                r#"<a><w trans="x" <w>nu</w></a>"#,
            ),
            (Reason::AttributeNotSeparated, r#"<a><w c-"x">nu</w></a>"#),
            (
                Reason::AttributeValueUnclosed,
                r#"<a><w trans="x>nu</w></a>"#,
            ),
            (
                Reason::AttributeGivenTwice,
                r#"<a><w c="1" c="2">nu</w></a>"#,
            ),
            (Reason::ElementNeverClosed, r#"<a><t><w>nu</t></a>"#),
            (
                Reason::CrossingElements,
                r#"<a><w><g>nu</w><w>ta</g></w></a>"#,
            ),
            (
                Reason::DuplicateEndTag,
                r#"<a><t><w>nu</w><g/></w></t></a>"#,
            ),
            (Reason::NoSuchElement, r#"<a><t>nu</q></t></a>"#),
            (
                Reason::MismatchedEndTagName,
                r#"<a><AO:TxtPubl>KBo 1.1</TxtPubl></a>"#,
            ),
        ];
        for (want, xml) in cases {
            assert_eq!(reason(xml), want, "for {xml}");
        }
    }

    /// A defect nobody measured is named as such rather than filed under a
    /// neighbour.
    ///
    /// The whole value of the breakdown is that a count under a reason means
    /// that reason. A classifier that never says "unclassified" has stopped
    /// distinguishing and started reassuring.
    #[test]
    fn a_defect_that_fits_no_reason_is_left_unclassified() {
        // A CDATA section that never closes is not well-formed, and it is none
        // of the defects the corpus was measured to have.
        let finding = classify(b"<a><![CDATA[x</a>").expect("unclosed CDATA is refused");
        assert_eq!(finding.reason, Reason::Unclassified);
    }

    /// The position is the one a reader of the package will see.
    #[test]
    fn the_position_names_line_and_column_from_one() {
        let xml = "<a>\n  <t><w>nu</t>\n</a>";
        let finding = classify(xml.as_bytes()).expect("a finding");
        assert_eq!(finding.reason, Reason::ElementNeverClosed);
        assert_eq!(finding.line, 2, "the second line carries the defect");
        assert!(finding.column > 1, "and it is not at the margin");
    }

    /// The parser's blind spot is a known limit, held by a test so that it
    /// cannot quietly change.
    ///
    /// XML forbids a raw `<` in an attribute value; `quick-xml` allows it. Four
    /// documents of the corpus are called well-formed here that `xmllint`
    /// refuses. The test asserts the limit rather than the fix: the fix is a
    /// second parser, and that was measured and rejected on 2026-09-06.
    #[test]
    fn a_raw_less_than_inside_an_attribute_value_is_not_caught() {
        let xml = br#"<a><w trans="a<b">x</w></a>"#;
        assert_eq!(
            classify(xml),
            None,
            "recorded limit, not an accident: see this module's header"
        );
    }

    /// Every reason has a distinct key, and the keys are what the manifest
    /// publishes.
    /// **Каждый образец из `cli/fixtures/xml/` получает тот ответ, что ему
    /// положен, и ответ этот записан здесь именем причины.**
    ///
    /// Образцы лежат в дереве с 20.08 и до сих пор никем не разбирались – их
    /// заводили под будущий разборщик. Тест привязывает классификатор к ним, а
    /// не к литералам, написанным рядом с ним же: литерал подгоняют под код,
    /// файл в фикстурах – нет.
    ///
    /// `not-utf8.xml` и `empty.xml` не названы: первый не текст, второй пуст, и
    /// оба отвечают на вопрос «корректный ли это XML» раньше разбора.
    #[test]
    fn the_fixtures_get_the_answers_they_were_written_for() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/xml");
        let cases: [(&str, Option<Reason>); 6] = [
            (
                "malformed/tag-mismatch.xml",
                Some(Reason::ElementNeverClosed),
            ),
            (
                "malformed/bad-attribute-name.xml",
                Some(Reason::AttributeNotSeparated),
            ),
            (
                "malformed/truncated-in-tag.xml",
                Some(Reason::UnterminatedStartTag),
            ),
            ("malformed/truncated.xml", Some(Reason::ElementNeverClosed)),
            // Две слепые зоны разборщика, обе на файлах дерева. XML запрещает
            // и голый `<` в значении атрибута, и пустое локальное имя; этот
            // разборщик принимает оба. Ожидание записано таким, каково оно
            // есть: тест обязан ломаться, если зона закроется, – это событие,
            // а не молчаливое улучшение.
            ("malformed/unescaped-lt-in-attribute.xml", None),
            ("malformed/empty-qname.xml", None),
        ];
        for (name, want) in cases {
            let bytes = std::fs::read(dir.join(name)).expect("образец на месте");
            assert_eq!(classify(&bytes).map(|f| f.reason), want, "образец {name}");
        }

        // Объявленная latin-1 – единственный образец каталога `valid`, который
        // этот разборщик не берет, и берет он его не потому, что документ
        // неправильный. Документ корректен, он просто не в UTF-8, а крейт
        // собран без поддержки перекодировки: фича отключена нарочно, корпус
        // весь в UTF-8, и тянуть таблицы кодировок ради ноля документов
        // незачем. Названо здесь, потому что `unclassified` в этом одном
        // случае значит «не та кодировка», а не «неправильная разметка».
        let latin1 = std::fs::read(dir.join("valid/declared-latin1.xml")).expect("образец");
        assert_eq!(
            classify(&latin1).map(|f| f.reason),
            Some(Reason::Unclassified),
            "документ корректен, но не в UTF-8"
        );

        // И ни одного слова обо всех остальных корректных: разборщик,
        // находящий беду там, где ее нет, хуже молчащего.
        for entry in std::fs::read_dir(dir.join("valid")).expect("каталог образцов")
        {
            let path = entry.expect("запись").path();
            if path.extension().is_none_or(|e| e != "xml")
                || path.file_name().is_some_and(|n| n == "declared-latin1.xml")
            {
                continue;
            }
            let bytes = std::fs::read(&path).expect("образец читается");
            assert_eq!(
                classify(&bytes),
                None,
                "корректный образец {} назван некорректным",
                path.display()
            );
        }
    }

    /// Каждая причина умеет назвать себя, и имя у нее не пустое.
    #[test]
    fn every_reason_names_itself() {
        for reason in Reason::ALL {
            assert!(!reason.key().is_empty(), "{reason:?}");
            assert!(
                reason
                    .key()
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-'),
                "ключ {reason:?} попадет в JSON и в разметку окна"
            );
        }
    }

    #[test]
    fn the_keys_are_distinct_and_all_reasons_are_listed() {
        let mut keys: Vec<&str> = Reason::ALL.iter().map(|r| r.key()).collect();
        let count = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), count, "two reasons share a key");
        assert_eq!(count, 10);
    }
}
