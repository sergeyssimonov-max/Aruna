#![forbid(unsafe_code)]

// Нужен в обеих сборках с 17.09.2026: `setup` спрашивает у него каталог
// ресурсов и состояние. До этого его звала только ветка `e2e`.
use tauri::Manager;

// E2E installs WDIO's logger instead. Keep the normal app's dependency without
// registering a second logger, and tell Cargo's unused-dependency lint that
// its absence from this feature's runtime path is intentional.
#[cfg(feature = "e2e")]
use tauri_plugin_log as _;

mod build;
mod commands;
mod dto;
mod read;

use build::Building;

#[cfg(feature = "e2e")]
fn wdio_webdriver_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_wdio_webdriver::init()
}

#[cfg(not(feature = "e2e"))]
fn wdio_webdriver_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("noop-wdio-webdriver").build()
}

#[cfg(feature = "e2e")]
fn wdio_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_wdio::init()
}

#[cfg(not(feature = "e2e"))]
fn wdio_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("noop-wdio").build()
}

/// Где внутри приложения лежат шрифты, относительно каталога ресурсов.
///
/// `tauri.conf.json` кладет `cli/resources/fonts/` сюда, и на macOS это
/// `Aruna.app/Contents/Resources/fonts/`. Одно место, названное один раз: путь
/// в конфигурации и путь в коде – это две половины одного решения, и разойтись
/// им нельзя.
pub(crate) const FONT_RESOURCES: &str = "fonts";

/// Итог проверки шрифтов при запуске: каталог, который прошел сверку, либо
/// причина отказа.
///
/// Хранится, а не печатается и забывается – с расчетом на то, что контур PDF
/// откажет со ссылкой на эту причину, а не будет искать шрифт заново. Контур
/// пришел 30.09.2026 и этого итога не читает: запрос окна (`window_request`)
/// берет каталог ресурсов у Tauri в момент сборки, и файлы проверяет ядро,
/// при сборке. Сборке на этот итог не переходить –
/// решение владельца 18.10.2026, 4.3: проверка одна. Метод чтения, которого
/// никто не звал, снят 01.10.2026. Итог остается записью для журнала.
#[derive(Default)]
pub(crate) struct FontResources(
    std::sync::Mutex<Option<std::result::Result<std::path::PathBuf, String>>>,
);

impl FontResources {
    /// Записать итог. Один раз, при запуске.
    fn set(&self, outcome: std::result::Result<std::path::PathBuf, String>) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = Some(outcome);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let contract = commands::contract();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // **Единственное окно исключено из восстановления, и это не отключение
        // плагина.**
        //
        // Окно объявлено `center: true` и `resizable: false`: оно обязано
        // открываться в центре экрана и в тех размерах, что записаны в
        // `tauri.conf.json`. Плагин же по умолчанию делает обратное – кладет
        // на диск позицию и размер при закрытии и возвращает их при следующем
        // запуске, и однажды уже вернул окно 321×262, где кнопка ушла под край.
        //
        // Из трех способов развести это выбран `with_denylist`, потому что в
        // исходнике плагина (`lib.rs`, обработчик `on_window_ready`) проверка
        // denylist стоит раньше и восстановления, и подписки на события окна:
        // для окна из списка плагин не читает состояние и не пишет его. Два
        // других способа слабее. `with_state_flags` без POSITION и SIZE все
        // равно вернул бы MAXIMIZED – развернутое окно, которое нельзя
        // изменить мышью, противоречит само себе. `skip_initial_state`
        // отменяет только чтение при старте, продолжая писать файл состояния,
        // который никто не прочтет.
        //
        // Плагин остается в сборке: список именной, и второе окно, если оно
        // появится, свое состояние получит.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_denylist(&["main"])
                .build(),
        )
        .plugin(wdio_webdriver_plugin())
        .plugin(wdio_plugin())
        // Состояние заводится здесь, до `setup`: `spec-guard.test.ts` находит
        // защиту логгера ниже текстовым поиском относительно `tauri_plugin_log`,
        // и вставка в `setup` сдвинула бы то, что он ищет.
        .manage(Building::default())
        .manage(FontResources::default())
        .invoke_handler(contract.invoke_handler())
        .setup(move |app| {
            // Первой строкой: пока события не смонтированы, ни одно из них не
            // дойдет до окна, а сборку окно может начать сразу.
            contract.mount_events(app);

            // Журнал – до первой записи в него, а не после.
            //
            // `log` без установленного логгера молчит: вызов уходит в никуда.
            // Проверка шрифтов ниже писала в журнал, а плагин регистрировался
            // в конце `setup`, так что удачный исход не попадал в журнал
            // никогда, а отказ уцелел только благодаря `eprintln!` рядом.
            // Держит `spec-guard`.
            #[cfg(not(feature = "e2e"))]
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Шрифты проверяются здесь при запуске – для журнала.
            //
            // Путь сборки этот итог не читает: сборка берет каталог ресурсов у
            // Tauri заново, и файлы проверяет ядро (`app::load_fonts`, решение
            // владельца 18.10.2026, 4.3). Отказ здесь окно не меняет: сборка
            // откажет `font_missing` или `font_altered` сама.
            //
            // **Почему при запуске, а не при сборке.** Сборочная проверка
            // держит дерево – это `fonts::tests` и `cli/tests/fonts.rs`, и они
            // ловят правку шрифта до того, как он уедет. Но файл внутри уже
            // установленного приложения ей не виден: подмену, обрезание при
            // копировании образа, испорченный том – видит только тот, кто
            // читает файл на той машине. Поэтому проверок две, и вторая
            // здесь.
            //
            // Каталог берется у Tauri и больше нигде: ни системных путей, ни
            // поиска по имени семейства. Приложение, не знающее, где его
            // ресурсы, не станет угадывать – в PDF угаданный шрифт нарисует
            // не тот знак, и никто ниже по течению этого не заметит.
            let outcome = app
                .handle()
                .path()
                .resource_dir()
                .map_err(|e| format!("каталог ресурсов недоступен: {e}"))
                .and_then(|dir| {
                    let fonts = dir.join(FONT_RESOURCES);
                    aruna::fonts::verify_dir(&fonts)
                        .map(|()| fonts)
                        .map_err(|e| e.to_string())
                });
            match &outcome {
                Ok(dir) => log::info!("шрифты на месте: {}", dir.display()),
                // Отказ называет файл и причину – их несет сама ошибка ядра.
                // `eprintln!` рядом с журналом сознательно: плагин журнала
                // регистрируется только в отладочной сборке, а это сообщение
                // нужно и в выпуске.
                Err(message) => {
                    log::error!("{message}");
                    eprintln!("Aruna: {message}");
                }
            }
            app.handle().state::<FontResources>().set(outcome);

            #[cfg(feature = "e2e")]
            app.handle()
                .add_capability(include_str!("../capabilities-e2e/e2e.json"))?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Провод: типы, которые окно получает, и обещания о том, что по нему не ходит.
///
/// Как и `counting` ниже, модуль закрыт только `test`, без привязки к фиче:
/// договор между Rust и окном один и тот же в обеих сборках.
#[cfg(test)]
#[path = "tests/wire.rs"]
mod wire;

// Проверка имени описи к фиче отношения не имеет, поэтому модуль закрыт только
// `test`: обе отказные ветки проверяются и в сборке с `e2e`, и без нее.
#[cfg(test)]
#[path = "tests/opening.rs"]
mod opening;

/// Архив корпуса для тяжелых тестов оболочки – один помощник на всех.
///
/// Порядок тот же, что у `corpus_archive` в `cli/tests/support` (оттуда
/// оболочка взять не может): `ARUNA_ZIP`, затем `ARUNA_FIXTURE_ZIP` (CI ставит
/// его только задаче разбора корпуса, а она гоняет тесты ядра), затем фикстура
/// репозитория. Названный – окончательно:
/// отсутствующий файл не подменяется путем по умолчанию, иначе прогон на
/// «другом» архиве прошел бы на фикстуре репозитория. Без архива – `None` и пропуск, а при
/// `ARUNA_REQUIRE_FIXTURE` – падение, как в ядре: до 02.10.2026 у тестов отмены
/// был свой помощник без этой проверки, и без архива они молча зеленели.
#[cfg(test)]
fn corpus_fixture() -> Option<std::path::PathBuf> {
    let zip = std::env::var_os("ARUNA_ZIP")
        .or_else(|| std::env::var_os("ARUNA_FIXTURE_ZIP"))
        .map_or_else(
            || {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../cli/fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip")
            },
            std::path::PathBuf::from,
        );
    if zip.is_file() {
        return Some(zip);
    }
    assert!(
        std::env::var_os("ARUNA_REQUIRE_FIXTURE").is_none(),
        "ARUNA_REQUIRE_FIXTURE is set but the corpus archive is not there: {}",
        zip.display()
    );
    None
}

// Разбор манифеста к фиче отношения не имеет, поэтому модуль закрыт только
// `test`: ветки проверяются и в сборке с `e2e`, и без нее.
#[cfg(test)]
#[path = "tests/markup.rs"]
mod markup;

// Счет по пакету к фиче отношения не имеет, поэтому модуль закрыт только
// `test`: обе ветки проверяются и в сборке с `e2e`, и без нее.
#[cfg(test)]
#[path = "tests/counting.rs"]
mod counting;

// Gated on the feature as well as on `test`: the one test inside is about what
// a build *without* `e2e` registers, so under the feature the module would be
// empty and its imports unused – which `clippy --all-features` reports,
// correctly.
#[cfg(all(test, not(feature = "e2e")))]
#[path = "tests/release.rs"]
mod tests;

/// Отмена идущей сборки – со стороны оболочки, на настоящей сборке.
///
/// Шестая красная позиция заслона 13.09.2026: «отмена идущей сборки через окно
/// ничем не доказана». Доказано до сих пор было соседнее – что `Building::stop`
/// поднимает флаг, который тест сам же ему и вручил, и что окно по отказу с
/// `cancelled` говорит «Остановлено». Между этими двумя фактами лежала сборка,
/// которой ни один тест не запускал.
///
/// Здесь она идет. Флаг живет в том же `Building`, что у команды; останавливает
/// его тот же `stop()`, который зовет `cancel_build`; работу делает тот же
/// `build_once`, который зовет `build_corpus`. Разница с командой одна и она
/// названа: архив берется с диска, а не из закрепленной записи Zenodo, потому
/// что тест в сеть не ходит.
///
/// Момент отмены выбран событием, а не таймером: нажатие приходит, когда окно
/// увидело «пишу документы» – и это воспроизводимо на любой машине, тогда как
/// задержка в миллисекундах делает тест хрупким и прячет дефект.
///
/// Зубы проверены пробой 17.09.2026: с обезвреженным `Building::stop` первый из
/// двух тестов падает на том, что сборка дошла до конца.
#[cfg(test)]
#[path = "tests/cancelling.rs"]
mod cancelling;
