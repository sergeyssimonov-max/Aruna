use crate::commands::said;
use crate::dto::{CorpusStats, Fonts, GroupSize, Spread, StatsSource};
use crate::read::{read_stats, StatsError};
use std::fs;
use std::path::Path;

/// Пакет из `groups` каталогов по `each` документов в каждом.
///
/// Рядом с документами кладется файл не-XML: обход обязан считать рукописи,
/// а не содержимое каталога.
fn package(root: &Path, groups: usize, each: usize) {
    for group in 0..groups {
        let dir = root.join(format!("CTH {group}"));
        fs::create_dir_all(&dir).unwrap();
        for document in 0..each {
            fs::write(dir.join(format!("KBo {document}.xml")), b"<doc/>").unwrap();
        }
        fs::write(dir.join("README.txt"), b"not a manuscript").unwrap();
    }
}

/// **Манифест отвечает первым.**
///
/// Числа в нем нарочно не сходятся с тем, что лежит на диске – иначе тест
/// не отличил бы прочитанный манифест от совпавшего с ним обхода.
#[test]
fn the_counts_come_from_the_manifest_when_it_has_them() {
    let dir = tempfile::tempdir().unwrap();
    package(dir.path(), 2, 3);
    fs::write(
        dir.path().join(aruna::export::MANIFEST),
        br#"{"schema":1,"counts":{"documents":23936,"groups":663}}"#,
    )
    .unwrap();

    let stats = read_stats(dir.path()).unwrap();

    assert_eq!(
        stats,
        CorpusStats {
            manuscripts: 23936,
            groups: 663,
            source: StatsSource::Manifest,
            // Манифест без списка групп: два итога он назвал, а разбивку
            // строить не из чего – и она пуста, а не выдумана.
            spread: Spread::default(),
            fonts: None,
        }
    );
}

/// **Без манифеста числа берутся обходом, и обход делает ядро.**
///
/// Что именно считается группой и что рукописью, проверено там, где это
/// написано – `aruna::export::counts`. Здесь проверяется работа обертки:
/// без манифеста она уходит к ядру, отвечает его числами и говорит, откуда
/// они взяты.
#[test]
fn a_package_without_a_manifest_is_counted_by_walking() {
    let dir = tempfile::tempdir().unwrap();
    package(dir.path(), 4, 5);

    let stats = read_stats(dir.path()).unwrap();

    assert_eq!(stats.manuscripts, 20);
    assert_eq!(stats.groups, 4);
    assert_eq!(stats.source, StatsSource::Walk);
    assert_eq!(
        stats
            .spread
            .largest
            .expect("в пакете есть группы")
            .fragments,
        5,
        "разбивка обхода доехала до окна"
    );
    assert_eq!(
        stats.fonts, None,
        "обход не читает документы и не может знать, как написан их текст"
    );
}

/// **Манифест без `counts` – то же самое, что манифест без манифеста.**
///
/// Задание требует подсчета, когда нужных полей нет, и испорченный JSON
/// ведет туда же: для окна оба случая – это «числа надо взять с диска».
#[test]
fn a_manifest_that_does_not_carry_the_counts_falls_through_to_the_walk() {
    let dir = tempfile::tempdir().unwrap();
    package(dir.path(), 3, 2);

    for text in [&br#"{"schema":1}"#[..], b"{ not json at all"] {
        fs::write(dir.path().join(aruna::export::MANIFEST), text).unwrap();
        let stats = read_stats(dir.path()).unwrap();
        assert_eq!(stats.source, StatsSource::Walk);
        assert_eq!(stats.manuscripts, 6);
        assert_eq!(stats.groups, 3);
    }
}

/// **Пакета нет – это названная ошибка, а не ноль.**
///
/// Ноль рукописей – это утверждение о пакете, и окно показало бы его как
/// число. Отсутствие пакета утверждением о его содержимом не является.
#[test]
fn a_path_that_is_not_a_package_is_a_named_failure() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nothing-here");

    let failure = read_stats(&missing).unwrap_err();

    assert!(matches!(failure, StatsError::Missing));
    let message = said(&failure);
    assert_eq!(message, "пакет по этому пути не найден");
    assert!(
        !message.contains("nothing-here"),
        "в сообщении об ошибке оказался путь: {message}"
    );
}

/// **Разбивка берется из перечня групп, который манифест уже несет.**
///
/// Числа нарочно не сходятся с тем, что лежит на диске – на диске в этом
/// тесте нет ничего, кроме манифеста – иначе проверка не отличила бы
/// прочитанный перечень от совпавшего с ним обхода.
///
/// Заодно проверяются два правила суммы аномалий: складываются все
/// счетчики, включая тот, которого эта программа не знает по имени, и
/// нулевые в сумму ничего не вносят.
#[test]
fn the_breakdown_comes_from_the_manifests_own_list_of_groups() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = format!(
        r#"{{
          "schema": 1,
          "counts": {{ "documents": 9, "groups": 3 }},
          "groups": [
            {{ "label": "CTH 832", "documents": [{{}}, {{}}, {{}}, {{}}, {{}}, {{}}] }},
            {{ "label": "CTH 1", "documents": [{{}}] }},
            {{ "label": "{missing}", "documents": [{{}}, {{}}] }}
          ],
          "fonts": {{
            "documents_examined": 9,
            "documents_not_in_nfc": 2,
            "documents_with_private_use": 4,
            "private_use_points": ["U+E83A", "U+100000"],
            "anomalies": {{ "unusual_space": 1, "zero_width": 0, "not_yet_invented": 5 }}
          }}
        }}"#,
        missing = aruna::parse::MISSING,
    );
    fs::write(dir.path().join(aruna::export::MANIFEST), manifest).unwrap();

    let stats = read_stats(dir.path()).unwrap();

    assert_eq!(stats.source, StatsSource::Manifest);
    assert_eq!(
        stats.spread,
        Spread {
            largest: Some(GroupSize {
                label: "CTH 832".to_owned(),
                fragments: 6,
            }),
            singletons: 1,
            without_cth: 2,
        }
    );
    assert_eq!(
        stats.fonts,
        Some(Fonts {
            not_in_nfc: 2,
            with_private_use: 4,
            private_use_points: 2,
            anomalies: 6,
        })
    );
}

/// **Форма, которую объявляет окно, записана в файле и сверяется с ним.**
///
/// С подключением specta 02.09.2026 второго ручного объявления типов не
/// стало: они порождаются в `frontend/src/bindings.ts`, и свежесть держит
/// `the_bindings_are_what_these_commands_produce`. Этот тест остался при
/// своем – он про сериализацию, а не про форму типа: здесь проверяется, что
/// Rust пишет именно то, что записано в `src-tauri/stats-sample.json`, а
/// `frontend/tests/ipc-shape.test.ts` сверяет с тем же файлом образец
/// `STATS_SAMPLE`. Порожденный тип ни того, ни другого не обещает.
///
/// Второй образец – про пустые места: он закрепляет, что отсутствие
/// группы и отсутствие манифеста уходят в окно как `null`, а не как
/// пропущенные поля.
#[test]
fn the_wire_shape_is_the_one_the_window_declares() {
    let sample = serde_json::json!({
        "manifest": CorpusStats {
            manuscripts: 23936,
            groups: 663,
            source: StatsSource::Manifest,
            spread: Spread {
                largest: Some(GroupSize {
                    label: "CTH 832".to_owned(),
                    fragments: 4480,
                }),
                singletons: 116,
                without_cth: 0,
            },
            fonts: Some(Fonts {
                not_in_nfc: 78,
                with_private_use: 1269,
                private_use_points: 7,
                anomalies: 0,
            }),
        },
        "walk": CorpusStats {
            manuscripts: 0,
            groups: 0,
            source: StatsSource::Walk,
            spread: Spread::default(),
            fonts: None,
        },
    });

    let recorded: serde_json::Value =
        serde_json::from_str(include_str!("../../stats-sample.json")).unwrap();

    assert_eq!(
        sample, recorded,
        "форма данных разошлась со stats-sample.json – окно объявляет старую"
    );
}

/// **Пустой пакет не называет самой большой группы.**
///
/// Ноль фрагментов в несуществующей группе – это утверждение, которого
/// делать не о чем: `None` здесь означает «групп нет», и окно на это
/// отвечает молчанием, а не строкой с нулем.
#[test]
fn an_empty_package_names_no_largest_group() {
    let dir = tempfile::tempdir().unwrap();

    let stats = read_stats(dir.path()).unwrap();

    assert_eq!(stats.groups, 0);
    assert_eq!(stats.spread, Spread::default());
}
