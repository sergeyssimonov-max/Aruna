use crate::build::{chosen_destination, in_slot, Building};
use crate::commands::contract;
use crate::dto::{BuildFailure, BuildProgress, Stage};

/// Порожденные типы, как они лежат в дереве.
fn committed() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../frontend/src/bindings.ts")
}

/// Порожденные типы, как их производит нынешнее объявление команд.
fn exported() -> String {
    let dir = tempfile::tempdir().expect("временный каталог");
    let file = dir.path().join("bindings.ts");
    contract()
        .export(specta_typescript::Typescript::default(), &file)
        .expect("экспорт типов");
    std::fs::read_to_string(&file).expect("прочитать порожденное")
}

/// **Закоммиченные типы – это то, что производят эти команды.**
///
/// Та же дисциплина, что у артефактов описи в `cli/src/generated/`: продукт
/// лежит в дереве, а тест пересобирает его и падает, если байт разошелся.
/// Порождать при старте приложения нельзя – отладочный запуск и прогон E2E
/// писали бы в рабочее дерево.
///
/// Обновить: `cargo test -p aruna-desktop -- --ignored regenerate`.
#[test]
fn the_bindings_are_what_these_commands_produce() {
    let file = committed();
    let on_disk = std::fs::read_to_string(&file).unwrap_or_default();
    assert_eq!(
        on_disk,
        exported(),
        "frontend/src/bindings.ts разошелся с объявлением команд; \
         обновить: cargo test -p aruna-desktop -- --ignored regenerate"
    );
}

/// Не проверка, а способ обновить продукт выше.
#[test]
#[ignore = "пишет frontend/src/bindings.ts; запускается, когда договор изменился"]
fn regenerate_the_bindings() {
    contract()
        .export(specta_typescript::Typescript::default(), committed())
        .expect("записать порожденное");
}

/// **Ни одно событие прогресса не несет пути файловой системы.**
///
/// То же правило, по которому живет `app::Failure` ядра
/// (`a_failure_never_carries_a_filesystem_path`), и та же причина: окно
/// показывают через плечо. Пять вариантов `progress::Event` носят `&Path`.
/// Четыре из них проверяются здесь – по имени, а не по представителю; пятый,
/// `DownloadRetrying`, несет путь не полем, а текстом причины, и ему
/// отведена своя проверка ниже (`a_retry_says_why_without_saying_where`).
#[test]
fn a_progress_event_never_carries_a_filesystem_path() {
    use aruna::progress::Event as Core;

    let secret = std::path::PathBuf::from("/Users/someone/Secrets/aruna");
    let carriers = [
        Core::CacheUnusable { dir: &secret },
        Core::ArchiveFromCache { path: &secret },
        Core::ArchiveKept { path: &secret },
        Core::PreviousPackageLeft { path: &secret },
    ];

    for event in carriers {
        let wire = serde_json::to_string(&BuildProgress::of(1, &event)).expect("сериализуется");
        assert!(
            !wire.contains("Secrets"),
            "в событии прогресса оказался путь: {wire}"
        );
    }
}

/// Ошибка, которую ядро отдает вместе с путем, доходит до окна без него.
///
/// Пятый носитель пути – `DownloadRetrying`, и он единственный, чей текст
/// до окна доходит: сообщение берется у `app::Failure`, где путь уже убран.
#[test]
fn a_retry_says_why_without_saying_where() {
    use aruna::progress::Event as Core;

    let error = aruna::error::ArunaError::Io {
        path: std::path::PathBuf::from("/Users/someone/Secrets/aruna.zip"),
        source: std::io::Error::other("диск отвалился"),
    };
    let event = Core::DownloadRetrying {
        attempt: 2,
        delay: std::time::Duration::from_secs(4),
        error: &error,
    };

    let progress = BuildProgress::of(7, &event);
    let note = progress.note.expect("повтор объясняет себя");
    assert!(!note.contains("Secrets"), "в тексте повтора оказался путь");
    assert!(!note.is_empty());
}

/// Стадия объявляет знаменатель, тик заполняет числитель.
///
/// Обе половины дроби приходят из ядра как есть; окно ничего не считает
/// само, и полоса не может показать долю, знаменатель которой разошелся с
/// объявленным.
#[test]
fn the_stage_names_the_whole_and_the_tick_fills_it_in() {
    use aruna::progress::Event as Core;

    let announced = BuildProgress::of(1, &Core::WritingDocuments { documents: 23_936 });
    assert_eq!(announced.stage, Stage::Writing);
    assert_eq!((announced.done, announced.total), (Some(0), Some(23_936)));

    let tick = BuildProgress::of(
        1,
        &Core::DocumentsWritten {
            done: 500,
            total: 23_936,
        },
    );
    assert_eq!(tick.stage, Stage::Writing);
    assert_eq!((tick.done, tick.total), (Some(500), Some(23_936)));

    // У загрузки знаменателя может не быть вовсе, и тогда его нет.
    let unknown = BuildProgress::of(
        1,
        &Core::Downloading {
            bytes: 4096,
            total: None,
        },
    );
    assert_eq!((unknown.done, unknown.total), (Some(4096), None));
}

/// **Одна сборка за раз, и вторая получает не панику, а отказ.**
///
/// Две сборки в одном каталоге назначения – это два экспорта, спорящих за
/// одну публикацию; ядро в этом случае отвечает `publish_busy`, но узнать
/// об этом через минуту загрузки было бы поздно. Окно узнает сразу.
#[test]
fn a_second_build_is_refused_while_the_first_is_running() {
    let building = Building::default();

    let first = building
        .claim(aruna::job::Cancel::new())
        .expect("место свободно");
    let refused = building
        .claim(aruna::job::Cancel::new())
        .expect_err("вторая сборка не начинается");

    assert_eq!(refused.code, "busy");
    assert_eq!(refused.message, "сборка уже идет");
    assert!(refused.retryable, "повторить можно – после первой");
    assert!(!refused.cancelled);

    drop(first);
    let _next = building
        .claim(aruna::job::Cancel::new())
        .expect("после прогона место снова свободно");
}

/// Отмена доходит до флага, который держит идущая сборка.
///
/// Тот самый случай, ради которого флаг лежит в состоянии приложения:
/// `cancel_build` – второй вызов, приходящий, пока кадр первого еще не
/// вернулся.
#[test]
fn a_stop_reaches_the_flag_the_running_build_holds() {
    let building = Building::default();
    let cancel = aruna::job::Cancel::new();
    let _held = building.claim(cancel.clone()).expect("место свободно");

    assert!(!cancel.is_cancelled());
    building.stop();
    assert!(cancel.is_cancelled(), "отмена не дошла до прогона");
}

/// Нажатие по сборке, которой нет – не ошибка.
#[test]
fn a_stop_with_nothing_running_says_nothing() {
    Building::default().stop();
}

/// **Паника между занятием места и его освобождением места не держит.**
///
/// Находка № 6 refactor-1. Команда освобождала место строкой после
/// `.await`, и паника потока сборки до нее доходила – tokio отдает ее
/// ошибкой `JoinHandle`. Но держала это строка, а не устройство: любой
/// вызывающий, у которого между `claim` и `release` случилась паника,
/// оставлял окно «занятым» до перезапуска.
#[test]
fn a_panic_while_building_leaves_the_slot_free() {
    let building = Building::default();
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _held = building
            .claim(aruna::job::Cancel::new())
            .expect("место свободно");
        panic!("сборка упала, не дойдя до освобождения");
    }));
    assert!(unwound.is_err());

    let _next = building
        .claim(aruna::job::Cancel::new())
        .expect("после паники место снова свободно");
}

/// **Паника внутри сборки доходит до окна существующим отказом, а место
/// освобождается.**
///
/// Тот же шов, что у команды: `in_slot` занимает место, отдает работу в
/// поток блокирующих задач и отображает ее исход. Окно получает
/// `interrupted` – код, который уже был у оборвавшейся сборки – следующая
/// сборка начинается без перезапуска, а «Остановить» отвечает как при
/// свободном месте.
#[test]
fn a_panic_inside_the_build_reaches_the_window_as_interrupted() {
    let building = Building::default();
    let refused =
        tauri::async_runtime::block_on(in_slot(&building, aruna::job::Cancel::new(), || {
            panic!("паника внутри сборки")
        }))
        .expect_err("сборка с паникой не может кончиться успехом");

    assert_eq!(refused.code, "interrupted");
    assert_eq!(refused.message, "сборка оборвалась, не сказав почему");
    assert!(refused.retryable, "повторить можно");
    assert!(!refused.cancelled, "паника – не отмена");

    building.stop();
    let _next = building
        .claim(aruna::job::Cancel::new())
        .expect("после паники место снова свободно");
}

/// **Окно называет, куда класть пакет, и не называет, откуда его брать.**
///
/// Две оси, и они разошлись 06.09.2026 решением владельца. Источник один –
/// закрепленная запись Zenodo: подать архив команде неоткуда, и это держит
/// форма команды, а не соглашение. Назначение, наоборот, выбирается: ядро
/// умело это с самого начала (`app::build_corpus_into`), а окно до него не
/// дотягивалось.
///
/// Проверяется по порожденному договору, а не по коду: окно видит именно
/// его, и вернуть аргумент источника проще всего незаметно.
#[test]
fn the_build_command_names_a_destination_and_never_an_archive() {
    let contract = exported();
    assert!(
        contract.contains("buildCorpus: (destination: string | null)"),
        "у buildCorpus нет аргумента назначения или есть еще один"
    );
    for gone in ["localArchive", "archive_missing"] {
        assert!(
            !contract.contains(gone),
            "в договоре осталось упоминание источника: {gone}"
        );
    }
}

/// **Папка проверяется там, где строка пересекает границу.**
///
/// Окно файловых ручек не получает и путей не толкует: строка приходит с
/// той стороны, и первое, что с ней делается – проверка, что за ней есть
/// каталог. Отказ на этом месте – предложение выбрать другую папку, а не
/// ошибка сборки, которой не было; повторять его нечем, поэтому
/// `retryable` – ложь.
#[test]
fn a_destination_that_is_not_a_directory_is_refused_before_anything_starts() {
    let dir = tempfile::tempdir().expect("временный каталог");

    let missing = dir.path().join("нет-такой-папки");
    let refused = chosen_destination(Some(missing.display().to_string()))
        .expect_err("несуществующая папка не принимается");
    assert_eq!(refused.code, "destination_missing");
    assert_eq!(refused.message, "выбранной папки нет на месте");
    assert!(
        !refused.retryable,
        "повторять нечего: надо выбрать другую папку"
    );

    // Файл – не каталог.
    let file = dir.path().join("файл.txt");
    std::fs::write(&file, b"x").expect("записать файл");
    let refused = chosen_destination(Some(file.display().to_string()))
        .expect_err("файл не принимается за папку");
    assert_eq!(refused.code, "destination_missing");

    // Каталог принимается как есть.
    assert_eq!(
        chosen_destination(Some(dir.path().display().to_string())).expect("каталог"),
        Some(dir.path().to_path_buf())
    );

    // А `null` – это папка загрузок, то есть поведение консоли.
    assert_eq!(chosen_destination(None).expect("без папки"), None);
}

/// Отказ ядра переходит на провод целиком, включая то, от чего зависит
/// поведение окна.
#[test]
fn a_failure_crosses_with_its_kind_and_its_advice() {
    let cancelled = aruna::app::Failure::of(&aruna::error::ArunaError::Cancelled {
        phase: aruna::job::Phase::Exporting,
    });
    let wire = BuildFailure::of(&cancelled);

    assert_eq!(wire.code, "cancelled");
    assert_eq!(wire.phase.as_deref(), Some("exporting"));
    assert!(wire.cancelled);
    // `retryable` – совет о неисправности, а отмена ею не является: флаг
    // приходит снятым. На этом стоит ветка отмены в окне – своя, потому
    // что читать отмену флагом повторимости нечем.
    assert!(!wire.retryable);
}
