use super::read::XmlSummaryError;
use crate::read::read_xml_summary;
use std::fs;

const SECTION: &str = r#"{"schema":1,"counts":{"documents":4,"groups":1},
  "xml":{"documents":4,"well_formed":2,"not_well_formed":2,
    "by_reason":{"crossing-elements":1,"element-never-closed":1,"unclassified":0},
    "not_well_formed_documents":[
      {"file":"CTH 1/KBo 1.1.xml","reason":"element-never-closed","line":7,"column":12},
      {"file":"CTH 1/KBo 1.2.xml","reason":"crossing-elements","line":3,"column":4}],
    "beyond_this_parser":{"documents":1,
      "by_limit":{"colon-without-local-name":0,"raw-less-than-in-attribute-value":1},
      "documents_beyond_this_parser":[
        {"file":"CTH 1/KBo 1.3.xml","limit":"raw-less-than-in-attribute-value",
         "line":5,"column":41}]},
    "totals":{
      "refused_here":{"documents":2,"means":"…"},
      "not_well_formed_xml":{"documents":3,"means":"…"},
      "objected_to_by_a_conforming_parser":{"documents":3,"means":"…"}}}}"#;

/// Манифест пакета, собранного до 10.09.2026: секция `xml` есть, второй
/// половины в ней нет.
const SECTION_WITHOUT_LIMITS: &str = r#"{"schema":1,"counts":{"documents":4,"groups":1},
  "xml":{"documents":4,"well_formed":2,"not_well_formed":2,
    "by_reason":{"crossing-elements":1,"element-never-closed":1,"unclassified":0},
    "not_well_formed_documents":[
      {"file":"CTH 1/KBo 1.1.xml","reason":"element-never-closed","line":7,"column":12},
      {"file":"CTH 1/KBo 1.2.xml","reason":"crossing-elements","line":3,"column":4}]}}"#;

/// **Сводка читается из манифеста и ничего не пересчитывает.**
///
/// Каталог при этом пуст: если бы команда считала обходом, она вернула бы
/// нули, и тест отличает одно от другого.
#[test]
fn the_summary_is_read_from_the_manifest_and_nothing_is_walked() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(aruna::export::MANIFEST), SECTION).unwrap();

    let summary = read_xml_summary(dir.path()).expect("манифест несет секцию");

    assert_eq!(summary.documents, 4);
    assert_eq!(summary.read, 2);
    assert_eq!(summary.unread, 2);
    assert_eq!(
        summary.documents_not_well_formed.len(),
        2,
        "имена некорректных доезжают до окна"
    );
    assert_eq!(
        summary.documents_not_well_formed[0].file,
        "CTH 1/KBo 1.1.xml"
    );
    assert_eq!(summary.documents_not_well_formed[0].line, 7);
    // Причина с нулем не выбрасывается: пустая строка на экране и
    // отсутствие строки значат разное.
    assert!(summary
        .reasons
        .iter()
        .any(|r| r.reason == "unclassified" && r.documents == 0));
}

/// **Вторая половина доезжает до окна так же, как первая.**
///
/// Документы, которые разборщик ядра принял, а строгий – нет: их число, их
/// классы (включая класс с нулем – по той же причине, что и причина с
/// нулем) и их имена со строками. Два итога, 210 и 223, окно берет из
/// манифеста, а не складывает само: слагаемые знает ядро.
#[test]
fn the_documents_beyond_the_parser_reach_the_window_too() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(aruna::export::MANIFEST), SECTION).unwrap();

    let summary = read_xml_summary(dir.path()).expect("манифест несет секцию");

    assert_eq!(summary.beyond_this_parser, 1);
    assert_eq!(summary.not_well_formed_xml, 3, "отказы плюс голый «<»");
    assert_eq!(summary.objected_to, 3);
    assert_eq!(summary.documents_beyond_this_parser.len(), 1);
    assert_eq!(
        summary.documents_beyond_this_parser[0].file,
        "CTH 1/KBo 1.3.xml"
    );
    assert_eq!(summary.documents_beyond_this_parser[0].line, 5);
    assert!(summary
        .limits
        .iter()
        .any(|l| l.limit == "colon-without-local-name" && l.documents == 0));

    let summed: u32 = summary.limits.iter().map(|l| l.documents).sum();
    assert_eq!(summed, summary.beyond_this_parser);
}

/// **Пакет, собранный между 06.09 и 10.09.2026, читается, а не отвергается.**
///
/// Отсутствие второй половины – не то же самое, что отсутствие секции: про
/// первую половину такой манифест знает все. Окно получает нули там, где
/// сведений нет, и ничего про них не печатает.
#[test]
fn a_manifest_from_before_the_limits_were_counted_is_still_read() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(aruna::export::MANIFEST),
        SECTION_WITHOUT_LIMITS,
    )
    .unwrap();

    let summary = read_xml_summary(dir.path()).expect("первая половина на месте");

    assert_eq!(summary.unread, 2, "она читается как раньше");
    assert_eq!(summary.beyond_this_parser, 0);
    assert!(summary.limits.is_empty());
    assert!(summary.documents_beyond_this_parser.is_empty());
    assert_eq!(
        summary.not_well_formed_xml, 0,
        "числа, которого манифест не несет, окно не выдумывает"
    );
    assert_eq!(summary.objected_to, 0);
}

/// **Разбивка сходится с итогом.**
///
/// То же равенство, что держит тест ядра, но проверенное на той стороне
/// провода: окно показывает сумму по причинам рядом с общим числом, и
/// разойтись им нельзя.
#[test]
fn the_breakdown_adds_up_to_the_total() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(aruna::export::MANIFEST), SECTION).unwrap();

    let summary = read_xml_summary(dir.path()).unwrap();

    let summed: u32 = summary.reasons.iter().map(|r| r.documents).sum();
    assert_eq!(summed, summary.unread);
    assert_eq!(summary.read + summary.unread, summary.documents);
}

/// **Пакет, собранный до 06.09.2026, получает отказ, а не нули.**
///
/// Манифест без секции – это не пакет без некорректных документов, и
/// показать ноль было бы неправдой о корпусе.
#[test]
fn a_manifest_without_the_section_is_refused_rather_than_read_as_zero() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(aruna::export::MANIFEST),
        br#"{"schema":1,"counts":{"documents":23936,"groups":663}}"#,
    )
    .unwrap();

    assert!(matches!(
        read_xml_summary(dir.path()),
        Err(XmlSummaryError::Absent)
    ));
}

/// Пакета нет и манифест не разбирается – два разных отказа.
#[test]
fn a_missing_package_and_a_broken_manifest_are_told_apart() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        read_xml_summary(&dir.path().join("нет-такого")),
        Err(XmlSummaryError::Missing)
    ));

    fs::write(dir.path().join(aruna::export::MANIFEST), "{ не json").unwrap();
    assert!(matches!(
        read_xml_summary(dir.path()),
        Err(XmlSummaryError::Unreadable)
    ));
}

/// Имена документов из манифеста, в порядке манифеста: сначала те, что
/// разборщик не прочитал, затем те, что он прочитал, а стандарт не допускает.
fn names_in(manifest: &serde_json::Value) -> Vec<String> {
    let xml = &manifest["xml"];
    let unread = xml["not_well_formed_documents"].as_array().unwrap();
    let beyond = xml["beyond_this_parser"]["documents_beyond_this_parser"]
        .as_array()
        .unwrap();
    unread
        .iter()
        .chain(beyond)
        .map(|entry| entry["file"].as_str().unwrap().to_owned())
        .collect()
}

/// Имена, как они уходят в окно: сводка, сериализованная тем же serde,
/// что несет ее по проводу.
fn names_on_the_wire(package: &std::path::Path) -> Vec<String> {
    let wire = serde_json::to_value(read_xml_summary(package).unwrap()).unwrap();
    ["documents_not_well_formed", "documents_beyond_this_parser"]
        .iter()
        .flat_map(|key| wire[*key].as_array().unwrap().clone())
        .map(|entry| entry["file"].as_str().unwrap().to_owned())
        .collect()
}

/// Ссылки описи – те же имена, что у манифеста, байт в байт.
///
/// У каждого документа манифеста в описи одна ссылка, на имени, в порядке
/// манифеста (решение владельца 19.10.2026). У документа с построенным PDF
/// она ведет на PDF: раскодированная дает побайтно путь `pdf` манифеста, а
/// без «.pdf» совпадает побайтно с `href` того же документа без «.xml» – то
/// же экранирование. У документа, чей PDF отказан, ссылка – его `href`,
/// раскодированная дает побайтно его `file`. Других ссылок нет. У `refused`
/// (имен, которые модель не читает) PDF нет, и ссылка у них – на XML.
fn every_link_carries_its_name(
    package: &std::path::Path,
    manifest: &serde_json::Value,
    refused: &[String],
) {
    use std::os::unix::ffi::OsStrExt as _;
    let name = package.file_name().unwrap().to_str().unwrap();
    assert!(aruna::editions::is_package_name(name), "имя пакета: {name}");
    let html = fs::read_to_string(package.join(format!("{name}.html"))).unwrap();
    let links = aruna::export::hrefs(&html);
    let mut wanted: Vec<String> = Vec::new();
    for group in manifest["groups"].as_array().unwrap() {
        for document in group["documents"].as_array().unwrap() {
            let file = document["file"].as_str().unwrap();
            let xml_href = document["href"].as_str().unwrap();
            let (link, path) = match document["pdf"].as_str() {
                Some(pdf) => {
                    let stem = xml_href.strip_suffix(".xml").unwrap();
                    (format!("{stem}.pdf"), pdf)
                }
                None => (xml_href.to_owned(), file),
            };
            let target = aruna::export::resolve(&link).unwrap();
            assert_eq!(
                target.as_os_str().as_bytes(),
                path.as_bytes(),
                "ссылка {link:?} раскодируется не в путь манифеста"
            );
            assert!(package.join(&target).is_file(), "в пакете нет {path:?}");
            wanted.push(link);
        }
    }
    assert_eq!(
        links.len(),
        wanted.len(),
        "ссылок в описи не по одной на документ"
    );
    for (at, (link, want)) in links.iter().zip(&wanted).enumerate() {
        assert_eq!(
            *link,
            want.as_str(),
            "ссылка {at} описи не та, что у документа манифеста"
        );
    }
    for name in refused {
        let pdf = std::path::Path::new(name).with_extension("pdf");
        assert!(!package.join(&pdf).exists(), "у {name:?} есть PDF");
        assert!(
            links
                .iter()
                .any(|h| aruna::export::resolve(h).as_deref() == Some(std::path::Path::new(name))),
            "опись не ссылается на XML {name:?}"
        );
    }
}

/// Все проверки провода на пакете `package`, где `decomposed` – имя
/// документа, записанное в NFD и названное манифестом.
///
/// Каждое имя из манифеста доходит до провода байт в байт и называет файл
/// пакета. Отрицательных контролей два, потому что APFS ищет файл без учета
/// формы нормализации и под именем в NFC найдет файл, записанный в NFD:
/// копия манифеста с тем же именем в NFC дает на проводе именно эти байты –
/// провод не нормализует, но проверка по диску эту подмену не различит;
/// копия с именем, которого в пакете нет, ловится по диску.
fn wire_carries_every_name(package: &std::path::Path, decomposed: &str) {
    let text = fs::read_to_string(package.join(aruna::export::MANIFEST)).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&text).unwrap();

    let expected = names_in(&manifest);
    let arrived = names_on_the_wire(package);
    assert_eq!(
        arrived, expected,
        "имена на проводе расходятся с манифестом"
    );
    for name in &arrived {
        assert!(package.join(name).is_file(), "в пакете нет {name:?}");
    }
    every_link_carries_its_name(package, &manifest, &arrived);
    assert!(
        arrived
            .iter()
            .any(|name| name.as_bytes() == decomposed.as_bytes()),
        "имя в NFD не дошло до провода тем же UTF-8"
    );

    let swapped_to = |to: &str| {
        let copy = tempfile::tempdir().unwrap();
        fs::write(
            copy.path().join(aruna::export::MANIFEST),
            text.replace(decomposed, to),
        )
        .unwrap();
        names_on_the_wire(copy.path())
    };

    let composed = decomposed.replace("C\u{327}", "\u{c7}");
    let recomposed = swapped_to(&composed);
    assert_ne!(recomposed, expected, "подмена формы не видна на проводе");
    assert!(recomposed
        .iter()
        .any(|name| name.as_bytes() == composed.as_bytes()));
    assert!(!recomposed
        .iter()
        .any(|name| name.as_bytes() == decomposed.as_bytes()));

    let elsewhere = decomposed.replace(".xml", " (elsewhere).xml");
    let renamed = swapped_to(&elsewhere);
    let missing: Vec<_> = renamed
        .iter()
        .filter(|name| !package.join(name).is_file())
        .collect();
    assert_eq!(missing, [&elsewhere], "подмененное имя не поймано");
}

/// **Каждое имя доходит до окна байт в байт – в обычном наборе.**
///
/// Синтетический манифест выше доказывает форму; этот – провод на пакете,
/// который собрало ядро: архив из трех документов, один из которых
/// разборщик не читает и назван в NFD, а другой читает, хотя стандарт его
/// не допускает. `source.not_manuscripts` на провод не идет вовсе: сводка
/// читает из манифеста только секцию `xml`.
#[test]
fn every_name_the_manifest_gives_reaches_the_window_byte_for_byte() {
    use std::io::Write as _;
    let dir = tempfile::tempdir().unwrap();
    let zip_path = dir.path().join("small.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let document = |siglum: &str, body: &str| {
        format!(
            r#"<AOxml xml:space="preserve"><AOHeader><docID>{siglum}</docID><meta><uebern editor="FB" date="2017-03-28"/></meta></AOHeader><body><text>{body}</text></body></AOxml>"#
        )
    };
    for (entry, siglum, body) in [
        ("fine.xml", "KBo 1.1", "<l lg=\"Hit\"/>text"),
        (
            "unread.xml",
            "C\u{327}orum 1",
            "<l lg=\"Hit\"><w>open</text>",
        ),
        ("beyond.xml", "KBo 1.3", "<w a=\"x<y\">z</w>"),
    ] {
        zip.start_file(format!("root/CTH 5_XML_HFR/{entry}"), options)
            .unwrap();
        zip.write_all(document(siglum, body).as_bytes()).unwrap();
    }
    zip.finish().unwrap();

    let out = tempfile::tempdir().unwrap();
    aruna::export::build(
        &zip_path,
        out.path(),
        "test",
        &aruna::app::load_fonts(Some(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../cli/resources/fonts"),
        ))
        .unwrap(),
        &aruna::job::Job::unattended(),
    )
    .unwrap();
    let package = out.path().join(aruna::export::PACKAGE);

    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(package.join(aruna::export::MANIFEST)).unwrap())
            .unwrap();
    assert_eq!(names_in(&manifest).len(), 2, "в пакете не те два документа");
    wire_carries_every_name(&package, "CTH 5/C\u{327}orum 1.xml");
}

/// **На настоящем пакете – те же проверки, на 223 именах.**
///
/// Пакет, названный `ARUNA_PACKAGE`, или собранный здесь же из архива
/// корпуса – тем же путем, каким тяжелые тесты берут фикстуру.
#[test]
#[ignore = "собирает пакет из архива корпуса или читает ARUNA_PACKAGE; запускать явно"]
fn every_name_of_a_real_package_reaches_the_window_byte_for_byte() {
    let built;
    let package = match std::env::var_os("ARUNA_PACKAGE") {
        Some(named) => std::path::PathBuf::from(named),
        None => {
            let Some(zip) = super::corpus_fixture() else {
                eprintln!("пропуск: ни ARUNA_PACKAGE, ни архива корпуса");
                return;
            };
            built = tempfile::tempdir().unwrap();
            let (_, package) = aruna::export::build_local_with_path(
                &zip,
                built.path(),
                &aruna::app::load_fonts(Some(
                    &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../cli/resources/fonts"),
                ))
                .unwrap(),
                &aruna::job::Job::unattended(),
            )
            .unwrap();
            assert_eq!(package, built.path().join(aruna::export::LEGACY_PACKAGE));
            package
        }
    };
    wire_carries_every_name(&package, "CTH 565/C\u{327}orum 6-1-96.xml");
}
