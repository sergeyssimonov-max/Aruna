//! Classify the documents the corpus contains that are not well-formed XML.
//!
//! ```text
//! cargo run --release --example xml_wellformedness
//! cargo run --release --example xml_wellformedness -- fixtures/…zip
//! ARUNA_ZIP=/path/to.zip cargo run --release --example xml_wellformedness
//! ```
//!
//! Step 2 of the parser acceptance in `docs/PROJECT-SPEC.ru.md` §4.13. Step 1
//! measured what `quick-xml` costs the tree; this measures whether it fits. The
//! decision it feeds is single: are the 210 defects repairable by a
//! deterministic byte-level fix in normalisation — where §4.13 wants them — or
//! do they need a parser that recovers from errors.
//!
//! Three things it must show, and each is printed rather than argued:
//!
//! * how many classes of defect there are, and how unlike each other they are;
//! * for each class, enough of the failing bytes to judge whether one rule
//!   repairs every member of it;
//! * whether a failure lands inside `body` — the transliteration itself —
//!   rather than in the header. A silent repair there changes data, so those
//!   are the dangerous ones however easy the fix looks.
//!
//! Nothing is written to disk and nothing is repaired. The archive is read, and
//! its digest is taken before and after to say so.
//!
//! What is parsed is the *normalised* form of each document, not the bytes as
//! they sit in the archive: normalisation is what the package contains, and the
//! 210 was measured on the package. It only ever touches the prologue, so no
//! failure below can be an artefact of it — but the run would be measuring the
//! wrong bytes if it read the archive directly.
//!
//! Nothing here expands anything. Entities, DTDs and XInclude stay unexpanded
//! because no call that expands them is made — not because a flag says so.

use aruna::export::normalize_into;
use aruna::parse::{is_manuscript_xml, looks_like_manuscript, HEADER_READ_LIMIT};
use quick_xml::errors::{Error, IllFormedError, SyntaxError};
use quick_xml::events::attributes::AttrError;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

/// What `docs/XML-CONTRACT.md` recorded with `xmllint --noout`.
///
/// Documents that are not well-formed XML, which is what `xmllint` exits
/// non-zero on. It is not the widest figure: thirteen more are objected to as a
/// namespace error, which `libxml2` exits zero on, and 206 + 4 + 13 = 223 is
/// what a conforming parser refuses altogether. The three are laid out in
/// `docs/XML-CONTRACT.md` §2 and written into the manifest's `xml.totals`.
const EXPECTED_MALFORMED: usize = 210;
/// Documents in the corpus, the denominator every other figure is read against.
const EXPECTED_DOCUMENTS: usize = 23_936;

/// One refusal: where it happened and what the bytes look like there.
struct Failure {
    document: String,
    class: &'static str,
    detail: String,
    line: usize,
    column: usize,
    /// Elements open at the point of failure, outermost first.
    path: Vec<String>,
    fragment: String,
}

impl Failure {
    /// Whether the failure is inside the transliteration rather than the header.
    ///
    /// `AOxml` holds `AOHeader` and `body`; everything a reader of the edition
    /// would call content is under the second. A repair in the header rewrites
    /// bookkeeping, a repair under `body` rewrites the text.
    fn in_content(&self) -> bool {
        self.path.iter().any(|name| name == "body")
    }
}

fn main() -> ExitCode {
    let Some(zip) = archive() else {
        eprintln!(
            "no archive: pass one as the first argument, set ARUNA_ZIP, or put it at\n\
             cli/fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip"
        );
        return ExitCode::FAILURE;
    };

    let before = match aruna::md5::md5_file(&zip) {
        Ok(digest) => digest,
        Err(err) => {
            eprintln!("cannot read {}: {err}", zip.display());
            return ExitCode::FAILURE;
        }
    };

    let file = match std::fs::File::open(&zip) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("cannot open {}: {err}", zip.display());
            return ExitCode::FAILURE;
        }
    };
    let mut corpus = match zip::ZipArchive::new(std::io::BufReader::with_capacity(1 << 18, file)) {
        Ok(archive) => archive,
        Err(err) => {
            eprintln!("cannot read {}: {err}", zip.display());
            return ExitCode::FAILURE;
        }
    };

    let mut documents = 0usize;
    let mut failures: Vec<Failure> = Vec::new();
    let mut source = Vec::new();
    let mut normalised = Vec::new();

    for i in 0..corpus.len() {
        let mut entry = match corpus.by_index(i) {
            Ok(entry) => entry,
            Err(err) => {
                eprintln!("entry {i} could not be read: {err}");
                continue;
            }
        };
        let name = entry.name().to_string();
        if !is_manuscript_xml(&name) {
            continue;
        }
        source.clear();
        if entry.read_to_end(&mut source).is_err() {
            continue;
        }
        let head = String::from_utf8_lossy(&source[..source.len().min(HEADER_READ_LIMIT)]);
        if !looks_like_manuscript(&head) {
            continue;
        }

        normalised.clear();
        normalize_into(&source, &mut normalised);
        documents += 1;
        classify(&name, &normalised, &mut failures);
    }

    let after = match aruna::md5::md5_file(&zip) {
        Ok(digest) => digest,
        Err(err) => {
            eprintln!("cannot re-read {}: {err}", zip.display());
            return ExitCode::FAILURE;
        }
    };

    report(&zip, documents, &failures, &before, &after);
    ExitCode::SUCCESS
}

/// Run the parser over one document, recording every refusal it produces.
///
/// Two kinds of refusal, and the difference matters for what can be counted.
/// A reader-level error ends the document: the parser's state is no longer
/// trustworthy and everything after it would be invented. An attribute error
/// does not — `Attributes` is a lazy iterator sitting on top of a tag the
/// reader has already read whole, so the scan continues and a document can
/// yield many. Both are recorded; the summary counts documents, not refusals.
fn classify(document: &str, bytes: &[u8], out: &mut Vec<Failure>) {
    let mut reader = Reader::from_reader(bytes);
    let mut buf = Vec::new();
    let mut path: Vec<String> = Vec::new();

    loop {
        let start = reader.buffer_position() as usize;
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => return,
            Ok(event) => {
                if let Event::Start(ref tag) | Event::Empty(ref tag) = event {
                    for attribute in tag.attributes() {
                        if let Err(err) = attribute {
                            let (class, offset) = attr_class(&err);
                            out.push(failure(
                                document,
                                class,
                                err.to_string(),
                                bytes,
                                // `AttrError` counts from the first byte after
                                // `<`, so the tag's own offset plus one puts the
                                // column on the defect rather than on the tag.
                                start + 1 + offset,
                                &path,
                            ));
                        }
                    }
                }
                match event {
                    Event::Start(tag) => {
                        path.push(tag.name().into_inner().to_owned());
                    }
                    Event::End(_) => {
                        path.pop();
                    }
                    _ => {}
                }
            }
            Err(err) => {
                let at = reader.error_position() as usize;
                out.push(failure(
                    document,
                    error_class(&err),
                    err.to_string(),
                    bytes,
                    at,
                    &path,
                ));
                return;
            }
        }
    }
}

/// The class of a reader-level refusal.
///
/// Matched variant by variant on purpose. Formatting the error and cutting the
/// payload off the front would group by message text, which changes with the
/// crate version and with the document's own names.
fn error_class(err: &Error) -> &'static str {
    match err {
        Error::Syntax(SyntaxError::UnclosedTag) => "tag not closed before end of input",
        Error::Syntax(SyntaxError::UnclosedComment) => "comment not closed",
        Error::Syntax(SyntaxError::UnclosedCData) => "CDATA not closed",
        Error::Syntax(SyntaxError::UnclosedXmlDecl) => "declaration not closed",
        Error::Syntax(SyntaxError::UnclosedDoctype) => "doctype not closed",
        Error::Syntax(SyntaxError::InvalidBangMarkup) => "malformed <! markup",
        Error::Syntax(_) => "other syntax error",
        Error::IllFormed(IllFormedError::MismatchedEndTag { .. }) => {
            "end tag closes another element"
        }
        Error::IllFormed(IllFormedError::UnmatchedEndTag(_)) => "end tag with nothing open",
        Error::IllFormed(IllFormedError::MissingEndTag(_)) => "element never closed",
        Error::IllFormed(IllFormedError::DoubleHyphenInComment) => "double hyphen inside comment",
        Error::IllFormed(IllFormedError::MissingDeclVersion(_)) => "declaration without version",
        Error::IllFormed(_) => "other ill-formed document",
        Error::Encoding(_) => "encoding refused",
        Error::Io(_) => "read error",
        _ => "other",
    }
}

/// The class of an attribute-level refusal, and where inside the tag it is.
fn attr_class(err: &AttrError) -> (&'static str, usize) {
    match err {
        AttrError::ExpectedEq(at) => ("attribute name not followed by =", *at),
        AttrError::ExpectedValue(at) => ("attribute without a value", *at),
        AttrError::ExpectedQuote(at, _) => ("attribute value not quoted", *at),
        AttrError::UnquotedValue(at) => ("attribute value unquoted", *at),
        AttrError::Duplicated(at, _) => ("attribute given twice", *at),
    }
}

fn failure(
    document: &str,
    class: &'static str,
    detail: String,
    bytes: &[u8],
    at: usize,
    path: &[String],
) -> Failure {
    let (line, column) = position(bytes, at);
    Failure {
        document: document.to_string(),
        class,
        detail,
        line,
        column,
        path: path.to_vec(),
        fragment: fragment(bytes, at),
    }
}

/// Line and column of a byte offset, counted in one pass.
///
/// Columns are counted in characters rather than bytes: the corpus is dense
/// with cuneiform, and a byte column would be three or four times the number a
/// person opening the file in an editor sees.
fn position(bytes: &[u8], at: usize) -> (usize, usize) {
    let at = at.min(bytes.len());
    let line = 1 + memchr::memchr_iter(b'\n', &bytes[..at]).count();
    let start = memchr::memrchr(b'\n', &bytes[..at]).map_or(0, |i| i + 1);
    let column = 1 + String::from_utf8_lossy(&bytes[start..at]).chars().count();
    (line, column)
}

/// The bytes around a refusal, on one line and safe to print.
fn fragment(bytes: &[u8], at: usize) -> String {
    const BEFORE: usize = 45;
    const AFTER: usize = 55;
    let at = at.min(bytes.len());
    let from = at.saturating_sub(BEFORE);
    let to = (at + AFTER).min(bytes.len());
    let mut out = String::new();
    if from > 0 {
        out.push('…');
    }
    for ch in String::from_utf8_lossy(&bytes[from..to]).chars() {
        match ch {
            '\n' | '\r' | '\t' => out.push(' '),
            ch if (ch as u32) < 0x20 => out.push('·'),
            ch => out.push(ch),
        }
    }
    if to < bytes.len() {
        out.push('…');
    }
    out
}

fn report(
    zip: &std::path::Path,
    documents: usize,
    failures: &[Failure],
    before: &str,
    after: &str,
) {
    // Documents per class, and documents overall: a document with an attribute
    // error and a tag mismatch is one document in each of two classes and one
    // document in the total, which is why both are counted over names.
    let mut by_class: BTreeMap<&'static str, Vec<&Failure>> = BTreeMap::new();
    for failure in failures {
        by_class.entry(failure.class).or_default().push(failure);
    }
    let refused: std::collections::BTreeSet<&str> =
        failures.iter().map(|f| f.document.as_str()).collect();
    let in_content: std::collections::BTreeSet<&str> = failures
        .iter()
        .filter(|f| f.in_content())
        .map(|f| f.document.as_str())
        .collect();

    println!();
    println!("archive:            {}", zip.display());
    println!("documents parsed:   {documents}");
    println!("  expected:         {EXPECTED_DOCUMENTS}");
    println!("documents refused:  {}", refused.len());
    println!("  expected:         {EXPECTED_MALFORMED}   (xmllint, docs/XML-CONTRACT.md)");
    println!("refusals in total:  {}", failures.len());
    println!("classes:            {}", by_class.len());
    println!(
        "refused inside body:{:>4}   ← a silent repair here changes the transliteration",
        in_content.len()
    );
    println!();

    // The first refusal in a document is the one worth counting. Everything
    // after it is the parser walking through wreckage: an attribute value whose
    // quote never closes swallows the next few hundred bytes, and every tag
    // inside them is reported again. 3 763 refusals in one class come from far
    // fewer defects, and a repair rule has to be written against the first.
    let mut first: Vec<&Failure> = Vec::new();
    {
        let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for failure in failures {
            if seen.insert(failure.document.as_str()) {
                first.push(failure);
            }
        }
    }
    let mut roots: BTreeMap<&'static str, Vec<&Failure>> = BTreeMap::new();
    for failure in &first {
        roots.entry(failure.class).or_default().push(failure);
    }

    println!("=== root causes: the first refusal in each document ===");
    println!();
    for (class, group) in &roots {
        println!("{:<38} {:>4} documents", class, group.len());
    }
    println!();
    for (class, group) in &roots {
        println!("--- {class} ---");
        for example in group.iter().take(4) {
            println!("    {}", example.document);
            println!(
                "      line {}, column {}, inside <{}>",
                example.line,
                example.column,
                example.path.last().map_or("\u{2014}", String::as_str)
            );
            println!("      {}", example.detail);
            println!("      {}", example.fragment);
        }
        println!();
    }

    println!("=== classes, by documents ===");
    println!();
    for (class, group) in &by_class {
        let names: std::collections::BTreeSet<&str> =
            group.iter().map(|f| f.document.as_str()).collect();
        let content = group.iter().filter(|f| f.in_content()).count();
        println!(
            "{:<38} {:>4} documents, {:>4} refusals, {} inside body",
            class,
            names.len(),
            group.len(),
            content
        );
        for example in group.iter().take(3) {
            println!("    {}", example.document);
            println!(
                "      line {}, column {}, inside <{}>",
                example.line,
                example.column,
                example.path.last().map_or("—", String::as_str)
            );
            println!("      {}", example.detail);
            println!("      {}", example.fragment);
        }
        println!();
    }

    println!("=== every refusal, class then document ===");
    println!();
    for (class, group) in &by_class {
        for failure in group {
            println!(
                "{class}\t{}\t{}:{}\t{}\t{}",
                failure.document,
                failure.line,
                failure.column,
                if failure.in_content() {
                    "body"
                } else {
                    "header"
                },
                failure.fragment
            );
        }
    }
    println!();

    println!("source digest:      {before}");
    println!(
        "  after the run:    {after}   {}",
        if before == after {
            "unchanged"
        } else {
            "CHANGED — the source was written to"
        }
    );
    if documents != EXPECTED_DOCUMENTS {
        println!();
        println!("note: the corpus holds {documents} documents, not {EXPECTED_DOCUMENTS}");
    }
    if refused.len() != EXPECTED_MALFORMED {
        println!();
        println!(
            "note: {} documents refused against {EXPECTED_MALFORMED} recorded. The two counts are \
             made by different parsers and need not agree — `quick-xml` does not reject a raw `<` \
             inside an attribute value, which is a class `xmllint` counts. The difference is a \
             finding about the parser, not about the corpus.",
            refused.len()
        );
    }
}

/// The archive, wherever this run keeps it.
///
/// An explicit argument wins, because that is how every other example in this
/// crate is invoked. Then `ARUNA_ZIP` and `ARUNA_FIXTURE_ZIP`, the pair the
/// binary and the test suite honour — the second is what CI sets, since there
/// the 71 MiB download lives outside the checkout. Then the fixture. No path is
/// written into the code.
fn archive() -> Option<PathBuf> {
    if let Some(given) = std::env::args_os().nth(1) {
        return Some(PathBuf::from(given));
    }
    for name in ["ARUNA_ZIP", "ARUNA_FIXTURE_ZIP"] {
        if let Some(named) = std::env::var_os(name) {
            return Some(PathBuf::from(named));
        }
    }
    let default = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip");
    default.is_file().then_some(default)
}
