//! The whole corpus, against the guarantees this project makes about it.
//!
//! Skipped when the archive is not there — it is 71 MiB and is not committed.
//! Set `ARUNA_REQUIRE_FIXTURE=1` to turn a missing archive into a failure, which
//! is what CI does after downloading it.
//!
//! ```sh
//! # the archive lives at cli/fixtures/, or name another with ARUNA_ZIP
//! cargo nextest run --test corpus
//! ARUNA_REQUIRE_FIXTURE=1 cargo nextest run --test corpus
//! ```
//!
//! What is checked here is the promise the project ranks first: that reading
//! the corpus does not change it, and that normalising a document changes
//! nothing the permit list does not name. Both were previously demonstrated by
//! a program someone had to remember to run.
//!
//! The counts asserted below are anchors, not specifications. They come from
//! `cargo run --release --example corpus_inventory` against TLHdig Beta 0.3 and
//! their job is to notice when a change to the gates silently admits or drops
//! documents. A new edition of the corpus is expected to move them, and moving
//! them is a deliberate edit here.

use aruna::export::{normalize_into, verify};
use aruna::parse::{is_manuscript_xml, looks_like_manuscript, HEADER_READ_LIMIT};
use std::io::Read as _;
use std::path::PathBuf;

/// What TLHdig Beta 0.3 holds.
const DOCUMENTS: usize = 23_936;
/// Documents whose start and end tags do not match. One of four classes of
/// malformed XML in this corpus; see the test at the bottom of this file.
///
/// Larger than the number of documents `xmllint` blames on a tag mismatch,
/// because `xmllint` stops at the first error and most of these have an
/// attribute error before it. Every one of them is inside the 210 it rejects.
const TAGS_DO_NOT_BALANCE: usize = 121;

/// Documents whose text is not in Unicode NFC.
///
/// Recorded rather than corrected — the corpus mixes forms and this program
/// does not touch a source document — so the number is an anchor like the two
/// above: it says a renderer will meet decomposed diacritics in this many
/// documents and had better place marks itself.
const NOT_NFC: usize = 78;

/// The archive, wherever this run keeps it.
///
/// Three names, in this order, and the second one is why this file exists as it
/// does: `ARUNA_ZIP` is what a person passes and what the binary itself honours;
/// `ARUNA_FIXTURE_ZIP` is what CI sets, because there the 71 MiB download lives
/// in a cache directory outside the checkout. This suite used to read only the
/// first, so the corpus job — which sets only the second, and sets
/// `ARUNA_REQUIRE_FIXTURE=1` precisely so that a skip cannot pass — never ran a
/// line of it, and the failure mode was silence rather than an error. Both are
/// read here now, and `tests/integration.rs` reads the same pair.
fn archive() -> Option<PathBuf> {
    for name in ["ARUNA_ZIP", "ARUNA_FIXTURE_ZIP"] {
        if let Some(named) = std::env::var_os(name) {
            return Some(PathBuf::from(named));
        }
    }
    let default = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip");
    default.is_file().then_some(default)
}

/// The archive, or a note saying why this test did nothing.
fn required() -> Option<PathBuf> {
    match archive() {
        Some(path) if path.is_file() => Some(path),
        other => {
            assert!(
                std::env::var_os("ARUNA_REQUIRE_FIXTURE").is_none(),
                "ARUNA_REQUIRE_FIXTURE is set but {:?} is missing",
                other
            );
            eprintln!("skipping: the corpus archive is not present");
            None
        }
    }
}

#[test]
fn the_whole_corpus_normalises_without_distortion_and_the_archive_is_not_written_to() {
    let Some(path) = required() else { return };

    let before = aruna::md5::md5_file(&path).expect("digest");
    let file = std::fs::File::open(&path).expect("open");
    let mut zip = zip::ZipArchive::new(std::io::BufReader::with_capacity(1 << 18, file))
        .expect("read archive");

    let (mut checked, mut refused) = (0usize, 0usize);
    let mut distorted: Vec<String> = Vec::new();
    let mut applied: std::collections::BTreeMap<String, usize> = Default::default();
    let mut source = Vec::new();
    let mut out = Vec::new();

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).expect("entry");
        let name = entry.name().to_string();
        if !is_manuscript_xml(&name) {
            continue;
        }
        source.clear();
        entry.read_to_end(&mut source).expect("read entry");
        let head = String::from_utf8_lossy(&source[..source.len().min(HEADER_READ_LIMIT)]);
        if !looks_like_manuscript(&head) {
            refused += 1;
            continue;
        }
        checked += 1;

        out.clear();
        normalize_into(&source, &mut out);
        match verify::compare(&source, &out) {
            Ok(report) => {
                // No longer asserted here that the rule is on the permit
                // list: since 2026-09-10 `compare` returns the list's own name
                // rather than the document's spelling, so the type carries what
                // this line used to check. `verify::a_dropped_instruction_is_counted_under_the_permitted_name`
                // holds that property where it now lives.
                for rule in report.dropped {
                    *applied.entry(format!("DROP_PI {rule}")).or_default() += 1;
                }
                if report.added_declaration {
                    *applied.entry("ADD declaration".into()).or_default() += 1;
                }
                if report.reflowed {
                    *applied.entry("REFLOW".into()).or_default() += 1;
                }
            }
            Err(why) if distorted.len() < 10 => distorted.push(format!("{name}: {why}")),
            Err(_) => {}
        }
    }

    assert!(
        distorted.is_empty(),
        "{} document(s) distorted, first few: {distorted:#?}",
        distorted.len()
    );
    assert_eq!(
        checked, DOCUMENTS,
        "the gates admitted a different number of documents than the inventory recorded"
    );
    assert!(
        refused > 0,
        "the debris in the archive should still be refused"
    );
    eprintln!("corpus: {checked} documents, permitted changes applied: {applied:?}");

    let after = aruna::md5::md5_file(&path).expect("digest");
    assert_eq!(before, after, "the archive was written to");
}

/// The corpus contains documents no conforming XML parser will accept.
///
/// 210 of 23 936 are not well-formed XML, and 223 are objected to at all;
/// re-measured with `xmllint --noout` on 2026-09-10, and the three numbers are
/// laid out in `docs/XML-CONTRACT.md` §2 and in the manifest's `xml.totals`.
/// `xmllint` reports the first error in each and blames: an attribute name that
/// is not a name (82), a tag mismatch (65), a raw `<` inside an attribute value
/// (44), an attribute construct (15), a qualified name with no local part (13),
/// and five others. Counting the whole document rather than its first error, 121
/// have tags that do not balance — most of those also have an attribute error
/// earlier, which is what `xmllint` stops on. All the classes are reproduced in
/// `fixtures/xml/malformed/`.
///
/// The breakdown written here until 2026-09-10 said 54 tag mismatches and put
/// the 13 qualified names inside the 210. It summed to 210 because the two
/// errors cancelled: the 13 are a `namespace error` that `libxml2` exits zero
/// on, and they belong to the 223, not the 210.
///
/// Only the tag-mismatch class is counted here, because it is the only one this
/// project can measure without shipping an XML parser it does not have. The
/// other three are recorded in `docs/XML-CONTRACT.md` with the command that
/// produced them.
///
/// None of this is a failure. The corpus is the source of truth and repairing it
/// in place is the one thing this project must never do. The number is for the
/// next stage: a converter built on a strict parser will refuse these documents
/// and needs a stated policy before it meets them rather than after.
#[test]
fn the_documents_whose_tags_do_not_balance_are_the_ones_already_known() {
    let Some(path) = required() else { return };
    let file = std::fs::File::open(&path).expect("open");
    let mut zip = zip::ZipArchive::new(std::io::BufReader::with_capacity(1 << 18, file))
        .expect("read archive");

    let mut unbalanced = Vec::new();
    let mut source = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).expect("entry");
        let name = entry.name().to_string();
        if !is_manuscript_xml(&name) {
            continue;
        }
        source.clear();
        entry.read_to_end(&mut source).expect("read entry");
        let head = String::from_utf8_lossy(&source[..source.len().min(HEADER_READ_LIMIT)]);
        if !looks_like_manuscript(&head) {
            continue;
        }
        if !balanced(&source) {
            unbalanced.push(name);
        }
    }

    assert_eq!(
        unbalanced.len(),
        TAGS_DO_NOT_BALANCE,
        "the corpus changed shape; first few: {:#?}",
        &unbalanced[..unbalanced.len().min(5)]
    );
}

/// Whether every end tag closes the element the last start tag opened.
///
/// Deliberately the smallest check that answers one question. It is not an XML
/// parser and must not grow into one: this project has no parser, and a partial
/// one living in a test is how a project ends up with two.
fn balanced(bytes: &[u8]) -> bool {
    let mut stack: Vec<&[u8]> = Vec::new();
    let mut i = 0usize;
    while let Some(open) = memchr_lt(&bytes[i..]) {
        i += open;
        let rest = &bytes[i..];
        let skip_to = |needle: &[u8]| {
            rest.windows(needle.len())
                .position(|w| w == needle)
                .map_or(rest.len(), |at| at + needle.len())
        };
        if rest.starts_with(b"<!--") {
            i += skip_to(b"-->");
            continue;
        }
        if rest.starts_with(b"<![CDATA[") {
            i += skip_to(b"]]>");
            continue;
        }
        if rest.starts_with(b"<?") || rest.starts_with(b"<!") {
            i += skip_to(b">");
            continue;
        }
        let end = skip_to(b">");
        let tag = &rest[..end];
        if rest.starts_with(b"</") {
            let name = local(&tag[2..tag.len().saturating_sub(1)]);
            match stack.pop() {
                Some(open) if open == name => {}
                _ => return false,
            }
        } else if !tag.ends_with(b"/>") {
            let inner = &tag[1..tag.len().saturating_sub(1)];
            let cut = inner
                .iter()
                .position(u8::is_ascii_whitespace)
                .unwrap_or(inner.len());
            stack.push(local(&inner[..cut]));
        }
        i += end;
    }
    stack.is_empty()
}

fn memchr_lt(bytes: &[u8]) -> Option<usize> {
    bytes.iter().position(|b| *b == b'<')
}

/// The part of a qualified name after the colon.
fn local(qname: &[u8]) -> &[u8] {
    match qname.iter().position(|c| *c == b':') {
        Some(at) => &qname[at + 1..],
        None => qname,
    }
}

/// **Nothing the pipeline reads comes out damaged.**
///
/// The one property that cannot be checked on a fixture, because the fixture
/// is written by whoever writes the test: a corpus of 23 936 documents from
/// four editorial series, decoded and folded into the model that every
/// renderer reads. What this asserts is that not one of them arrives with a
/// replacement character, a stray control code, an unusual space, a zero-width
/// mark, a soft hyphen or a bidi override in it.
///
/// **The replacement character is the sharp one.** U+FFFD is what decoding
/// produces when the bytes were not what they were taken for, and it is
/// invisible in an inventory — a manuscript listed under a name with a black
/// diamond in it looks like a corpus problem rather than a program one. The
/// count is zero and this test is what keeps it there.
///
/// The archive does contain 643 files that are not UTF-8: `__MACOSX/._*.xml`,
/// the AppleDouble resource-fork stubs a Mac put in the zip. They are binary,
/// they are not manuscripts, and the gates refuse every one of them — which is
/// why this walks the corpus through `is_manuscript_xml` and
/// `looks_like_manuscript` rather than over every entry ending in `.xml`.
/// Counting them as corpus damage was the first answer this test gave, and it
/// was wrong.
#[test]
fn every_document_the_gates_admit_survives_decoding_intact() {
    let Some(path) = required() else { return };

    let file = std::fs::File::open(&path).expect("open the archive");
    let mut archive =
        zip::ZipArchive::new(std::io::BufReader::new(file)).expect("read the archive");

    let mut contract = aruna::export::manifest::FontContract::default();
    let mut admitted = 0usize;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).expect("entry");
        if !is_manuscript_xml(entry.name()) {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).expect("read the document");
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(HEADER_READ_LIMIT)]);
        if !looks_like_manuscript(&head) {
            continue;
        }
        admitted += 1;
        contract.observe(&String::from_utf8_lossy(&bytes));
    }

    assert_eq!(
        admitted, DOCUMENTS,
        "the gates admitted a different number of documents than the export reports"
    );

    for (name, count) in contract.anomalies.counts() {
        assert_eq!(
            count, 0,
            "{count} documents carry {name}; the pipeline is damaging text or the corpus changed"
        );
    }

    // Not an anomaly, and not corrected: an anchor, so that a change in the
    // corpus or in how it is read is noticed rather than assumed.
    assert_eq!(
        contract.not_nfc, NOT_NFC,
        "the number of documents outside NFC moved"
    );
}

/// The seventeen documents this crate's parser accepts and `libxml2` objects to.
///
/// Paths inside the archive, with its own top folder stripped — not package
/// paths, because `place` renames a colliding document and the archive is what
/// this test reads.
///
/// **Why they are written out rather than counted.** A count would pass on a
/// scanner that found seventeen different documents. These names are the ones
/// `xmllint --noout` blames over the exported package, measured 2026-09-10, and
/// naming them is what makes [`aruna::xml_wellformed::beyond_the_parser`] a
/// reproduction of that measurement rather than a second opinion about it.
///
/// A new edition of the corpus will move this list, and moving it is an edit
/// here, made after re-running `xmllint` — never after reading a diff.
const RAW_LESS_THAN: [&str; 4] = [
    "CTH 211_XML_TLH/DAAM 6.93.xml",
    "CTH 447_XML_BESRIT/KBo 11.72+.xml",
    "CTH 581_XML_HDivT/KBo 18.142.xml",
    "CTH 76_XML_SVH/KUB 19.6+.xml",
];

/// The thirteen with `<AO:-…>`: a colon, and after it something that cannot
/// begin a local name.
///
/// `libxml2` calls this a *namespace* error and exits zero on it, which is why
/// these thirteen went uncounted from the first measurement in August until
/// 2026-09-10 while their four neighbours above were known all along.
const COLON_WITHOUT_LOCAL_NAME: [&str; 13] = [
    "CTH 52_XML_SVH/KBo 1.3+.xml",
    "CTH 526_XML_KULTINV/KUB 25.23+.xml",
    "CTH 528_XML_KULTINV/KBo 13.192.xml",
    "CTH 577_XML_HDivT/KUB 52.25.xml",
    "CTH 61_XML_HAnn/KBo 50.19.xml",
    "CTH 61_XML_HAnn/KBo 50.30+.xml",
    "CTH 628_XML_HFR/IBoT 2.85+.xml",
    "CTH 628_XML_HFR/KBo 47.71.xml",
    "CTH 647_XML_HFR/KBo 31.190.xml",
    "CTH 670_XML_HFR/CTH 670-0026-0050/KBo 39.105.xml",
    "CTH 670_XML_HFR/CTH 670-3926-3950/KBo 18.192.xml",
    "CTH 670_XML_HFR/CTH 670-aus-CTH 832/KBo 52.74.xml",
    "CTH 670_XML_HFR/CTH 670-aus-CTH 832/KBo 66.131.xml",
];

/// What this crate's parser accepts and a conforming one does not — by name.
///
/// The acceptance test for the scanner that closes the gap between the 206 this
/// crate refuses and the 223 `libxml2` objects to. Set equality in both
/// directions, because both directions are defects: a name missing means the
/// scanner stopped seeing a document `xmllint` blames, and an extra name means
/// it invented one, and the manifest would publish either.
///
/// Run over the *normalised* bytes, since those are what the package holds and
/// what `xmllint` was run against.
#[test]
fn the_documents_beyond_this_parser_are_the_seventeen_xmllint_names() {
    let Some(path) = required() else { return };
    let file = std::fs::File::open(&path).expect("open");
    let mut zip = zip::ZipArchive::new(std::io::BufReader::with_capacity(1 << 18, file))
        .expect("read archive");

    let mut raw_lt: Vec<String> = Vec::new();
    let mut no_local: Vec<String> = Vec::new();
    let mut source = Vec::new();
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).expect("entry");
        let name = entry.name().to_string();
        if !is_manuscript_xml(&name) {
            continue;
        }
        source.clear();
        entry.read_to_end(&mut source).expect("read entry");
        let head = String::from_utf8_lossy(&source[..source.len().min(HEADER_READ_LIMIT)]);
        if !looks_like_manuscript(&head) {
            continue;
        }
        out.clear();
        normalize_into(&source, &mut out);
        // Only the ones this parser accepts. A document it already refuses is
        // counted in the 206 and would be counted twice here.
        if aruna::xml_wellformed::classify(&out).is_some() {
            continue;
        }
        let Some(beyond) = aruna::xml_wellformed::beyond_the_parser(&out) else {
            continue;
        };
        let short = name
            .split_once('/')
            .map_or(name.clone(), |(_, r)| r.to_string());
        match beyond.limit {
            aruna::xml_wellformed::Limit::RawLessThanInAttributeValue => raw_lt.push(short),
            aruna::xml_wellformed::Limit::ColonWithoutLocalName => no_local.push(short),
        }
    }

    raw_lt.sort();
    no_local.sort();
    let mut expected_raw = RAW_LESS_THAN.map(str::to_string).to_vec();
    let mut expected_local = COLON_WITHOUT_LOCAL_NAME.map(str::to_string).to_vec();
    expected_raw.sort();
    expected_local.sort();

    assert_eq!(
        raw_lt, expected_raw,
        "the documents with a raw '<' in an attribute value are not the ones xmllint blames"
    );
    assert_eq!(
        no_local, expected_local,
        "the documents with a colon and no local name are not the ones xmllint blames"
    );
    assert_eq!(
        raw_lt.len() + no_local.len(),
        17,
        "206 refused here plus 17 accepted here and objected to elsewhere is the 223"
    );
}

/// **Структурный критерий на всем корпусе: пакетная копия — то же дерево.**
///
/// Байтовый критерий — `verify::compare` — уже идет по всем 23 936 документам
/// в первом тесте этого файла и доказывает, что тело документа побайтово то
/// же. О структуре он не говорит ничего, и §4.13 стоит ровно на этом уроке:
/// последовательность знаков не меняется от того, где закрыть элемент.
///
/// Здесь тот же вопрос задает сторонняя реализация и задает его о дереве:
/// `xmllint --c14n` приводит порядок атрибутов, кавычки и запись пустого
/// элемента к одному виду, так что совпадение канонических форм — это
/// совпадение деревьев, а не байтов. Разрешенный список снимается с обеих
/// сторон одинаково: C14N сохраняет инструкции обработки вне корня, а снятая
/// ссылка на таблицу стилей — то самое, что нормализации разрешено убрать.
///
/// Документы, которых сторонний разборщик не берет, пропускаются: их 210, они
/// остаются в пакете, и отсутствие у них канонической формы — свойство
/// исходных данных.
///
/// `#[ignore]`, потому что это 23 936 запусков внешней программы — минуты, а
/// не секунды. Включается:
///
/// ```sh
/// ARUNA_ZIP=cli/fixtures/…zip cargo nextest run --test corpus --run-ignored all
/// ```
#[test]
#[ignore = "23 936 запусков xmllint: минуты; включается вручную"]
fn every_document_in_the_package_is_the_same_tree_as_in_the_archive() {
    let Some(path) = required() else { return };
    if std::process::Command::new("xmllint")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| !s.success())
        .unwrap_or(true)
    {
        eprintln!("xmllint отсутствует — проверка пропущена");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("source.xml");
    let dst = dir.path().join("normalised.xml");
    let file = std::fs::File::open(&path).expect("open");
    let mut zip = zip::ZipArchive::new(std::io::BufReader::with_capacity(1 << 18, file))
        .expect("read archive");

    let (mut same, mut refused) = (0usize, 0usize);
    let mut differing: Vec<String> = Vec::new();
    let mut source = Vec::new();
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).expect("entry");
        let name = entry.name().to_string();
        if !is_manuscript_xml(&name) {
            continue;
        }
        source.clear();
        entry.read_to_end(&mut source).expect("read entry");
        let head = String::from_utf8_lossy(&source[..source.len().min(HEADER_READ_LIMIT)]);
        if !looks_like_manuscript(&head) {
            continue;
        }
        out.clear();
        normalize_into(&source, &mut out);
        if verify::compare(&source, &out).is_err() {
            continue;
        }
        let Some(before) = canonical_form(&source, &src) else {
            refused += 1;
            continue;
        };
        let after = canonical_form(&out, &dst)
            .unwrap_or_else(|| panic!("{name}: пакетная копия перестала разбираться"));
        if without_dropped_pis(&before) == without_dropped_pis(&after) {
            same += 1;
        } else if differing.len() < 5 {
            differing.push(name);
        }
    }

    assert!(
        differing.is_empty(),
        "у {} документов дерево изменилось при совпавших байтах тела; первые: {differing:#?}",
        differing.len()
    );
    assert!(
        same > 23_000,
        "сверено всего {same} документов при {refused} отвергнутых сторонним разборщиком — \
         выборка перестала быть корпусом"
    );
    eprintln!("структурный критерий: {same} совпало, {refused} без канонической формы");
}

/// Каноническая форма по `xmllint --c14n`, или `None` — документ ей не дался.
fn canonical_form(bytes: &[u8], at: &std::path::Path) -> Option<Vec<u8>> {
    std::fs::write(at, bytes).expect("write");
    let out = std::process::Command::new("xmllint")
        .arg("--c14n")
        .arg(at)
        .output()
        .expect("run xmllint");
    out.status.success().then_some(out.stdout)
}

/// Каноническая форма без инструкций, которые нормализации разрешено снимать,
/// и без оставшегося от них пробела.
fn without_dropped_pis(canonical: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(canonical);
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_ref();
    while let Some(at) = rest.find("<?") {
        let (before, tail) = rest.split_at(at);
        out.push_str(before);
        let Some(end) = tail.find("?>") else {
            rest = tail;
            break;
        };
        let pi = &tail[2..end];
        let target = pi.split([' ', '\t', '\r', '\n']).next().unwrap_or(pi);
        if !verify::is_dropped(target.as_bytes()) {
            out.push_str(&tail[..end + 2]);
        }
        rest = &tail[end + 2..];
    }
    out.push_str(rest);
    out.trim_start().as_bytes().to_vec()
}
