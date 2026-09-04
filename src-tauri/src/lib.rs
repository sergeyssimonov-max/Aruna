#![forbid(unsafe_code)]

#[cfg(feature = "e2e")]
use tauri::Manager;

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

/// Где лежит то, что собрала консольная часть программы.
///
/// Единственный источник этих путей – ядро: каталог загрузок отдает
/// `aruna::paths`, имя пакета и имя описи объявлены там же константами.
/// Оболочка ничего не вычисляет сама, иначе одно поведение имело бы две
/// реализации, расходящиеся при первой же правке ядра.
#[derive(serde::Serialize, specta::Type)]
pub struct CorpusLocation {
    downloads: String,
    package: String,
    inventory: String,
    package_exists: bool,
    inventory_exists: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("не удалось определить папку загрузок")]
    Downloads,
}

/// Сколько в собранном пакете рукописей и групп.
///
/// Числа окно показывает как есть, поэтому здесь они уже такие, какими их надо
/// показать: пересчета на стороне окна нет.
#[derive(Debug, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct CorpusStats {
    manuscripts: u32,
    groups: u32,
    source: StatsSource,
    /// Как фрагменты разложены по группам, а не только сколько их всего.
    spread: Spread,
    /// Что манифест насчитал о письме корпуса.
    ///
    /// `None`, когда числа взяты обходом каталога: эти счетчики получены при
    /// разборе документов и по разложенному пакету не восстанавливаются.
    /// Пустая структура из нулей соврала бы – ноль документов вне NFC и
    /// «неизвестно, сколько их» на экране выглядят одинаково, а значат разное.
    fonts: Option<Fonts>,
}

/// Разложение фрагментов по группам CTH.
///
/// Два итога – сколько фрагментов и сколько групп – ничего не говорят о том,
/// как одно распределено по другому, а распределение здесь крайне неровное:
/// в самой большой группе больше фрагментов, чем в четырех сотнях самых
/// маленьких вместе.
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Spread {
    /// Самая большая группа. `None` – в пакете нет ни одной.
    largest: Option<GroupSize>,
    /// Групп ровно с одним фрагментом.
    singletons: u32,
    /// Фрагменты, у которых CTH нет.
    ///
    /// Экспорт кладет их в группу с меткой `aruna::parse::MISSING`, и метка
    /// берется у ядра, а не пишется здесь строкой: она принадлежит разбору, и
    /// вторая ее копия разошлась бы с первой молча.
    without_cth: u32,
}

/// Группа и ее размер.
#[derive(Debug, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct GroupSize {
    label: String,
    fragments: u32,
}

/// Что манифест насчитал о письме корпуса при разборе.
#[derive(Debug, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Fonts {
    /// Документы, чей текст пришел не в нормальной форме C.
    not_in_nfc: u32,
    /// Документы, где встречаются кодовые точки из области частного
    /// использования.
    with_private_use: u32,
    /// Сколько таких точек различают во всем корпусе.
    private_use_points: u32,
    /// Аномалии письма – все шесть счетчиков манифеста одним числом.
    anomalies: u32,
}

/// Откуда взяты числа.
///
/// Поле нужно не окну, а тому, кто разбирается в расхождении: манифест пишет
/// экспорт ядра в тот же миг, когда пакет складывается, а обход каталога
/// считает то, что на диске лежит сейчас. Разойтись они могут только если
/// пакет после сборки правили руками, и тогда важно знать, какой из двух
/// ответов получен.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum StatsSource {
    Manifest,
    Walk,
}

/// Почему числа получить не удалось.
///
/// Путей в сообщениях нет намеренно – тем же правилом живет ядро: адрес файла
/// в тексте ошибки попадает в окно, а окно показывают через плечо.
#[derive(Debug, thiserror::Error)]
pub enum StatsError {
    #[error("пакет по этому пути не найден")]
    Missing,
    #[error("каталог пакета не читается: {0}")]
    Read(String),
}

/// Счетчик на проводе — тридцать два бита, и это не сужение, а точное
/// объявление.
///
/// JSON несет числа как double, поэтому целое, способное перевалить за 2^53,
/// пересекает границу с потерей; specta по этой причине отказывается
/// экспортировать `usize` и `u64` вовсе, и правильный ответ на отказ — назвать
/// тот тип, которым число на самом деле является. Все числа этого окна
/// ограничены заведомо ниже: документов в архиве не бывает больше
/// `archive::MAX_ENTRIES`, то есть 500 000, групп — не больше, чем документов,
/// а байт загрузки — больше гигабайта их не примет сам загрузчик.
///
/// Насыщение, а не усечение: показать предел честнее, чем показать остаток от
/// деления.
fn counted<T: TryInto<u32>>(value: T) -> u32 {
    value.try_into().unwrap_or(u32::MAX)
}

/// Ошибка команды — предложение, а не структура.
///
/// У обеих читающих команд отказ ровно один, и сказать о нем больше, чем
/// сказано в тексте, нечего: разбирать в окне нечего, показывать надо целиком.
/// Тегированная структура появляется там, где ветвление есть, — у сборки, где
/// кодов двадцать и от них зависит, предлагать ли повтор (`BuildFailure`).
/// Проводной вид при этом тот же, что был до specta: голая строка.
fn said(error: impl std::fmt::Display) -> String {
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

/// Числа о пакете, который лежит по этому пути.
///
/// Путь приходит от окна, а окно берет его из [`corpus_location`], – своей
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
    read_stats(std::path::Path::new(&path)).map_err(said)
}

/// Команда без Tauri, чтобы обе ветки проверялись тестом.
///
/// Манифест – первый источник, потому что его пишет тот же экспорт, который
/// раскладывал файлы: это его собственный счет, а не пересчет чужой работы.
/// Обход каталога – ответ на случай, когда манифеста нет или он не тот:
/// испорченный JSON и манифест без `counts` ведут туда же, куда его отсутствие,
/// потому что для окна это одно и то же положение – числа надо взять с диска.
fn read_stats(package: &std::path::Path) -> Result<CorpusStats, StatsError> {
    if !package.is_dir() {
        return Err(StatsError::Missing);
    }
    match counts_from_manifest(package) {
        Some(stats) => Ok(stats),
        None => count_by_walking(package),
    }
}

/// Что манифест пакета говорит о его содержимом, если он там есть и читается.
///
/// Структуры объявлены здесь, а не рядом с ответом команды: из всего манифеста
/// – а это без малого девять мегабайт – окну нужна горстка чисел, и объявить
/// хочется ровно их. Остальные поля serde пропускает.
///
/// **Размеры групп читаются, а документы – нет.** В манифесте у каждой группы
/// лежит полный список ее документов с десятью полями на каждый; окну от этого
/// списка нужна одна длина. `Vec<IgnoredAny>` – это и есть «сосчитай, но не
/// собирай»: serde проходит массив, ничего из него не строит, а `len` остается
/// верным. Разбор целиком с материализацией строк стоил бы на порядок дороже
/// ради данных, которые тут же были бы выброшены.
fn counts_from_manifest(package: &std::path::Path) -> Option<CorpusStats> {
    #[derive(serde::Deserialize)]
    struct Counts {
        documents: usize,
        groups: usize,
    }

    #[derive(serde::Deserialize)]
    struct Group {
        label: String,
        #[serde(default)]
        documents: Vec<serde::de::IgnoredAny>,
    }

    /// Счетчики письма. Аномалии – картой, а не шестью полями: манифест их
    /// перечисляет сам, и седьмая, добавленная завтра, попадет в сумму без
    /// правки здесь.
    #[derive(serde::Deserialize)]
    struct FontsEntry {
        documents_not_in_nfc: usize,
        documents_with_private_use: usize,
        #[serde(default)]
        private_use_points: Vec<serde::de::IgnoredAny>,
        #[serde(default)]
        anomalies: std::collections::BTreeMap<String, usize>,
    }

    #[derive(serde::Deserialize)]
    struct Manifest {
        counts: Counts,
        /// Не обязателен: манифест без списка групп – это манифест, по
        /// которому разбивку не построить, а два итога по-прежнему верны.
        #[serde(default)]
        groups: Vec<Group>,
        #[serde(default)]
        fonts: Option<FontsEntry>,
    }

    let text = std::fs::read_to_string(package.join(aruna::export::MANIFEST)).ok()?;
    let manifest: Manifest = serde_json::from_str(&text).ok()?;
    Some(CorpusStats {
        manuscripts: counted(manifest.counts.documents),
        groups: counted(manifest.counts.groups),
        source: StatsSource::Manifest,
        spread: spread_of(
            manifest
                .groups
                .into_iter()
                .map(|group| (group.label, group.documents.len())),
        ),
        fonts: manifest.fonts.map(|fonts| Fonts {
            not_in_nfc: counted(fonts.documents_not_in_nfc),
            with_private_use: counted(fonts.documents_with_private_use),
            private_use_points: counted(fonts.private_use_points.len()),
            anomalies: counted(fonts.anomalies.values().sum::<usize>()),
        }),
    })
}

/// Разбивка по группам – из того, как они названы и сколько в них фрагментов.
///
/// Одна функция на оба источника: манифест перечисляет группы с их
/// документами, обход – каталоги с их файлами, а вопросы к этому перечню
/// одинаковые. Второй экземпляр этой арифметики разошелся бы с первым ровно
/// тогда, когда числа с двух источников сравнят.
fn spread_of(groups: impl IntoIterator<Item = (String, usize)>) -> Spread {
    let mut singletons = 0;
    let mut without_cth = 0;
    let mut largest: Option<GroupSize> = None;

    for (label, fragments) in groups {
        if fragments == 1 {
            singletons += 1;
        }
        if label == aruna::parse::MISSING {
            without_cth += counted(fragments);
        }
        // Строго больше: при равенстве остается первая встреченная, а порядок
        // здесь – тот, в котором группы перечисляет опись.
        if largest
            .as_ref()
            .is_none_or(|biggest| counted(fragments) > biggest.fragments)
        {
            largest = Some(GroupSize {
                label,
                fragments: counted(fragments),
            });
        }
    }

    Spread {
        largest,
        singletons,
        without_cth,
    }
}

/// Пересчет по разложенному пакету: группы – подкаталоги, рукописи – файлы XML
/// внутри них.
///
/// Ровно та раскладка, которую делает экспорт: каталог на группу CTH, документ
/// на рукопись. Файлы в корне пакета – манифест и опись – группами не считаются
/// потому, что каталогами не являются.
fn count_by_walking(package: &std::path::Path) -> Result<CorpusStats, StatsError> {
    let failed = |err: std::io::Error| StatsError::Read(err.to_string());

    let mut sizes: Vec<(String, usize)> = Vec::new();
    let mut manuscripts = 0;
    for group in std::fs::read_dir(package).map_err(failed)? {
        let group = group.map_err(failed)?;
        if !group.file_type().map_err(failed)?.is_dir() {
            continue;
        }
        let mut fragments = 0;
        for document in std::fs::read_dir(group.path()).map_err(failed)? {
            let document = document.map_err(failed)?;
            let path = document.path();
            let is_xml = path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"));
            if is_xml {
                fragments += 1;
            }
        }
        manuscripts += fragments;
        // Имя каталога, а не метка группы: экспорт получает первое из второго
        // через `dir_component`, и для группы без CTH они совпадают – метка
        // `—` проходит правило имени неизменной. Там, где они разойдутся –
        // сигла со слешем внутри метки, – расходится и то, что показывает
        // обход: он видит каталог и честно называет его так, как тот назван.
        sizes.push((group.file_name().to_string_lossy().into_owned(), fragments));
    }

    Ok(CorpusStats {
        manuscripts: counted(manuscripts),
        groups: counted(sizes.len()),
        source: StatsSource::Walk,
        spread: spread_of(sizes),
        // Обход считает файлы, а не читает документы: как написан их текст, он
        // не знает и знать не может.
        fonts: None,
    })
}

// ---------------------------------------------------------------------------
// Сборка корпуса: то, ради чего окно и заводилось
// ---------------------------------------------------------------------------

/// Что сборка дала.
///
/// Числа не пересчитываются: это то, что вернул сам прогон, — `CorpusReport`
/// ядра, переложенный во владеющий вид. Пересчет после сборки уже однажды
/// разошелся с манифестом в этом проекте, и это отдельная строка в комментарии
/// `app::PackageReport`.
///
/// Пути здесь есть, и это не то же самое, что путь в сообщении об ошибке
/// (правило рядом, у [`StatsError`]): показать, что собрано и где оно лежит, —
/// и есть работа этого окна.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct BuildReport {
    /// Идентификатор прогона. Им же помечены события прогресса, так что окно
    /// может отличить отчет своей сборки от чужой.
    pub job: u32,
    pub package: String,
    pub inventory: String,
    /// Архив, из которого собрано, когда его выбрал человек; `null`, когда
    /// архив пришел с Zenodo через кеш.
    pub archive: Option<String>,
    pub documents: u32,
    pub groups: u32,
    /// Документы, которым пришлось дать суффикс: их сиглум был уже занят.
    pub disambiguated: u32,
    /// Документы, из которых убрана ссылка на таблицу стилей, которой в пакете
    /// нет.
    pub stylesheet_dropped: u32,
}

/// Почему сборка не дошла до конца.
///
/// Тегированная структура, как требует §3 `docs/FRONTEND-CONTRACT.md`: вид,
/// предложение для человека, фаза и — отдельно от всего — стоит ли предлагать
/// повтор. Строкой это быть не может: у отказа двадцать видов, и от вида
/// зависит, что окну делать дальше.
///
/// Поля один в один повторяют `app::Failure` ядра, включая `retryable`, который
/// там не переписан заново, а делегирован клиенту загрузки: две независимые
/// формулировки того же правила в этом проекте уже расходились.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct BuildFailure {
    /// Устойчивый машинный вид: `cancelled`, `network`, `checksum`, …
    pub code: String,
    /// Фаза, на которой это случилось, — `Phase::code` ядра. `null` там, где
    /// отказ возможен на любой.
    pub phase: Option<String>,
    /// Одно предложение для человека. Пути файловой системы из него убраны
    /// ядром.
    pub message: String,
    pub retryable: bool,
    /// Единственный исход, который не является неисправностью.
    pub cancelled: bool,
}

impl BuildFailure {
    /// Отказ ядра, как его видит окно.
    fn of(failure: &aruna::app::Failure) -> BuildFailure {
        BuildFailure {
            code: failure.code.to_string(),
            phase: failure.phase.map(|phase| phase.code().to_string()),
            message: failure.message.clone(),
            retryable: failure.retryable,
            cancelled: failure.cancelled,
        }
    }

    /// Отказ самой оболочки: у ядра такого вида нет, потому что это не о
    /// корпусе, а о том, что окно попросило невозможное.
    fn shell(code: &str, message: &str, retryable: bool) -> BuildFailure {
        BuildFailure {
            code: code.to_string(),
            phase: None,
            message: message.to_string(),
            retryable,
            cancelled: false,
        }
    }
}

/// Стадия прогона, как ее называет провод.
///
/// Перечислением, а не строкой, и это не украшение: specta выводит из него
/// объединение литералов, а `svelte-check` по объединению проверяет, что окно
/// разобрало все стадии. Строка позволила бы забыть одну и показать читателю
/// пустое место — ровно тот отказ, ради которого `progress::Event` в ядре не
/// `#[non_exhaustive]`.
///
/// Имена принадлежат оболочке: по `docs/ARCHITECTURE.md` §7 события IPC — ее
/// собственность, а не ядра. Их семнадцать против девятнадцати вариантов
/// события, потому что две пары — объявление стадии и ее тик — это одна стадия.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    CacheUnusable,
    CachedArchiveRejected,
    ArchiveFromCache,
    ZenodoNotice,
    ZenodoUnreachable,
    Downloading,
    DownloadRetrying,
    ArchiveKept,
    Parsing,
    EntriesSkipped,
    Indexed,
    ReadingHeaders,
    HeadersRead,
    Writing,
    CheckingPackage,
    CheckingPublished,
    PreviousPackageLeft,
}

/// Насколько далеко зашла сборка.
///
/// Одно событие на все стадии, а не по типу на каждую: окну нужно имя стадии и,
/// где она их знает, две половины дроби. Новый показатель — поле здесь, и
/// старое окно, которое о нем не знает, продолжает работать.
///
/// **Путей не носит.** Пять вариантов `progress::Event` несут `&Path`, и по
/// правилу рядом со [`StatsError`] им сюда нельзя: окно показывают через плечо.
/// Из таких событий сюда доходит только имя стадии.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
pub struct BuildProgress {
    /// Чей это прогресс. Тот же номер приходит в [`BuildReport::job`].
    pub job: u32,
    /// На чем прогон сейчас.
    pub stage: Stage,
    /// Числитель и знаменатель, когда стадия умеет их назвать. У загрузки
    /// знаменателя может не быть: сервер не обязан объявлять длину.
    pub done: Option<u32>,
    pub total: Option<u32>,
    /// Сколько нашлось, когда это уже известно, — окно говорит числа до того,
    /// как появится отчет.
    pub manuscripts: Option<u32>,
    pub groups: Option<u32>,
    /// Предложение, когда событие несет то, что стоит показать словами.
    pub note: Option<String>,
}

impl BuildProgress {
    /// Событие ядра, переложенное на провод.
    ///
    /// Разбор исчерпывающий и без `_`: `progress::Event` намеренно не
    /// `#[non_exhaustive]`, чтобы новая стадия ядра ломала сборку здесь, а не
    /// молча пропадала из окна.
    fn of(job: u32, event: &aruna::progress::Event<'_>) -> BuildProgress {
        use aruna::progress::Event as Core;

        let mut progress = BuildProgress {
            job,
            stage: Stage::Parsing,
            done: None,
            total: None,
            manuscripts: None,
            groups: None,
            note: None,
        };
        progress.stage = match event {
            Core::CacheUnusable { .. } => Stage::CacheUnusable,
            Core::CachedArchiveRejected => Stage::CachedArchiveRejected,
            Core::ArchiveFromCache { .. } => Stage::ArchiveFromCache,
            Core::ZenodoNotice { message } => {
                progress.note = Some((*message).to_string());
                Stage::ZenodoNotice
            }
            Core::ZenodoUnreachable { cause } => {
                progress.note = Some((*cause).to_string());
                Stage::ZenodoUnreachable
            }
            // Стадия объявляет знаменатель, тик заполняет числитель. Ноль в
            // начале — чтобы полоса появилась сразу, а не после первой четверти
            // секунды.
            Core::DownloadStarted => {
                progress.done = Some(0);
                Stage::Downloading
            }
            Core::Downloading { bytes, total } => {
                progress.done = Some(counted(*bytes));
                // Длина, которую этот загрузчик все равно откажется принять
                // (потолок — гигабайт), знаменателем не является: лучше
                // показать движение без доли, чем долю от неправды.
                progress.total = total.and_then(|total| u32::try_from(total).ok());
                Stage::Downloading
            }
            // Сообщение берется у `Failure`, а не у самой ошибки: только там из
            // него убран путь.
            Core::DownloadRetrying { error, .. } => {
                progress.note = Some(aruna::app::Failure::of(error).message);
                Stage::DownloadRetrying
            }
            Core::ArchiveKept { .. } => Stage::ArchiveKept,
            Core::ParsingArchive => Stage::Parsing,
            Core::EntriesSkipped { .. } => Stage::EntriesSkipped,
            Core::Indexed { manuscripts } => {
                progress.manuscripts = Some(counted(*manuscripts));
                Stage::Indexed
            }
            Core::ReadingHeaders => Stage::ReadingHeaders,
            Core::HeadersRead {
                manuscripts,
                groups,
            } => {
                progress.manuscripts = Some(counted(*manuscripts));
                progress.groups = Some(counted(*groups));
                Stage::HeadersRead
            }
            Core::WritingDocuments { documents } => {
                progress.done = Some(0);
                progress.total = Some(counted(*documents));
                Stage::Writing
            }
            Core::DocumentsWritten { done, total } => {
                progress.done = Some(counted(*done));
                progress.total = Some(counted(*total));
                Stage::Writing
            }
            Core::CheckingPackage => Stage::CheckingPackage,
            Core::CheckingPublished => Stage::CheckingPublished,
            Core::PreviousPackageLeft { .. } => Stage::PreviousPackageLeft,
        };
        progress
    }
}

/// Синк прогресса, который шлет события в окно.
///
/// `report` обязан не паниковать: `catch_unwind` в проекте нет нигде, а паника
/// отсюда прошла бы сквозь `export::build` и вернулась бы обломком задания без
/// объяснения. Поэтому отказ отправки проглатывается: окно, которое закрыли на
/// середине сборки, — это не сбой сборки.
struct WindowProgress {
    app: tauri::AppHandle,
    job: u32,
}

impl aruna::progress::Progress for WindowProgress {
    fn report(&self, event: aruna::progress::Event<'_>) {
        use tauri_specta::Event as _;
        let _ = BuildProgress::of(self.job, &event).emit(&self.app);
    }
}

/// Идет ли сборка, и чем ее остановить.
///
/// Флаг живет здесь, а не в команде: `cancel_build` приходит вторым вызовом,
/// когда кадр первого еще не вернулся, — и `Job::with_id` написан ровно для
/// этого случая. `Cancel` клонируется поверх `Arc`, поэтому останавливает не тот
/// поток, который работает.
#[derive(Default)]
pub struct Building(std::sync::Mutex<Option<aruna::job::Cancel>>);

impl Building {
    /// Занять место под сборку, если оно свободно.
    ///
    /// Отдельной функцией, а не строками внутри команды, по одной причине: это
    /// и есть правило «одна сборка за раз», и проверить его должно быть можно
    /// без Tauri вокруг.
    fn claim(&self, cancel: aruna::job::Cancel) -> Result<(), BuildFailure> {
        let mut slot = self.0.lock().map_err(|_| {
            BuildFailure::shell("interrupted", "предыдущая сборка оборвалась", true)
        })?;
        if slot.is_some() {
            return Err(BuildFailure::shell(
                "busy",
                "сборка уже идет",
                // Повторить имеет смысл — но после того, как закончится та.
                true,
            ));
        }
        *slot = Some(cancel);
        Ok(())
    }

    /// Освободить место, чем бы прогон ни кончился.
    fn release(&self) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = None;
        }
    }

    /// Попросить текущую сборку остановиться. Молча, если ее нет.
    fn stop(&self) {
        if let Ok(slot) = self.0.lock() {
            if let Some(cancel) = slot.as_ref() {
                cancel.cancel();
            }
        }
    }
}

/// Архив, выбранный человеком, — проверенный здесь, а не там, где он читается.
///
/// Окно файловых ручек не получает и путей не толкует: строка приходит с той
/// стороны, и первое, что с ней делается, — проверка, что за ней есть файл.
/// Отказ на этом месте — предложение выбрать другой, а не ошибка сборки,
/// которой не было.
fn chosen_archive(
    local_archive: Option<String>,
) -> Result<Option<std::path::PathBuf>, BuildFailure> {
    match local_archive {
        Some(given) => {
            let path = std::path::PathBuf::from(given);
            if !path.is_file() {
                return Err(BuildFailure::shell(
                    "archive_missing",
                    "выбранного архива нет на месте",
                    false,
                ));
            }
            Ok(Some(path))
        }
        None => Ok(None),
    }
}

/// Собрать корпус и сказать, что вышло.
///
/// `local_archive` — архив, выбранный человеком; `null` означает закрепленную
/// запись Zenodo через кеш, то есть ровно то, что делает консольный бинарь.
/// Путь приходит строкой и проверяется здесь: окно файловых ручек не получает
/// (§3 контракта).
///
/// Работа идет не в главном потоке. Сборка — это от шести секунд до минуты с
/// лишним, а команда на главном потоке заморозила бы webview и заодно все
/// последующие вызовы, включая отмену.
#[tauri::command]
#[specta::specta]
async fn build_corpus(
    app: tauri::AppHandle,
    state: tauri::State<'_, Building>,
    local_archive: Option<String>,
) -> Result<BuildReport, BuildFailure> {
    let archive = chosen_archive(local_archive)?;
    let cancel = aruna::job::Cancel::new();
    state.claim(cancel.clone())?;

    let handle = app.clone();
    let chosen = archive.clone();
    // Задание строится внутри замыкания, и иначе нельзя: `Job<'a>` заимствует
    // и синк, и флаг, поэтому оно не может жить дольше вызова, который его
    // создал. Через границу потока переходят владеющие половины.
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let id = aruna::job::JobId::next();
        let sink = WindowProgress {
            app: handle,
            job: counted(id.get()),
        };
        let job = aruna::job::Job::with_id(id, &sink, &cancel);
        let request = aruna::app::CorpusRequest {
            local_archive: chosen.clone(),
        };
        aruna::app::build_corpus(&request, &job)
            .map(|report| BuildReport {
                job: counted(report.job.get()),
                package: report.package.root.display().to_string(),
                inventory: report.inventory.display().to_string(),
                archive: chosen.as_ref().map(|path| path.display().to_string()),
                documents: counted(report.package.documents),
                groups: counted(report.package.groups),
                disambiguated: counted(report.package.disambiguated),
                stylesheet_dropped: counted(report.package.stylesheet_dropped),
            })
            .map_err(|error| BuildFailure::of(&aruna::app::Failure::of(&error)))
    })
    .await;

    // Место освобождается чем бы прогон ни кончился, иначе окно осталось бы
    // навсегда занятым сборкой, которой уже нет.
    state.release();

    match outcome {
        Ok(result) => result,
        // Задание не вернулось: рабочий поток снят или сорван паникой изнутри
        // зависимости — в собственном коде паник нет, это правило проекта.
        Err(_) => Err(BuildFailure::shell(
            "interrupted",
            "сборка оборвалась, не сказав почему",
            true,
        )),
    }
}

/// Попросить текущую сборку остановиться.
///
/// Именно попросить: отмена в ядре кооперативная и проверяется в безопасных
/// местах — между документами, между чанками загрузки, — а запрос метаданных
/// Zenodo (до десяти секунд), пересчет MD5 архива и все, что идет после начала
/// публикации, не прерываются вовсе. Поэтому окно после нажатия говорит
/// «останавливаю» и меняет это на «остановлено» только по отказу с
/// `cancelled` — подтверждение приходит дважды, как требует §3 контракта.
///
/// Ничего не делает, если сборки нет: нажатие по уже закончившемуся прогону —
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
/// TypeScript — раньше их держала внимательность и один файл образцов.
fn contract() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            corpus_location,
            corpus_stats,
            build_corpus,
            cancel_build
        ])
        .events(tauri_specta::collect_events![BuildProgress])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let contract = contract();
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
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(wdio_webdriver_plugin())
        .plugin(wdio_plugin())
        // Состояние заводится здесь, до `setup`: `spec-guard.test.ts` находит
        // защиту логгера ниже текстовым поиском относительно `tauri_plugin_log`,
        // и вставка в `setup` сдвинула бы то, что он ищет.
        .manage(Building::default())
        .invoke_handler(contract.invoke_handler())
        .setup(move |app| {
            // Первой строкой: пока события не смонтированы, ни одно из них не
            // дойдет до окна, а сборку окно может начать сразу.
            contract.mount_events(app);

            #[cfg(feature = "e2e")]
            app.handle()
                .add_capability(include_str!("../capabilities-e2e/e2e.json"))?;

            #[cfg(not(feature = "e2e"))]
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
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
mod wire {
    use super::*;

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
    /// Две сборки в одном каталоге назначения — это два экспорта, спорящих за
    /// одну публикацию; ядро в этом случае отвечает `publish_busy`, но узнать
    /// об этом через минуту загрузки было бы поздно. Окно узнает сразу.
    #[test]
    fn a_second_build_is_refused_while_the_first_is_running() {
        let building = Building::default();

        building
            .claim(aruna::job::Cancel::new())
            .expect("место свободно");
        let refused = building
            .claim(aruna::job::Cancel::new())
            .expect_err("вторая сборка не начинается");

        assert_eq!(refused.code, "busy");
        assert!(refused.retryable, "повторить можно — после первой");
        assert!(!refused.cancelled);

        building.release();
        building
            .claim(aruna::job::Cancel::new())
            .expect("после прогона место снова свободно");
    }

    /// Отмена доходит до флага, который держит идущая сборка.
    ///
    /// Тот самый случай, ради которого флаг лежит в состоянии приложения:
    /// `cancel_build` — второй вызов, приходящий, пока кадр первого еще не
    /// вернулся.
    #[test]
    fn a_stop_reaches_the_flag_the_running_build_holds() {
        let building = Building::default();
        let cancel = aruna::job::Cancel::new();
        building.claim(cancel.clone()).expect("место свободно");

        assert!(!cancel.is_cancelled());
        building.stop();
        assert!(cancel.is_cancelled(), "отмена не дошла до прогона");
    }

    /// Нажатие по сборке, которой нет, — не ошибка.
    #[test]
    fn a_stop_with_nothing_running_says_nothing() {
        Building::default().stop();
    }

    /// Архив проверяется там, где строка пересекает границу.
    #[test]
    fn an_archive_that_is_not_there_is_refused_before_anything_starts() {
        let dir = tempfile::tempdir().expect("временный каталог");
        let missing = dir.path().join("нет-такого.zip");

        let refused = chosen_archive(Some(missing.display().to_string()))
            .expect_err("несуществующий архив не принимается");
        assert_eq!(refused.code, "archive_missing");
        assert!(
            !refused.retryable,
            "повторять нечего: надо выбрать другой файл"
        );

        // Каталог — не архив.
        let refused = chosen_archive(Some(dir.path().display().to_string()))
            .expect_err("каталог не принимается за архив");
        assert_eq!(refused.code, "archive_missing");

        // А `null` — это Zenodo через кеш, то есть поведение консоли.
        assert_eq!(chosen_archive(None).expect("без архива"), None);
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
    }
}

// Счет по пакету к фиче отношения не имеет, поэтому модуль закрыт только
// `test`: обе ветки проверяются и в сборке с `e2e`, и без нее.
#[cfg(test)]
mod counting {
    use super::*;
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
    /// Числа в нем нарочно не сходятся с тем, что лежит на диске, – иначе тест
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

    /// **Без манифеста считается то, что на диске.**
    #[test]
    fn a_package_without_a_manifest_is_counted_by_walking() {
        let dir = tempfile::tempdir().unwrap();
        package(dir.path(), 4, 5);

        let stats = read_stats(dir.path()).unwrap();

        assert_eq!(stats.manuscripts, 20);
        assert_eq!(stats.groups, 4);
        assert_eq!(stats.source, StatsSource::Walk);
        assert_eq!(stats.spread.singletons, 0);
        assert_eq!(stats.spread.without_cth, 0);
        assert_eq!(
            stats.fonts, None,
            "обход не читает документы и не может знать, как написан их текст"
        );

        // Все четыре группы одного размера, поэтому проверяется размер, а имя
        // – только тем, что оно вообще из пакета: порядок, в котором файловая
        // система отдает каталоги, здесь не обещан никем.
        let largest = stats.spread.largest.expect("в пакете есть группы");
        assert_eq!(largest.fragments, 5);
        assert!(
            largest.label.starts_with("CTH "),
            "самой большой названа не группа пакета: {}",
            largest.label
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
    /// тесте нет ничего, кроме манифеста, – иначе проверка не отличила бы
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

    /// **Группу без CTH называет ядро, а не эта программа.**
    ///
    /// Метка берется из `aruna::parse::MISSING` – в тесте тоже, потому что
    /// написать здесь `—` значило бы проверять, что две копии одной строки
    /// совпадают, а не что программа берет ее у разбора.
    #[test]
    fn a_group_without_a_cth_is_recognised_by_the_label_the_core_gives_it() {
        let dir = tempfile::tempdir().unwrap();
        for (group, documents) in [(aruna::parse::MISSING, 3), ("CTH 1", 1)] {
            let path = dir.path().join(group);
            fs::create_dir_all(&path).unwrap();
            for document in 0..documents {
                fs::write(path.join(format!("KBo {document}.xml")), b"<doc/>").unwrap();
            }
        }

        let stats = read_stats(dir.path()).unwrap();

        assert_eq!(stats.source, StatsSource::Walk);
        assert_eq!(stats.spread.without_cth, 3);
        assert_eq!(stats.spread.singletons, 1, "это группа CTH 1, а не вторая");
        assert_eq!(
            stats.spread.largest,
            Some(GroupSize {
                label: aruna::parse::MISSING.to_owned(),
                fragments: 3,
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
            serde_json::from_str(include_str!("../stats-sample.json")).unwrap();

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
}

// Gated on the feature as well as on `test`: the one test inside is about what
// a build *without* `e2e` registers, so under the feature the module would be
// empty and its imports unused — which `clippy --all-features` reports,
// correctly.
#[cfg(all(test, not(feature = "e2e")))]
mod tests {
    use super::*;
    use tauri::plugin::Plugin as _;

    /// **The release build carries no WebDriver, and the compiler is what says
    /// so.**
    ///
    /// Four things keep the end-to-end contour out of a release, and three of
    /// them are checked in `frontend/tests/spec-guard.test.ts` by reading the
    /// files that declare them: the two wdio crates are `optional`, they are
    /// reached only through the `e2e` feature, and their permissions live
    /// outside the directory `build.rs` scans.
    ///
    /// This is the fourth, and it is the only one that is not a reading of a
    /// declaration: without the feature, the plugins the builder actually
    /// registers are the no-ops declared above. An optional dependency that is
    /// not selected is never compiled, so a build that reaches this assertion
    /// is a build with no WebDriver server in it — and the test compiles under
    /// exactly the feature set a release uses, which is the default one.
    ///
    /// Under `--features e2e` it does not run at all: there the answer is
    /// supposed to be different, and asserting it here would only restate the
    /// `cfg` a few lines above.
    #[test]
    fn a_build_without_the_feature_registers_no_webdriver() {
        let webdriver = wdio_webdriver_plugin::<tauri::Wry>();
        let backend = wdio_plugin::<tauri::Wry>();

        assert_eq!(
            webdriver.name(),
            "noop-wdio-webdriver",
            "a release build registered a real WebDriver server"
        );
        assert_eq!(
            backend.name(),
            "noop-wdio",
            "a release build registered the wdio backend"
        );
    }
}
