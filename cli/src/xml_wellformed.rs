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
//! **The parser has blind spots, they are recorded rather than worked around,
//! and since 2026-09-10 they are measured rather than remembered.**
//! `quick-xml` accepts a raw `<` inside an attribute value, and a colon with no
//! local name after it (`<AO:-LineNrExpl>`, `<AO:--italic>` — both occur); XML
//! forbids the first outright and
//! *Namespaces in XML* forbids the second. Closing them would take a second
//! parser, and the one measured for the purpose was rejected on 2026-09-06 for
//! damaging the transliteration in silence — so the limits stay, and
//! [`beyond_the_parser`] walks the bytes for both and names the documents.
//!
//! The prose that stood here until 2026-09-10 said "four documents". Four is
//! what `xmllint` refuses outright; it also reports thirteen more, as a
//! *namespace* error, and exits zero on them — which is why they were never
//! counted. Seventeen documents of this corpus are accepted here and objected
//! to by `libxml2`, and the manifest now lists all seventeen by name.
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

/// A defect XML forbids and this parser accepts.
///
/// Not a second classifier. [`classify`] answers "does `quick-xml` refuse
/// this", and everything it refuses is already named by a [`Reason`]. This
/// answers the other question, the one the module header has carried as prose
/// since 2026-09-06: *of the documents it accepts, which ones would a
/// conforming parser still object to, and where*. Two classes, both measured on
/// the corpus, both scanned over the bytes rather than asked of a parser —
/// asking is what the crate cannot do, since these are exactly the two things
/// its parser does not see.
///
/// **Why measure at all rather than write the number down.** The count used to
/// be four, in a sentence, in three files. `xmllint` reports seventeen
/// documents, and the missing thirteen were invisible because nothing in this
/// crate looked for them. A number that no code produces cannot notice a new
/// edition of the corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Limit {
    /// A raw `<` inside an attribute value: `<w trans="a<b">`.
    ///
    /// Forbidden by XML outright — `<` in a value must be `&lt;` — and the
    /// reason is that a parser cannot otherwise tell a value from the start of
    /// the next tag. `quick-xml` takes it as an ordinary character of the
    /// value; `libxml2` says "Unescaped '<' not allowed in attributes values"
    /// and stops. Four documents of this corpus, measured 2026-09-10.
    RawLessThanInAttributeValue,
    /// A colon in an element name with no local name after it: `<AO:-LineNrExpl>`.
    ///
    /// XML 1.0 on its own permits this — `:` and `-` are both name characters,
    /// so `AO:-italic` is a legal `Name`. *Namespaces in XML* does not: a name
    /// with a colon must be `prefix:local`, and `-italic` cannot begin a local
    /// name. `libxml2` reports it as a namespace error rather than a parser
    /// error, and — this is the part worth knowing — still exits zero, so a
    /// build that only checked the exit status would never have seen these.
    /// Thirteen documents, measured 2026-09-10.
    ColonWithoutLocalName,
}

impl Limit {
    /// The name the manifest and the window use. Stable: it is a published key.
    pub fn key(self) -> &'static str {
        match self {
            Limit::RawLessThanInAttributeValue => "raw-less-than-in-attribute-value",
            Limit::ColonWithoutLocalName => "colon-without-local-name",
        }
    }

    /// Both of them, so a breakdown can list a class with no documents.
    pub const ALL: [Limit; 2] = [
        Limit::RawLessThanInAttributeValue,
        Limit::ColonWithoutLocalName,
    ];
}

/// One defect beyond this parser, and where it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Beyond {
    pub limit: Limit,
    /// Line of the first one, counted from 1 in the package copy — the same
    /// convention [`Finding`] uses, and the same one `xmllint` run over the
    /// package reports, so the two can be compared line for line.
    pub line: usize,
    /// Column, counted from 1 in characters.
    pub column: usize,
}

/// The first defect in `bytes` that this crate's parser accepts and XML does
/// not. `None` means there is none.
///
/// Meant for documents [`classify`] returned `None` for; it is safe on any
/// input, but on a document that is already refused the answer describes
/// wreckage. The scan walks tags the way [`unterminated_start_tag`] does —
/// comments, CDATA, declarations and processing instructions stepped over,
/// quoted values walked through — because those are the places where a `<` and
/// a `:` mean nothing.
pub fn beyond_the_parser(bytes: &[u8]) -> Option<Beyond> {
    let mut i = 0usize;
    while i < bytes.len() {
        let start = i + memchr::memchr(b'<', &bytes[i..])?;
        let rest = &bytes[start..];
        if rest.starts_with(b"<!--") {
            i = memchr_after(bytes, start + 4, b"-->");
            continue;
        }
        if rest.starts_with(b"<![CDATA[") {
            i = memchr_after(bytes, start + 9, b"]]>");
            continue;
        }
        // One walk of the tag, not two: `scan_tag` is `end_of_tag` with the
        // one extra question asked while the quotes are already being tracked.
        // Two walks cost a second pass over every tag of 340 MB to learn what
        // the first pass had in hand.
        let (end, quoted) = scan_tag(bytes, start);
        if rest.starts_with(b"<!") || rest.starts_with(b"<?") {
            i = end.max(start + 1);
            continue;
        }
        // Element name first, then the attributes, because that is the order
        // the bytes are in: reporting the first defect in the document means
        // reporting the leftmost one inside a tag too.
        let (name_at, name) = tag_name(bytes, start);
        if colon_without_local_name(name) {
            let (line, column) = position(bytes, name_at);
            return Some(Beyond {
                limit: Limit::ColonWithoutLocalName,
                line,
                column,
            });
        }
        if let Some(at) = quoted {
            let (line, column) = position(bytes, at);
            return Some(Beyond {
                limit: Limit::RawLessThanInAttributeValue,
                line,
                column,
            });
        }
        i = end.max(start + 1);
    }
    None
}

/// The name of the tag beginning at `start`, and where it begins.
///
/// `</w>` and `<w …>` alike: the slash of an end tag is stepped over, so a
/// closing tag is held to the same rule as the opening one. `libxml2` reports
/// the QName failure on both.
fn tag_name(bytes: &[u8], start: usize) -> (usize, &[u8]) {
    let mut at = start + 1;
    if bytes.get(at) == Some(&b'/') {
        at += 1;
    }
    let mut end = at;
    while end < bytes.len() {
        match bytes[end] {
            b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>' => break,
            _ => end += 1,
        }
    }
    (at, &bytes[at..end])
}

/// Whether a name carries a colon that no local name follows.
///
/// Deliberately narrow. It does not ask whether the name is a valid `NCName`
/// on both sides of the colon — that would be a second, wider judgement about
/// documents nobody has measured, and a class in the manifest is worth having
/// only when the documents under it were counted. It asks the one thing
/// `libxml2` refused on: after the first colon there has to be something that
/// can begin a name.
///
/// "Can begin a name" is taken as a letter, `_`, or any byte outside ASCII —
/// the last because `NameStartChar` covers most of Unicode and this corpus is
/// written in it. A digit, a hyphen, a dot, a second colon or nothing at all is
/// not a name start, which is exactly the set `AO:-italic` and `AO:` fall into.
fn colon_without_local_name(name: &[u8]) -> bool {
    let Some(colon) = memchr::memchr(b':', name) else {
        return false;
    };
    match name.get(colon + 1) {
        None => true,
        Some(&byte) => !(byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80),
    }
}

/// Where the tag beginning at `start` ends, and where inside it a `<` stands in
/// a quoted attribute value.
///
/// [`end_of_tag`] answers the first question and tracks quotes to do it; this
/// is that walk with the second question asked from the same state. The `<`
/// found here is the mirror of [`unquoted_lt`]'s: there a `<` **outside** the
/// quotes means the tag was never finished, here one **inside** them is a value
/// XML forbids. Different defects, different repairs — but the same single pass
/// over the bytes tells them apart.
fn scan_tag(bytes: &[u8], start: usize) -> (usize, Option<usize>) {
    let mut quoted_lt: Option<usize> = None;
    let mut i = start + 1;
    while i < bytes.len() {
        // Вне значения интересны ровно три байта из двухсот пятидесяти шести,
        // и искать их побайтно незачем: 96 % байтов корпуса лежат внутри тегов
        // (средний тег 51 байт), так что скалярный обход этого места был
        // обходом всех 339,5 МБ по одному байту — 7,26 % команд прогона,
        // замерено счетчиком инструкций 10.09.2026.
        let Some(offset) = memchr::memchr3(b'"', b'\'', b'>', &bytes[i..]) else {
            break;
        };
        let at = i + offset;
        let quote = match bytes[at] {
            b'>' => return (at + 1, quoted_lt),
            other => other,
        };
        // Внутри значения интересны два: закрывающая кавычка и голый `<`.
        // `>` здесь не значит ничего — в значении он не требует экранирования,
        // и этот корпус им пользуется.
        let mut j = at + 1;
        loop {
            let Some(offset) = memchr::memchr2(quote, b'<', &bytes[j..]) else {
                return (bytes.len(), quoted_lt);
            };
            let at = j + offset;
            if bytes[at] == quote {
                i = at + 1;
                break;
            }
            if quoted_lt.is_none() {
                quoted_lt = Some(at);
            }
            j = at + 1;
        }
    }
    (bytes.len(), quoted_lt)
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
    /// The two blind spots, found by the scanner that exists for them.
    ///
    /// The same two fixtures the test above asserts `classify` is silent on.
    /// Together the pair is the whole statement: this parser accepts these
    /// documents, and this crate knows exactly what is wrong with them.
    #[test]
    fn the_blind_spots_are_found_by_the_scanner_written_for_them() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/xml");
        let cases: [(&str, Limit); 2] = [
            (
                "malformed/unescaped-lt-in-attribute.xml",
                Limit::RawLessThanInAttributeValue,
            ),
            ("malformed/empty-qname.xml", Limit::ColonWithoutLocalName),
        ];
        for (name, want) in cases {
            let bytes = std::fs::read(dir.join(name)).expect("образец на месте");
            assert_eq!(classify(&bytes), None, "образец {name} разборщиком принят");
            assert_eq!(
                beyond_the_parser(&bytes).map(|b| b.limit),
                Some(want),
                "образец {name}"
            );
        }
    }

    /// And it is silent on documents that have neither defect.
    ///
    /// Both halves matter equally. A scanner that answers "yes" everywhere
    /// would put 23 936 documents in the manifest under a heading that means
    /// nothing; the corpus test holds it to seventeen, and this holds it to the
    /// shapes that look like the defect and are not: a colon that does have a
    /// local name after it, a `<` outside quotes, an `&lt;` where it belongs.
    #[test]
    fn a_document_with_neither_defect_says_so() {
        for xml in [
            r#"<AOxml><AO:TxtPubl>KBo 1.1</AO:TxtPubl></AOxml>"#,
            r#"<a><w trans="a&lt;b">x</w></a>"#,
            r#"<a><!-- a<b and AO:- --><w c="1"/></a>"#,
            r#"<a><![CDATA[a<b AO:-]]></a>"#,
            r#"<a><?xml-stylesheet href="a<b"?><w/></a>"#,
        ] {
            assert_eq!(beyond_the_parser(xml.as_bytes()), None, "для {xml}");
        }
    }

    /// An end tag is held to the same rule as the start tag.
    #[test]
    fn a_colon_without_a_local_name_is_found_on_an_end_tag_too() {
        let xml = r#"<a><AO:italic>x</AO:-italic></a>"#;
        assert_eq!(
            beyond_the_parser(xml.as_bytes()).map(|b| b.limit),
            Some(Limit::ColonWithoutLocalName)
        );
    }

    /// The position is the package copy's, the same as a finding's.
    #[test]
    fn the_limit_names_the_line_it_is_on() {
        let xml = "<a>\n  <w c=\"1\"/>\n  <w trans=\"a<b\"/>\n</a>";
        let beyond = beyond_the_parser(xml.as_bytes()).expect("найден");
        assert_eq!(beyond.limit, Limit::RawLessThanInAttributeValue);
        assert_eq!(beyond.line, 3);
    }

    /// The published keys, held the way the reasons' keys are.
    #[test]
    fn every_limit_names_itself_distinctly() {
        let mut keys: Vec<&str> = Limit::ALL.iter().map(|l| l.key()).collect();
        for key in &keys {
            assert!(
                !key.is_empty() && key.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "ключ {key} попадет в JSON и в разметку окна"
            );
        }
        let count = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), count, "два предела делят ключ");
        assert_eq!(count, 2);
    }
}
