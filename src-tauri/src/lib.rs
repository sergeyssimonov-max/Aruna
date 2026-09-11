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

/// Чем команда окна может отказать.
///
/// Ни в одном тексте нет пути файловой системы, и это правило, а не случайность
/// формулировок: §3 контракта запрещает пускать пути в сообщения для человека,
/// и до 07.09.2026 его нарушал не наш код, а плагин — `open_path` возвращал
/// `Not allowed to open path /Users/…/TLHdig_Beta_0.3.html`, по-английски и с
/// путем. Теперь опись открывает [`open_inventory`], и отказ приходит отсюда.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("не удалось определить папку загрузок")]
    Downloads,
    /// Окну назвали файл, который описью не является.
    #[error("это не опись корпуса")]
    NotInventory,
    /// Опись была на месте, когда окно узнало о ней, и исчезла до нажатия.
    #[error("описи нет на месте – соберите корпус заново")]
    InventoryGone,
    /// Система отказалась открывать документ, и почему — знает только она.
    #[error("система не открыла опись")]
    Opening,
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

/// Что разборщик сказал о документах пакета.
///
/// **Ни один документ по этим сведениям из пакета не исключен.** Пакет –
/// побайтовое зеркало корпуса, копированию разборщик не нужен, и все 23 936
/// документов в нем лежат. Некорректность разметки – свойство исходных данных,
/// оно мешает превращению документа в PDF, а не его хранению.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct XmlSummary {
    /// Документов в пакете – все, и корректные, и нет.
    documents: u32,
    /// Из них корректный XML.
    well_formed: u32,
    /// Из них не корректный XML.
    not_well_formed: u32,
    /// По причинам, включая те, у которых ноль.
    ///
    /// Ноль перечислен нарочно – он отличает «искали и не нашли» от «не
    /// искали», и в манифесте это различие есть. На экран нулевые причины окно
    /// не выносит: там строка «ноль документов» читается как найденная беда.
    /// Провод несет полный список, показывать из него – решение окна.
    reasons: Vec<XmlReasonCount>,
    /// Имена некорректных, в порядке манифеста.
    documents_not_well_formed: Vec<XmlDocument>,
    /// Документов, которые этот разборщик принимает, а строгий – нет.
    ///
    /// Вторая половина того же вопроса, и до 10.09.2026 ее не считал никто.
    /// Числа выше говорят, что отказал разборщик ядра; это – что он пропустил.
    /// В пакете 2026-09-10 их семнадцать: четыре с голым `<` внутри значения
    /// атрибута и тринадцать с именем вида `<AO:-…>`, у которого нет локальной
    /// части.
    beyond_this_parser: u32,
    /// По классам предела, включая класс с нулем.
    limits: Vec<XmlLimitCount>,
    /// Имена этих документов, в порядке манифеста.
    documents_beyond_this_parser: Vec<XmlLimitDocument>,
    /// Некорректный XML как таковой: отказы ядра плюс голый `<`.
    ///
    /// То самое число, на котором `xmllint --noout` выходит с ненулевым кодом:
    /// 210 в пакете 2026-09-10.
    not_well_formed_xml: u32,
    /// Все, к чему придирается строгий разборщик: 223 в том же пакете.
    objected_to: u32,
}

/// Сколько документов у одной причины.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct XmlReasonCount {
    /// Ключ причины, как его пишет манифест.
    reason: String,
    documents: u32,
}

/// Один некорректный документ и место первой ошибки.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct XmlDocument {
    /// Путь внутри пакета.
    file: String,
    reason: String,
    line: u32,
    column: u32,
}

/// Сколько документов у одного класса предела.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct XmlLimitCount {
    /// Ключ класса, как его пишет манифест.
    limit: String,
    documents: u32,
}

/// Один документ, который разборщик ядра принял, а строгий – нет.
///
/// Отдельный тип, а не [`XmlDocument`] с переименованным полем: там `reason` –
/// причина отказа, здесь `limit` – класс того, чего разборщик не увидел. Одно
/// имя на двоих читалось бы как одно и то же событие, а это разные события.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct XmlLimitDocument {
    /// Путь внутри пакета.
    file: String,
    limit: String,
    line: u32,
    column: u32,
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

/// Почему сводку о разметке взять не удалось.
///
/// Обхода каталога в запасе нет, и это не упущение: разбор 23 936 документов
/// заново – работа на секунды, а числа уже сосчитаны тем же прогоном, который
/// раскладывал файлы. Манифест здесь единственный источник, и когда его нет,
/// честный ответ – сказать это, а не пересчитать чужую работу второй раз и
/// другим кодом.
#[derive(Debug, thiserror::Error)]
pub enum XmlSummaryError {
    #[error("пакет по этому пути не найден")]
    Missing,
    #[error("манифест пакета не читается")]
    Unreadable,
    #[error("манифест пакета не содержит сведений о разметке")]
    Absent,
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
/// Разбирать в окне нечего ни у читающих команд, ни у открытия описи: отказ
/// показывается целиком, как он написан, и ветвиться по нему окну незачем.
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

/// Открыть опись тем, чем читатель обычно открывает HTML.
///
/// Открыть можно ровно один файл — опись, чье имя объявлено ядром; лежать она
/// при этом может где угодно, потому что «Собрать в папку…» кладет ее туда,
/// куда указал человек. Путь приходит от окна, как у [`corpus_stats`] и
/// [`corpus_xml`]: из [`corpus_location`] или из `BuildReport` той сборки,
/// которая его и написала.
// Комментарии ниже намеренно обычные, а не доксрока: доксроки команд specta
// переносит в `bindings.ts`, и объяснение, адресованное этому файлу, уехало бы
// в продукт — то же правило, что у `corpus_stats` про поток.
//
// **Почему это команда оболочки, а не вызов плагина из окна.** До 07.09.2026
// окно звало `openPath` плагина `opener` напрямую, и кнопка не работала ни в
// одной выпущенной сборке. Разрешение `opener:allow-open-path` включает
// команду, но не наполняет ее область путей — так и написано в самом плагине:
// «enables the open_path command without any pre-configured scope». Область
// осталась пустой, `is_path_allowed` вернул `false` обоими своими условиями, и
// на экран легло `Not allowed to open path …`. Наполнить область было нечем:
// область, разрешающая любой путь, — это отмена области, а не ее настройка.
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
/// Два условия, и второе не лишнее: окно узнает об описи заранее — при чтении
/// папки или из отчета сборки, — а нажимают на кнопку позже, и между тем и
/// другим файл могли убрать.
fn named_inventory(path: &std::path::Path) -> Result<(), CommandError> {
    if path.file_name() != Some(std::ffi::OsStr::new(aruna::paths::OUTPUT_FILE_NAME)) {
        return Err(CommandError::NotInventory);
    }
    if !path.is_file() {
        return Err(CommandError::InventoryGone);
    }
    Ok(())
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

/// Что разборщик сказал о документах пакета, лежащего по этому пути.
///
/// Путь приходит от окна из [`corpus_location`], как и у [`corpus_stats`].
// `async` по той же причине, что у `corpus_stats`: манифест – без малого
// девять мегабайт разбора, и на главном потоке это подвешивает окно ровно на
// свою длительность.
#[tauri::command(async)]
#[specta::specta]
fn corpus_xml(path: String) -> Result<XmlSummary, String> {
    read_xml_summary(std::path::Path::new(&path)).map_err(said)
}

/// Команда без Tauri, чтобы ветки проверялись тестом.
///
/// Один источник – манифест, и обхода в запасе нет намеренно: числа сосчитал
/// тот же прогон, который раскладывал файлы, а второй счет другим кодом – это
/// второе поведение, расходящееся с первым при первой же правке.
fn read_xml_summary(package: &std::path::Path) -> Result<XmlSummary, XmlSummaryError> {
    /// Из всего манифеста окну нужна одна секция; остальные поля serde
    /// пропускает.
    #[derive(serde::Deserialize)]
    struct Manifest {
        xml: Option<XmlSection>,
    }

    #[derive(serde::Deserialize)]
    struct XmlSection {
        documents: usize,
        well_formed: usize,
        not_well_formed: usize,
        #[serde(default)]
        by_reason: std::collections::BTreeMap<String, usize>,
        #[serde(default)]
        not_well_formed_documents: Vec<Entry>,
        /// Секцию манифест несет с 10.09.2026. Пакет, собранный раньше, ее не
        /// имеет, и это не поломка: `default` дает нули и пустые списки, окно
        /// показывает то, что есть. Отдельного отказа тут не нужно – в отличие
        /// от секции `xml` целиком, отсутствие которой значит «пакет об этом
        /// не знает», здесь известно все, кроме второй половины.
        #[serde(default)]
        beyond_this_parser: Option<BeyondSection>,
        #[serde(default)]
        totals: Option<Totals>,
    }

    #[derive(serde::Deserialize, Default)]
    struct BeyondSection {
        #[serde(default)]
        documents: usize,
        #[serde(default)]
        by_limit: std::collections::BTreeMap<String, usize>,
        #[serde(default)]
        documents_beyond_this_parser: Vec<LimitEntry>,
    }

    #[derive(serde::Deserialize, Default)]
    struct Totals {
        #[serde(default)]
        not_well_formed_xml: Total,
        #[serde(default)]
        objected_to_by_a_conforming_parser: Total,
    }

    #[derive(serde::Deserialize, Default)]
    struct Total {
        #[serde(default)]
        documents: usize,
    }

    #[derive(serde::Deserialize)]
    struct Entry {
        file: String,
        reason: String,
        line: usize,
        column: usize,
    }

    #[derive(serde::Deserialize)]
    struct LimitEntry {
        file: String,
        limit: String,
        line: usize,
        column: usize,
    }

    if !package.is_dir() {
        return Err(XmlSummaryError::Missing);
    }
    let text = std::fs::read_to_string(package.join(aruna::export::MANIFEST))
        .map_err(|_| XmlSummaryError::Unreadable)?;
    let manifest: Manifest =
        serde_json::from_str(&text).map_err(|_| XmlSummaryError::Unreadable)?;
    // Манифест старого пакета секции не несет, и это не поломка: собран он был
    // до 06.09.2026. Отдельный отказ, а не нули, – ноль некорректных документов
    // и «пакет об этом не знает» на экране выглядят одинаково, а значат разное.
    let mut section = manifest.xml.ok_or(XmlSummaryError::Absent)?;
    let beyond = section.beyond_this_parser.take().unwrap_or_default();
    let totals = section.totals.take().unwrap_or_default();

    Ok(XmlSummary {
        documents: counted(section.documents),
        well_formed: counted(section.well_formed),
        not_well_formed: counted(section.not_well_formed),
        reasons: section
            .by_reason
            .into_iter()
            .map(|(reason, documents)| XmlReasonCount {
                reason,
                documents: counted(documents),
            })
            .collect(),
        documents_not_well_formed: section
            .not_well_formed_documents
            .into_iter()
            .map(|entry| XmlDocument {
                file: entry.file,
                reason: entry.reason,
                line: counted(entry.line),
                column: counted(entry.column),
            })
            .collect(),
        beyond_this_parser: counted(beyond.documents),
        limits: beyond
            .by_limit
            .into_iter()
            .map(|(limit, documents)| XmlLimitCount {
                limit,
                documents: counted(documents),
            })
            .collect(),
        documents_beyond_this_parser: beyond
            .documents_beyond_this_parser
            .into_iter()
            .map(|entry| XmlLimitDocument {
                file: entry.file,
                limit: entry.limit,
                line: counted(entry.line),
                column: counted(entry.column),
            })
            .collect(),
        // Пакет старее 10.09.2026 итогов не несет, и складывать их здесь
        // самим нельзя: 210 – это отказы плюс один из двух классов предела, а
        // какой именно, знает ядро, а не окно. Ноль честнее выдуманного числа,
        // и экран на нем ничего не печатает.
        not_well_formed_xml: counted(totals.not_well_formed_xml.documents),
        objected_to: counted(totals.objected_to_by_a_conforming_parser.documents),
    })
}

/// Команда без Tauri, чтобы обе ветки проверялись тестом.
///
/// Манифест – первый источник, потому что его пишет тот же экспорт, который
/// раскладывал файлы: это его собственный счет, а не пересчет чужой работы.
/// Обход каталога – ответ на случай, когда манифеста нет или он не тот:
/// испорченный JSON и манифест без `counts` ведут туда же, куда его отсутствие,
/// потому что для окна это одно и то же положение – числа надо взять с диска.
///
/// **Обход делает ядро.** До 06.09.2026 он был написан здесь и выводил
/// раскладку пакета второй раз – группа есть подкаталог, рукопись есть `.xml`
/// внутри, – хотя задает ее экспорт. Спецификация 4.9.6 такого не разрешает:
/// оболочка не заводит собственных путей к данным корпуса. Выбор между двумя
/// источниками остался здесь, потому что это и есть работа обертки; сами числа
/// считает `aruna::export::count_package`.
fn read_stats(package: &std::path::Path) -> Result<CorpusStats, StatsError> {
    if let Some(stats) = counts_from_manifest(package) {
        return Ok(stats);
    }
    let walked = aruna::export::count_package(package).map_err(|err| match err {
        aruna::export::CountError::NotAPackage => StatsError::Missing,
        aruna::export::CountError::Read(err) => StatsError::Read(err.to_string()),
    })?;
    Ok(CorpusStats {
        manuscripts: counted(walked.documents),
        groups: counted(walked.groups),
        source: StatsSource::Walk,
        spread: wire_spread(walked.spread),
        // Обход считает файлы, а не читает документы: как написан их текст, он
        // не знает и знать не может.
        fonts: None,
    })
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
        spread: wire_spread(aruna::export::spread(
            manifest
                .groups
                .into_iter()
                .map(|group| (group.label, group.documents.len())),
        )),
        fonts: manifest.fonts.map(|fonts| Fonts {
            not_in_nfc: counted(fonts.documents_not_in_nfc),
            with_private_use: counted(fonts.documents_with_private_use),
            private_use_points: counted(fonts.private_use_points.len()),
            anomalies: counted(fonts.anomalies.values().sum::<usize>()),
        }),
    })
}

/// Разбивка ядра, переложенная в то, что уходит в окно.
///
/// Считает ее `aruna::export::spread` – одна функция на оба источника, потому
/// что вопросы к перечню групп не зависят от того, кто этот перечень составил:
/// манифест списком своих групп или обход каталогами на диске. Здесь остается
/// приведение к объявленной на проводе форме – `u32` вместо `usize`, – и
/// больше ничего: арифметики в оболочке нет.
fn wire_spread(spread: aruna::export::Spread) -> Spread {
    Spread {
        largest: spread.largest.map(|group| GroupSize {
            label: group.label,
            fragments: counted(group.fragments),
        }),
        singletons: counted(spread.singletons),
        without_cth: counted(spread.without_cth),
    }
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
    pub documents: u32,
    pub groups: u32,
    /// Документы, которым пришлось дать суффикс: их сиглум был уже занят.
    pub disambiguated: u32,
    /// Убранные ссылки на таблицу стилей, которой в пакете нет.
    ///
    /// Инструкции, а не документы: один документ корпуса несет две. До
    /// 10.09.2026 число считалось по документам, а называлось инструкциями –
    /// на корпусе это ровно единица разницы, 8 424 против 8 423.
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

/// Папка, выбранная человеком, — проверенная здесь, а не там, где в нее пишут.
///
/// Окно файловых ручек не получает и путей не толкует: строка приходит с той
/// стороны, и первое, что с ней делается, — проверка, что за ней есть каталог.
/// Отказ на этом месте — предложение выбрать другую папку, а не ошибка сборки,
/// которой не было. `None` означает папку загрузок, то есть поведение консоли.
fn chosen_destination(
    destination: Option<String>,
) -> Result<Option<std::path::PathBuf>, BuildFailure> {
    match destination {
        Some(given) => {
            let path = std::path::PathBuf::from(given);
            if !path.is_dir() {
                return Err(BuildFailure::shell(
                    "destination_missing",
                    "выбранной папки нет на месте",
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
/// Две оси, и они разные. **Источник один** — закрепленная запись Zenodo через
/// кеш, то есть ровно то, что делает консольный бинарь: архива команда не
/// принимает вовсе, решением владельца 06.09.2026. **Назначение выбирается:**
/// `destination` — папка, названная человеком, `null` — папка загрузок. Ядро
/// умело это с самого начала, `app::build_corpus_into`; окно до него не
/// дотягивалось. Путь приходит строкой и проверяется здесь: окно файловых ручек
/// не получает (§3 контракта).
///
/// Работа идет не в главном потоке. Сборка — это от шести секунд до минуты с
/// лишним, а команда на главном потоке заморозила бы webview и заодно все
/// последующие вызовы, включая отмену.
#[tauri::command]
#[specta::specta]
async fn build_corpus(
    app: tauri::AppHandle,
    state: tauri::State<'_, Building>,
    destination: Option<String>,
) -> Result<BuildReport, BuildFailure> {
    let chosen = chosen_destination(destination)?;
    let cancel = aruna::job::Cancel::new();
    state.claim(cancel.clone())?;

    let handle = app.clone();
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
        // Ядро умеет читать архив с диска, окно этой возможностью не
        // пользуется: `None` — закрепленная запись Zenodo через кеш.
        let request = aruna::app::CorpusRequest {
            local_archive: None,
        };
        // Две ветки, а не одна с подстановкой умолчания: место, где спрашивают
        // у платформы про папку загрузок, обязано остаться единственным, и оно
        // внутри `app::build_corpus`.
        match &chosen {
            Some(folder) => aruna::app::build_corpus_into(&request, folder, &job),
            None => aruna::app::build_corpus(&request, &job),
        }
        .map(|report| BuildReport {
            job: counted(report.job.get()),
            package: report.package.root.display().to_string(),
            inventory: report.inventory.display().to_string(),
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
            corpus_xml,
            open_inventory,
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
            "у buildCorpus нет аргумента назначения: окно не может выбрать папку"
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
    /// той стороны, и первое, что с ней делается, – проверка, что за ней есть
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
    }
}

// Проверка имени описи к фиче отношения не имеет, поэтому модуль закрыт только
// `test`: обе отказные ветки проверяются и в сборке с `e2e`, и без нее.
#[cfg(test)]
mod opening {
    use super::{named_inventory, CommandError};
    use std::fs;

    /// Опись — это файл с тем именем, которое объявило ядро, и лежать он может
    /// где угодно: «Собрать в папку…» кладет его в папку, названную человеком.
    #[test]
    fn the_inventory_is_the_file_the_core_names_wherever_it_lies() {
        let dir = tempfile::tempdir().unwrap();
        let inventory = dir.path().join(aruna::paths::OUTPUT_FILE_NAME);
        fs::write(&inventory, b"<html></html>").unwrap();

        assert!(named_inventory(&inventory).is_ok());
    }

    /// **Ничего, кроме описи, эта команда не открывает.**
    ///
    /// Та граница, которую должна была дать область путей плагина и не дала:
    /// сосед описи по каталогу — уже не опись.
    #[test]
    fn nothing_but_the_inventory_is_opened() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("TLHdig_Beta_0.3.zip");
        fs::write(&other, b"not the inventory").unwrap();

        assert!(matches!(
            named_inventory(&other),
            Err(CommandError::NotInventory)
        ));
    }

    /// Окно узнает об описи заранее, а нажимают позже: между тем и другим файл
    /// могли убрать, и это отдельный отказ, а не «не опись».
    #[test]
    fn an_inventory_that_left_between_the_reading_and_the_click_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join(aruna::paths::OUTPUT_FILE_NAME);

        assert!(matches!(
            named_inventory(&gone),
            Err(CommandError::InventoryGone)
        ));
    }

    /// **Ни в одном отказе команды нет пути файловой системы.**
    ///
    /// Правило §3 контракта, и до 07.09.2026 его нарушал плагин, а не наш код:
    /// `Not allowed to open path /Users/…/TLHdig_Beta_0.3.html` — по-английски
    /// и с путем. Проверка держит уже свои тексты, чтобы правило не ушло вместе
    /// с тем, кто его нарушал.
    #[test]
    fn no_command_failure_carries_a_path() {
        for failure in [
            CommandError::Downloads,
            CommandError::NotInventory,
            CommandError::InventoryGone,
            CommandError::Opening,
        ] {
            let said = failure.to_string();
            assert!(
                !said.contains(std::path::MAIN_SEPARATOR),
                "отказ «{said}» несет путь"
            );
        }
    }
}

// Разбор манифеста к фиче отношения не имеет, поэтому модуль закрыт только
// `test`: ветки проверяются и в сборке с `e2e`, и без нее.
#[cfg(test)]
mod markup {
    use super::{read_xml_summary, XmlSummaryError};
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
        assert_eq!(summary.well_formed, 2);
        assert_eq!(summary.not_well_formed, 2);
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

        assert_eq!(summary.not_well_formed, 2, "она читается как раньше");
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
        assert_eq!(summed, summary.not_well_formed);
        assert_eq!(
            summary.well_formed + summary.not_well_formed,
            summary.documents
        );
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

    /// **Без манифеста числа берутся обходом, и обход делает ядро.**
    ///
    /// Что именно считается группой и что рукописью, проверено там, где это
    /// написано, – `aruna::export::counts`. Здесь проверяется работа обертки:
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
