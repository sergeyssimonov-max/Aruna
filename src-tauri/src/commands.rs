//! Шесть команд окна и единая регистрация для IPC и генерации bindings.

use crate::build::{self, Building};
use crate::dto::{
    BuildFailure, BuildProgress, BuildReport, CommandError, CorpusLocation, CorpusStats, XmlSummary,
};
use crate::read::{is_link, named_package, read_stats, read_xml_summary};

/// Ошибка команды – предложение, а не структура.
///
/// Разбирать в окне нечего ни у читающих команд, ни у открытия описи: отказ
/// показывается целиком, как он написан, и ветвиться по нему окну незачем.
/// Тегированная структура появляется там, где ветвление есть: у сборки, где
/// кодов двадцать девять (двадцать шесть у ядра, три у оболочки) и от них
/// зависит, предлагать ли повтор (`BuildFailure`).
/// Проводной вид при этом тот же, что был до specta: голая строка.
pub(super) fn said(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[tauri::command]
#[specta::specta]
fn corpus_location() -> Result<CorpusLocation, String> {
    let downloads = aruna::paths::downloads_dir().map_err(|_| said(CommandError::Downloads))?;
    let package = downloads.join(aruna::export::PACKAGE);
    let inventory = package.join(aruna::paths::OUTPUT_FILE_NAME);
    Ok(CorpusLocation {
        downloads: downloads.display().to_string(),
        package: package.display().to_string(),
        inventory: inventory.display().to_string(),
        package_exists: package.is_dir(),
        inventory_exists: inventory.is_file(),
    })
}

/// Открыть опись тем, чем читатель обычно открывает HTML.
///
/// Открыть можно ровно один файл – опись, чье имя объявлено ядром; лежать она
/// при этом может где угодно, потому что «Собрать в папку…» кладет ее туда,
/// куда указал человек. Путь приходит от окна, как у [`corpus_stats`] и
/// [`corpus_xml`]: из [`corpus_location`] или из `BuildReport` той сборки,
/// которая его и написала.
// Комментарии ниже намеренно обычные, а не доксрока: доксроки команд specta
// переносит в `bindings.ts`, и объяснение, адресованное этому файлу, уехало бы
// в продукт – то же правило, что у `corpus_stats` про поток.
//
// **Почему это команда оболочки, а не вызов плагина из окна.** До 07.09.2026
// окно звало `openPath` плагина `opener` напрямую, и кнопка не работала ни в
// одной выпущенной сборке. Разрешение `opener:allow-open-path` включает
// команду, но не наполняет ее область путей – так и написано в самом плагине:
// «enables the open_path command without any pre-configured scope». Область
// осталась пустой, `is_path_allowed` вернул `false` обоими своими условиями, и
// на экран легло `Not allowed to open path …`. Наполнить область было нечем:
// область, разрешающая любой путь – это отмена области, а не ее настройка.
//
// Граница, которую область должна была дать, стоит здесь и уже. Rust-сторона
// плагина области не строит вовсе: `Scope::new` во всем плагине встречается
// только в `commands.rs`.
//
// `async` по той же причине, что у соседей: открытие запускает стороннюю
// программу, а синхронная команда осталась бы на главном потоке и подвесила бы
// окно на ее запуск, что запрещает 4.9.7.
#[tauri::command(async)]
#[specta::specta]
fn open_inventory(app: tauri::AppHandle, path: String) -> Result<(), String> {
    named_inventory(std::path::Path::new(&path)).map_err(said)?;
    tauri_plugin_opener::OpenerExt::opener(&app)
        .open_path(path, None::<&str>)
        .map_err(|_| said(CommandError::Opening))
}

/// Проверка без Tauri, чтобы обе отказные ветки проверялись тестом.
///
/// Два условия, и второе не лишнее: окно узнает об описи заранее – при чтении
/// папки или из отчета сборки – а нажимают на кнопку позже, и между тем и
/// другим файл могли убрать.
pub(super) fn named_inventory(path: &std::path::Path) -> Result<(), CommandError> {
    if path.file_name() != Some(std::ffi::OsStr::new(aruna::paths::OUTPUT_FILE_NAME)) {
        return Err(CommandError::NotInventory);
    }
    // Ссылка под именем описи – не опись: opener открыл бы то, на что она
    // указывает. Ядро опись ссылкой не пишет.
    if is_link(path) {
        return Err(CommandError::NotInventory);
    }
    if !path.is_file() {
        return Err(CommandError::InventoryGone);
    }
    Ok(())
}

/// Числа о пакете, который лежит по этому пути.
///
/// Путь приходит от окна, а окно берет его из [`corpus_location`] – своей
/// второй догадки о том, где лежит пакет, здесь нет.
// **`async` здесь не про ожидание, а про поток.** Без него команда исполняется
// на главном потоке – правило Tauri: синхронная команда остается на главном,
// помеченная `async` уходит в `async_runtime::spawn`. Работа тут не мгновенная:
// манифест – без малого девять мегабайт разбора, а запасной путь – обход
// десятков тысяч файлов; и то и другое на главном потоке подвешивает окно ровно
// на свою длительность, что запрещает 4.9.7.
//
// Комментарий намеренно обычный, а не доксрока: доксроки команд specta
// переносит в `bindings.ts`, и объяснение про поток уехало бы в продукт,
// которому оно не адресовано.
#[tauri::command(async)]
#[specta::specta]
fn corpus_stats(path: String) -> Result<CorpusStats, String> {
    let package = std::path::Path::new(&path);
    named_package(package).map_err(said)?;
    read_stats(package).map_err(said)
}

/// Что разборщик сказал о документах пакета, лежащего по этому пути.
///
/// Путь приходит от окна из [`corpus_location`], как и у [`corpus_stats`].
// `async` по той же причине, что у `corpus_stats`: манифест – без малого
// девять мегабайт разбора, и на главном потоке это подвешивает окно ровно на
// свою длительность.
#[tauri::command(async)]
#[specta::specta]
fn corpus_xml(path: String) -> Result<XmlSummary, String> {
    let package = std::path::Path::new(&path);
    named_package(package).map_err(said)?;
    read_xml_summary(package).map_err(said)
}

/// Собрать корпус и сказать, что вышло.
///
/// Две оси, и они разные. **Источник один** – закрепленная запись Zenodo через
/// кеш, то есть ровно то, что делает консольный бинарь: архива команда не
/// принимает вовсе, решением владельца 06.09.2026. **Назначение выбирается:**
/// `destination` – папка, названная человеком, `null` – папка загрузок. Ядро
/// умело это с самого начала, `app::build_corpus_into`; окно до него не
/// дотягивалось. Путь приходит строкой и проверяется здесь: окно файловых ручек
/// не получает (§3 контракта).
///
/// Каждый пакет несет PDF всех документов. Каталог шрифтов берется у Tauri в
/// момент сборки и проверяется один раз, в `app::load_fonts` ядра (решение
/// владельца 18.10.2026, 4.3). Каталога ресурсов нет – ядро откажет
/// `font_missing`, как при отсутствующем файле.
///
/// Работа идет не в главном потоке. Сборка – это от шести секунд до двух
/// минут с PDF, а команда на главном потоке заморозила бы webview и заодно
/// все последующие вызовы, включая отмену.
#[tauri::command]
#[specta::specta]
async fn build_corpus(
    app: tauri::AppHandle,
    state: tauri::State<'_, Building>,
    destination: Option<String>,
) -> Result<BuildReport, BuildFailure> {
    build::start(app, state.inner(), destination).await
}

/// Попросить текущую сборку остановиться.
///
/// Именно попросить: отмена в ядре кооперативная и проверяется в безопасных
/// местах – между документами, между чанками загрузки – а запрос метаданных
/// Zenodo (до десяти секунд), пересчет MD5 архива и все, что идет после начала
/// публикации, не прерываются вовсе. Поэтому окно после нажатия говорит
/// «останавливаю» и меняет это на «остановлено» только по отказу с
/// `cancelled` – подтверждение приходит дважды, как требует §3 контракта.
///
/// Ничего не делает, если сборки нет: нажатие по уже закончившемуся прогону –
/// не ошибка.
#[tauri::command]
#[specta::specta]
fn cancel_build(state: tauri::State<'_, Building>) {
    state.stop();
}

/// Команды и события, объявленные один раз.
///
/// Отсюда и рантайм (`invoke_handler`, `mount_events`), и типы для окна: то же
/// объявление порождает `frontend/src/bindings.ts`, поэтому имя команды,
/// написание ее аргумента и форма ответа не могут разойтись между Rust и
/// TypeScript – раньше их держала внимательность и один файл образцов.
pub(super) fn contract() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            corpus_location,
            corpus_stats,
            corpus_xml,
            open_inventory,
            build_corpus,
            cancel_build
        ])
        .events(tauri_specta::collect_events![BuildProgress])
}
