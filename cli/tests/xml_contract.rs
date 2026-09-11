//! The contract between the corpus and this program, checked against fixtures.
//!
//! Three things are being held to account here, in the order the project ranks
//! them:
//!
//! 1. The source is never written to. Every fixture is read, put through
//!    everything this program does, and compared byte for byte afterwards.
//! 2. Nothing is lost or invented between a document and its normalised form,
//!    beyond a permit list that is stated in one place and checked here.
//! 3. Nothing panics, whatever the document is — including the four classes of
//!    malformed XML the real corpus actually contains.
//!
//! Every fixture is synthetic and is described in `fixtures/xml/MANIFEST.md`.
//! Where one reproduces something the corpus really has, the manifest says how
//! many documents that is.

use aruna::export::{normalize_into, verify};
use aruna::parse::{is_manuscript_xml, looks_like_manuscript, parse_manuscript, HEADER_READ_LIMIT};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/xml")
}

/// Every fixture, as (relative name, bytes).
fn all() -> Vec<(String, Vec<u8>)> {
    let root = fixtures();
    let mut out = Vec::new();
    for group in ["valid", "malformed", "hostile"] {
        let dir = root.join(group);
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".xml"))
            .collect();
        names.sort();
        for name in names {
            let path = dir.join(&name);
            let bytes = std::fs::read(&path).expect("read fixture");
            out.push((format!("{group}/{name}"), bytes));
        }
    }
    out
}

/// A plausible archive path for a fixture, so the corpus's own gates accept it.
fn archive_path(name: &str) -> String {
    format!("root/CTH 5_XML_HFR/{}", name.replace('/', "-"))
}

#[test]
fn the_manifest_and_the_directory_describe_the_same_fixtures() {
    let manifest = std::fs::read_to_string(fixtures().join("MANIFEST.md")).expect("manifest");
    let described: BTreeSet<String> = manifest
        .lines()
        .filter_map(|line| line.strip_prefix("### `")?.strip_suffix("`"))
        .map(str::to_string)
        .collect();
    let present: BTreeSet<String> = all().into_iter().map(|(name, _)| name).collect();

    let missing: Vec<_> = present.difference(&described).collect();
    let stale: Vec<_> = described.difference(&present).collect();
    assert!(
        missing.is_empty(),
        "fixtures with no manifest entry: {missing:?}"
    );
    assert!(
        stale.is_empty(),
        "manifest entries with no fixture: {stale:?}"
    );

    // The recorded size catches an accidental edit without this test carrying a
    // second digest implementation; `SHA256SUMS` is the real check and the
    // manifest says how to run it.
    for (name, bytes) in all() {
        let head = manifest
            .split(&format!("### `{name}`"))
            .nth(1)
            .expect("entry");
        let recorded: usize = head
            .lines()
            .find_map(|l| l.trim().strip_prefix("- bytes: "))
            .expect("a byte count")
            .parse()
            .expect("a number");
        assert_eq!(recorded, bytes.len(), "{name} changed size");
    }
}

#[test]
fn no_fixture_is_written_to_by_anything_this_program_does() {
    let before = all();
    for (name, bytes) in &before {
        // Everything the program does to a document, in order.
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(HEADER_READ_LIMIT)]);
        let path = archive_path(name);
        let _ = is_manuscript_xml(&path);
        let _ = looks_like_manuscript(&head);
        let _ = parse_manuscript(&path, &head);
        let mut out = Vec::new();
        normalize_into(bytes, &mut out);
        let _ = verify::compare(bytes, &out);
    }
    let after = all();
    assert_eq!(
        before.len(),
        after.len(),
        "the fixture directory gained or lost a file"
    );
    for ((name, before), (_, after)) in before.iter().zip(&after) {
        assert_eq!(before, after, "{name} was modified");
    }
}

#[test]
fn well_formed_fixtures_normalise_without_distortion() {
    for (name, bytes) in all() {
        if !name.starts_with("valid/") || name == "valid/declared-latin1.xml" {
            continue;
        }
        let mut out = Vec::new();
        normalize_into(&bytes, &mut out);
        let report = verify::compare(&bytes, &out)
            .unwrap_or_else(|why| panic!("{name} was distorted: {why}"));

        // Whatever was dropped is on the permit list, which is what `compare`
        // returning `Ok` already means; this checks the report says so too.
        for rule in &report.dropped {
            assert!(
                verify::is_dropped(rule.as_bytes()),
                "{name} reports dropping <?{rule}…?>, which is not on the permit list"
            );
        }
        assert!(
            out.starts_with(verify::DECLARATION),
            "{name} lost its canonical declaration"
        );
    }
}

/// The one permitted change that is not permitted after all.
///
/// Replacing the declaration is on the list. Replacing what it says about the
/// encoding is not: the body is copied byte for byte, so the bytes would stay
/// and their meaning would change — and the byte comparison would call that no
/// distortion, because by that measure it is none.
#[test]
fn a_declaration_that_would_change_the_encoding_is_refused() {
    let bytes = std::fs::read(fixtures().join("valid/declared-latin1.xml")).expect("fixture");
    assert!(
        bytes.contains(&0xE9),
        "the fixture is supposed to carry a byte that is not UTF-8"
    );
    let mut out = Vec::new();
    normalize_into(&bytes, &mut out);

    assert!(
        std::str::from_utf8(&out).is_err(),
        "the fixture is supposed to become invalid UTF-8 once relabelled"
    );
    let why = verify::compare(&bytes, &out).expect_err("this must not be allowed through");
    assert!(why.contains("ISO-8859-1"), "{why}");
    assert!(why.contains("UTF-8"), "{why}");
}

#[test]
fn an_undeclared_or_utf8_document_is_allowed_through() {
    for source in [
        &b"<AOxml><a/></AOxml>"[..],
        b"<?xml version=\"1.0\"?><AOxml><a/></AOxml>",
        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><AOxml><a/></AOxml>",
        b"<?xml version=\"1.0\" encoding=\"utf8\"?><AOxml><a/></AOxml>",
    ] {
        let mut out = Vec::new();
        normalize_into(source, &mut out);
        verify::compare(source, &out).unwrap_or_else(|why| {
            panic!(
                "{} was refused: {why}",
                String::from_utf8_lossy(&source[..source.len().min(50)])
            )
        });
    }
}

#[test]
fn documents_that_are_not_well_formed_are_read_without_panic_and_kept_verbatim() {
    for (name, bytes) in all() {
        if !name.starts_with("malformed/") {
            continue;
        }
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(HEADER_READ_LIMIT)]);
        let record = parse_manuscript(&archive_path(&name), &head);
        // The parser never refuses; it reports what it could not find.
        assert!(!record.sigla.is_empty(), "{name} produced no siglum at all");

        let mut out = Vec::new();
        normalize_into(&bytes, &mut out);
        // The body is not this program's to repair. Everything after the
        // prologue must survive exactly, malformed or not.
        let tail = |v: &[u8]| {
            let at = v
                .windows(9)
                .position(|w| w == b"<AOHeader")
                .unwrap_or_default();
            v[at..].to_vec()
        };
        if !bytes.is_empty() {
            assert_eq!(tail(&bytes), tail(&out), "{name} was repaired or damaged");
        }
    }
}

#[test]
fn the_fields_the_manifest_promises_are_the_fields_extracted() {
    // A path with no CTH in it, so the header is what answers.
    let neutral = |name: &str| format!("root/unfiled_XML_HFR/{}", name.replace('/', "-"));
    let cases: [(&str, &str, Option<&str>); 5] = [
        ("valid/minimal.xml", "KBo 1.1", None),
        ("valid/typical.xml", "KUB 2.1", Some("CTH 561")),
        ("valid/sections.xml", "KBo 3.5", Some("CTH 1")),
        ("valid/duplicate-ids.xml", "KBo 5.1", None),
        ("valid/deep.xml", "KBo 59.74", None),
    ];
    for (name, siglum, cth) in cases {
        let bytes = std::fs::read(fixtures().join(name)).expect("fixture");
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(HEADER_READ_LIMIT)]);
        let record = parse_manuscript(&neutral(name), &head);
        assert_eq!(record.sigla, siglum, "{name}");
        assert_eq!(record.cth.as_deref(), cth, "{name}");
    }
}

/// Which source of truth wins, when two of them disagree.
///
/// The archive folder does. It is how the corpus is filed, and it is what the
/// package's own folders are named after — a document that says one group while
/// sitting in another would otherwise be linked from a group it is not in.
/// Pinned here because the next stage reads this grouping to build bookmarks,
/// and a converter that resolves the tie the other way would produce a table of
/// contents that disagrees with the folders beside it.
#[test]
fn the_folder_decides_the_group_when_it_and_the_header_disagree() {
    let bytes = std::fs::read(fixtures().join("valid/typical.xml")).expect("fixture");
    let head = String::from_utf8_lossy(&bytes);
    assert!(
        head.contains("<CTHNr>CTH 561</CTHNr>"),
        "the fixture is supposed to name a group in its header"
    );
    let filed_elsewhere = parse_manuscript("root/CTH 5_XML_HFR/KUB 2.1.xml", &head);
    assert_eq!(filed_elsewhere.cth.as_deref(), Some("CTH 5"));
    let unfiled = parse_manuscript("root/unfiled_XML_HFR/KUB 2.1.xml", &head);
    assert_eq!(unfiled.cth.as_deref(), Some("CTH 561"));
}

#[test]
fn the_gates_refuse_an_empty_document_and_accept_the_rest() {
    for (name, bytes) in all() {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(HEADER_READ_LIMIT)]);
        let accepted = looks_like_manuscript(&head);
        // Only a document with nothing recognisable in its first 16 KiB is
        // refused. `not-utf8.xml` opens with bytes that are not UTF-8 and is
        // still accepted: the header is decoded lossily, which leaves the
        // markup after the junk readable, and refusing a document because its
        // first two bytes are wrong would lose a manuscript over a prefix.
        if name == "malformed/empty.xml" {
            assert!(!accepted, "{name} should not pass the content gate");
        } else {
            assert!(accepted, "{name} should pass the content gate");
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Второе мнение: две программы из состава macOS.
//
// До 10.09.2026 `xmllint` и `xsltproc` были названы в спецификации (§3.8) и в
// комментариях этого дерева, но ни один тест их не звал: XML-сторона будущей
// приемки PDF держалась на прозе. Ниже она держится прогоном.
//
// Оба теста пропускаются молча, когда программы нет: они входят в macOS, а
// конвейер собирается и на другой системе, и отсутствие внешней программы не
// должно превращаться в отказ сборки. Java здесь запрещена, сеть не нужна,
// зависимостей не прибавляется.

/// Есть ли программа в `PATH`.
fn have(tool: &str) -> bool {
    std::process::Command::new(tool)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Каноническая форма документа по `xmllint --c14n`, или `None`, если документ
/// разборщику не дался.
fn canonical(bytes: &[u8], at: &Path) -> Option<Vec<u8>> {
    std::fs::write(at, bytes).expect("write");
    let out = std::process::Command::new("xmllint")
        .arg("--c14n")
        .arg(at)
        .output()
        .expect("run xmllint");
    out.status.success().then_some(out.stdout)
}

/// Строки канонической формы без тех инструкций, которые нормализации
/// разрешено снимать.
///
/// C14N выбрасывает объявление XML и **сохраняет** инструкции обработки вне
/// корневого элемента, поэтому снятая ссылка на таблицу стилей видна в
/// канонической форме исходника и отсутствует в форме пакетной копии. Это не
/// расхождение деревьев, а ровно тот разрешенный список, который держит
/// `verify::compare`; чтобы структурный критерий говорил о структуре, список
/// снимается с обеих сторон одинаково.
fn without_dropped(canonical: &[u8]) -> Vec<u8> {
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
    // Перевод строки, оставшийся от снятой инструкции. C14N ставит каждую
    // инструкцию верхнего уровня на свою строку, и снятие самой инструкции
    // оставляет её перевод; пробел между инструкциями пролога разрешенный
    // список называет отдельным правилом (`REFLOW prologue whitespace`), так
    // что расхождением он быть не может.
    out.trim_start().as_bytes().to_vec()
}

/// **Структурный критерий рядом с текстовым: нормализация не меняет дерева.**
///
/// `verify::compare` доказывает, что тело документа побайтово то же. Это
/// сильное утверждение о байтах и **никакое** о структуре: там, где байты
/// сравнивают, дерево не проверяют, и урок §4.13 стоит именно об этом — знаки
/// не меняются от того, где закрыть элемент. Здесь то же самое спрашивает
/// сторонняя реализация, и спрашивает о дереве: канонические формы исходника и
/// пакетной копии обязаны совпасть, если снять с обеих разрешенные инструкции.
///
/// Это заготовка под приемку PDF (§8.3): два критерия, текстовый и
/// структурный, и второй здесь появляется впервые.
#[test]
fn the_package_copy_is_the_same_tree_as_the_source() {
    if !have("xmllint") {
        eprintln!("xmllint отсутствует — проверка пропущена");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("source.xml");
    let dst = dir.path().join("normalised.xml");

    let mut checked = 0usize;
    for (name, bytes) in all() {
        // Только корректные: у остальных канонической формы нет вовсе, и это
        // свойство исходных данных, а не нормализации.
        let Some(before) = canonical(&bytes, &src) else {
            continue;
        };
        let mut normalised = Vec::new();
        normalize_into(&bytes, &mut normalised);
        // Ровно тот путь, по которому документ попадает в пакет: то, что
        // `verify::compare` отвергает, туда не доходит вовсе. Единственный
        // такой образец здесь — документ, объявивший latin-1: канонической
        // формы у нормализованной копии нет и быть не должно, потому что
        // копия эта никогда не пишется.
        if verify::compare(&bytes, &normalised).is_err() {
            continue;
        }
        let after = canonical(&normalised, &dst)
            .unwrap_or_else(|| panic!("{name}: нормализованный документ перестал разбираться"));

        assert_eq!(
            without_dropped(&before),
            without_dropped(&after),
            "{name}: дерево изменилось, хотя байты тела совпали"
        );
        checked += 1;
    }
    assert!(
        checked >= 15,
        "проверено всего {checked} образцов — сторонний разборщик отверг больше, чем должен"
    );
}

/// **Поля манифеста, извлеченные не этим крейтом.**
///
/// Крейт читает семь полей из первых 16 КиБ собственным сканером. Проверять
/// его собственным же чтением — значит мерить проект против себя самого;
/// §8.1 спецификации на этот случай называет `xsltproc`, извлекающий те же
/// поля из корректного исходника по XPath. Здесь берется `docID` — то, из чего
/// получается сиглум, то есть имя файла в пакете и ключ описи.
///
/// Вторая заготовка под приемку PDF: способ сверить извлечение с манифестом,
/// не спрашивая извлекатель о нем самом.
#[test]
fn the_siglum_is_what_an_independent_extractor_reads() {
    if !have("xsltproc") {
        eprintln!("xsltproc отсутствует — проверка пропущена");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let sheet = dir.path().join("docid.xsl");
    std::fs::write(
        &sheet,
        r#"<?xml version="1.0"?>
<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
  <xsl:output method="text"/>
  <xsl:template match="/"><xsl:value-of select="normalize-space((//docID)[1])"/></xsl:template>
</xsl:stylesheet>
"#,
    )
    .expect("write");

    let doc = dir.path().join("doc.xml");
    let mut checked = 0usize;
    for (name, bytes) in all() {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(HEADER_READ_LIMIT)]);
        if !looks_like_manuscript(&head) {
            continue;
        }
        let record = parse_manuscript(&archive_path(&name), &head);
        std::fs::write(&doc, &bytes).expect("write");
        let out = std::process::Command::new("xsltproc")
            .arg(&sheet)
            .arg(&doc)
            .output()
            .expect("run xsltproc");
        if !out.status.success() {
            // Некорректный XML: у стороннего разборщика мнения нет, и это не
            // расхождение — это те самые 206 документов.
            continue;
        }
        let theirs = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if theirs.is_empty() {
            continue;
        }
        // **Ссылки на сущности этот крейт не раскрывает, и это записанное
        // решение, а не расхождение.** Раскрытие внешних сущностей — дыра в
        // безопасности, внутренних — решение, которое `docs/XML-CONTRACT.md`
        // §5 держит открытым: сегодня таких документов в корпусе ноль.
        // `xsltproc` раскрывает и то и другое, поэтому на таком образце два
        // ответа расходятся законно. Пропускается по признаку самого ответа,
        // а не по имени файла: имя ничего не доказывает, а `&…;` в поле —
        // ровно тот случай, о котором идет речь.
        if record.sigla.starts_with('&') && record.sigla.ends_with(';') {
            continue;
        }
        assert_eq!(
            record.sigla, theirs,
            "{name}: сиглум крейта и docID стороннего извлекателя разошлись"
        );
        checked += 1;
    }
    assert!(
        checked >= 10,
        "сверено всего {checked} образцов — выборка перестала быть представительной"
    );
}
