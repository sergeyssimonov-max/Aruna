//! Типы IPC и преобразования данных ядра в сообщения окна. Без ввода-вывода.

/// Где лежит то, что собрала консольная часть программы.
///
/// Единственный источник этих путей – ядро: каталог загрузок отдает
/// `aruna::paths`, имя пакета и имя описи объявлены там же константами.
/// Оболочка ничего не вычисляет сама, иначе одно поведение имело бы две
/// реализации, расходящиеся при первой же правке ядра.
#[derive(serde::Serialize, specta::Type)]
pub(crate) struct CorpusLocation {
    pub(super) downloads: String,
    pub(super) package: String,
    pub(super) inventory: String,
    pub(super) package_exists: bool,
    pub(super) inventory_exists: bool,
}

/// Чем команда окна может отказать.
///
/// Ни в одном тексте нет пути файловой системы, и это правило, а не случайность
/// формулировок: §3 контракта запрещает пускать пути в сообщения для человека,
/// и до 07.09.2026 его нарушал не наш код, а плагин – `open_path` возвращал
/// `Not allowed to open path /Users/…/TLHdig_Beta_0.3.html`, по-английски и с
/// путем. Теперь опись открывает `commands::open_inventory`, и отказ приходит отсюда.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CommandError {
    #[error("не удалось определить папку загрузок")]
    Downloads,
    /// Окну назвали файл, который описью не является.
    #[error("это не опись корпуса")]
    NotInventory,
    /// Опись была на месте, когда окно узнало о ней, и исчезла до нажатия.
    #[error("описи нет на месте – соберите корпус заново")]
    InventoryGone,
    /// Система отказалась открывать документ, и почему – знает только она.
    #[error("система не открыла опись")]
    Opening,
    /// Окну назвали каталог, который пакетом не является.
    #[error("это не пакет корпуса")]
    NotPackage,
}

/// Счетчик на проводе – тридцать два бита, и это не сужение, а точное
/// объявление.
///
/// JSON несет числа как double, поэтому целое, способное перевалить за 2^53,
/// пересекает границу с потерей; specta по этой причине отказывается
/// экспортировать `usize` и `u64` вовсе, и правильный ответ на отказ – назвать
/// тот тип, которым число на самом деле является. Все числа этого окна
/// ограничены заведомо ниже: документов в архиве не бывает больше
/// `archive::MAX_ENTRIES`, то есть 500 000, групп – не больше, чем документов,
/// а байт загрузки – больше гигабайта их не примет сам загрузчик.
///
/// Насыщение, а не усечение: показать предел честнее, чем показать остаток от
/// деления.
pub(super) fn counted<T: TryInto<u32>>(value: T) -> u32 {
    value.try_into().unwrap_or(u32::MAX)
}

/// Что сборка дала.
///
/// Числа не пересчитываются: это то, что вернул сам прогон – `CorpusReport`
/// ядра, переложенный во владеющий вид. Пересчет после сборки уже однажды
/// разошелся с манифестом в этом проекте, и это отдельная строка в комментарии
/// `app::PackageReport`.
///
/// Пути здесь есть, и это не то же самое, что путь в сообщении об ошибке
/// (правило рядом, у [`StatsError`]): показать, что собрано и где оно лежит –
/// и есть работа этого окна.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct BuildReport {
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
    /// PDF сборки: `PackageReport::pdfs` ядра.
    pub pdfs: PdfReport,
}

/// Сколько PDF построено и сколько отказано: `export::PdfCount` ядра.
///
/// Одна пара, а не два поля отчета по отдельности: построенные без отказанных
/// – это половина числа. Причины отказов записаны в манифесте пакета, сюда
/// они не идут.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct PdfReport {
    pub built: u32,
    pub refused: u32,
}

/// Почему сборка не дошла до конца.
///
/// Тегированная структура, как требует §3 `docs/FRONTEND-CONTRACT.md`: вид,
/// предложение для человека, фаза и – отдельно от всего – стоит ли предлагать
/// повтор. Строкой это быть не может: у отказа двадцать девять видов – двадцать
/// шесть у ядра (`app::Failure::of`) и три у оболочки (`busy`, `interrupted`,
/// `destination_missing`). От вида зависит, что окну делать дальше.
///
/// Поля один в один повторяют `app::Failure` ядра, включая `retryable`, который
/// там не переписан заново, а делегирован клиенту загрузки: две независимые
/// формулировки того же правила в этом проекте уже расходились.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct BuildFailure {
    /// Устойчивый машинный вид: `cancelled`, `network`, `checksum`, …
    pub code: String,
    /// Фаза, на которой это случилось – `Phase::code` ядра. `null` там, где
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
    pub(super) fn of(failure: &aruna::app::Failure) -> BuildFailure {
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
    pub(super) fn shell(code: &str, message: &str, retryable: bool) -> BuildFailure {
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
/// пустое место – ровно тот отказ, ради которого `progress::Event` в ядре не
/// `#[non_exhaustive]`.
///
/// Имена принадлежат оболочке: по `docs/ARCHITECTURE.md` §7 события IPC – ее
/// собственность, а не ядра. Их двадцать одна против двадцати четырех
/// вариантов события, потому что три пары – объявление стадии и ее тик у
/// загрузки, записи документов и PDF – это одна стадия.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Stage {
    CacheUnusable,
    CachedArchiveRejected,
    ArchiveFromCache,
    ZenodoNotice,
    ZenodoUnreachable,
    /// Прокси из окружения назван, но этой программе не годится – SOCKS,
    /// `https://`, порт не числом – и запрос идет напрямую. В заметке – имя
    /// переменной, не значение: в значении бывают учетные данные. С 25.09.2026.
    ProxyUnusable,
    Downloading,
    DownloadRetrying,
    ArchiveKept,
    Parsing,
    EntriesSkipped,
    Indexed,
    ReadingHeaders,
    HeadersRead,
    Writing,
    /// PDF каждого документа строится рядом с ним. Своя стадия с 18.10.2026
    /// (part3-1): до того фаза PDF до окна не доходила вовсе.
    WritingPdfs,
    CheckingPackage,
    /// Другой запуск публикует в ту же папку, и этот его ждет. С 24.09.2026:
    /// до того ожидание шло молча, а за брошенной блокировкой – пять минут.
    WaitingForPublication,
    /// Диск не держит блокировку публикации – нет `flock` или номера файлов не
    /// стоят на месте – и прогон публикует без нее. С 25.09.2026, решение
    /// владельца: сказать, а не отказать и не ждать.
    PublishingWithoutLock,
    CheckingPublished,
    PreviousPackageLeft,
}

/// Насколько далеко зашла сборка.
///
/// Одно событие на все стадии, а не по типу на каждую: окну нужно имя стадии и,
/// где она их знает, две половины дроби. Новый показатель – поле здесь, и
/// старое окно, которое о нем не знает, продолжает работать.
///
/// **Путей не носит.** Пять вариантов `progress::Event` несут `&Path`, и по
/// правилу рядом со [`StatsError`] им сюда нельзя: окно показывают через плечо.
/// Из таких событий сюда доходит только имя стадии.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
pub(crate) struct BuildProgress {
    /// Чей это прогресс. Тот же номер приходит в [`BuildReport::job`].
    pub job: u32,
    /// На чем прогон сейчас.
    pub stage: Stage,
    /// Числитель и знаменатель, когда стадия умеет их назвать. У загрузки
    /// знаменателя может не быть: сервер не обязан объявлять длину.
    pub done: Option<u32>,
    pub total: Option<u32>,
    /// Сколько PDF построено, а не отказано. `None` – фаза PDF еще не
    /// началась. Объявление фазы несет ноль, тики – число ядра как есть, а все
    /// события после фазы – последнее число тика (`WindowProgress` его
    /// помнит). `done` и `total` тиков – тоже числа ядра: обработано и всего
    /// размещенных документов.
    pub built: Option<u32>,
    /// Сколько нашлось, когда это уже известно – окно говорит числа до того,
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
    pub(super) fn of(job: u32, event: &aruna::progress::Event<'_>) -> BuildProgress {
        use aruna::progress::Event as Core;

        let mut progress = BuildProgress {
            job,
            stage: Stage::Parsing,
            done: None,
            total: None,
            built: None,
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
            Core::ProxyUnusable { variable } => {
                progress.note = Some((*variable).to_string());
                Stage::ProxyUnusable
            }
            // Стадия объявляет знаменатель, тик заполняет числитель. Ноль в
            // начале – чтобы полоса появилась сразу, а не после первой четверти
            // секунды.
            Core::DownloadStarted => {
                progress.done = Some(0);
                Stage::Downloading
            }
            Core::Downloading { bytes, total } => {
                progress.done = Some(counted(*bytes));
                // Длина, которую этот загрузчик все равно откажется принять
                // (потолок – гигабайт), знаменателем не является: лучше
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
            Core::WritingPdfs { documents } => {
                progress.done = Some(0);
                progress.total = Some(counted(*documents));
                progress.built = Some(0);
                Stage::WritingPdfs
            }
            Core::PdfsWritten { done, built, total } => {
                progress.done = Some(counted(*done));
                progress.total = Some(counted(*total));
                progress.built = Some(counted(*built));
                Stage::WritingPdfs
            }
            Core::CheckingPackage => Stage::CheckingPackage,
            Core::WaitingForPublication => Stage::WaitingForPublication,
            Core::PublishingWithoutLock => Stage::PublishingWithoutLock,
            Core::CheckingPublished => Stage::CheckingPublished,
            Core::PreviousPackageLeft { .. } => Stage::PreviousPackageLeft,
        };
        progress
    }

    /// Это событие с числом построенных PDF, начиная с объявления фазы: свое
    /// число оно запоминает в `last`, а событие без числа получает оттуда
    /// последнее. До фазы `last` пуст, и `built` остается `None`.
    pub(super) fn carrying(mut self, last: &mut Option<u32>) -> BuildProgress {
        match self.built {
            Some(built) => *last = Some(built),
            None => self.built = *last,
        }
        self
    }
}

/// Сколько в собранном пакете рукописей и групп.
///
/// Числа окно показывает как есть, поэтому здесь они уже такие, какими их надо
/// показать: пересчета на стороне окна нет.
#[derive(Debug, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct CorpusStats {
    pub(crate) manuscripts: u32,
    pub(crate) groups: u32,
    pub(crate) source: StatsSource,
    /// Как фрагменты разложены по группам, а не только сколько их всего.
    pub(crate) spread: Spread,
    /// Что манифест насчитал о письме корпуса.
    ///
    /// `None`, когда числа взяты обходом каталога: эти счетчики получены при
    /// разборе документов и по разложенному пакету не восстанавливаются.
    /// Пустая структура из нулей соврала бы – ноль документов вне NFC и
    /// «неизвестно, сколько их» на экране выглядят одинаково, а значат разное.
    pub(crate) fonts: Option<Fonts>,
}

/// Разложение фрагментов по группам CTH.
///
/// Два итога – сколько фрагментов и сколько групп – ничего не говорят о том,
/// как одно распределено по другому, а распределение здесь крайне неровное:
/// в самой большой группе больше фрагментов, чем в четырех сотнях самых
/// маленьких вместе.
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct Spread {
    /// Самая большая группа. `None` – в пакете нет ни одной.
    pub(crate) largest: Option<GroupSize>,
    /// Групп ровно с одним фрагментом.
    pub(crate) singletons: u32,
    /// Фрагменты, у которых CTH нет.
    ///
    /// Экспорт кладет их в группу с меткой `aruna::parse::MISSING`, и метка
    /// берется у ядра, а не пишется здесь строкой: она принадлежит разбору, и
    /// вторая ее копия разошлась бы с первой молча.
    pub(crate) without_cth: u32,
}

/// Группа и ее размер.
#[derive(Debug, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct GroupSize {
    pub(crate) label: String,
    pub(crate) fragments: u32,
}

/// Что манифест насчитал о письме корпуса при разборе.
#[derive(Debug, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct Fonts {
    /// Документы, чей текст пришел не в нормальной форме C.
    pub(crate) not_in_nfc: u32,
    /// Документы, где встречаются кодовые точки из области частного
    /// использования.
    pub(crate) with_private_use: u32,
    /// Сколько таких точек различают во всем корпусе.
    pub(crate) private_use_points: u32,
    /// Аномалии письма – все шесть счетчиков манифеста одним числом.
    pub(crate) anomalies: u32,
}

/// Что разборщик сказал о документах пакета.
///
/// **Ни один документ по этим сведениям из пакета не исключен.** Пакет –
/// побайтовое зеркало корпуса, копированию разборщик не нужен, и все 23 936
/// документов в нем лежат. Некорректность разметки – свойство исходных данных,
/// оно мешает превращению документа в PDF, а не его хранению.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct XmlSummary {
    /// Документов в пакете – все, и прочитанные, и нет.
    pub(crate) documents: u32,
    /// Из них программа прочитала.
    ///
    /// Не «корректный XML»: среди прочитанных есть и те, что XML нарушают
    /// (голый `<` в значении атрибута), и те, что нарушают пространства имен.
    pub(crate) read: u32,
    /// Из них программа не прочитала: разметка нарушена внутри текста.
    ///
    /// **Последствия для читателя это число само по себе не несет.** Модель
    /// документа отвергает не только их, но и семнадцать, которые разборщик
    /// принимает, – число с последствием стоит в [`XmlSummary::objected_to`].
    /// До 13.09.2026 окно ставило крупным это, с подписью «не будут
    /// преобразованы», принадлежащей тому. О правилах XML оно тоже не
    /// говорит: четыре прочитанных документа нарушают XML все равно, а
    /// тринадцать других не нарушают его вовсе.
    ///
    /// **Имя на проводе и ключ в манифесте разошлись нарочно.** В манифесте то
    /// же число лежит под ключом `not_well_formed`, и это имя неверно тем же
    /// способом, каким была неверна подпись в окне: 206 – отказы нашего
    /// разборщика, а не нарушение стандарта. Ключ манифеста – часть формата
    /// пакета: его правка двигает сумму пакета и ломает читателей, которые по
    /// нему ходят, поэтому она отдельное решение владельца. Провод наш, и здесь
    /// имя исправлено 13.09.2026.
    pub(crate) unread: u32,
    /// По причинам, включая те, у которых ноль.
    ///
    /// Ноль перечислен нарочно – он отличает «искали и не нашли» от «не
    /// искали», и в манифесте это различие есть. На экран нулевые причины окно
    /// не выносит: там строка «ноль документов» читается как найденная беда.
    /// Провод несет полный список, показывать из него – решение окна.
    pub(crate) reasons: Vec<XmlReasonCount>,
    /// Имена непрочитанных, в порядке манифеста.
    pub(crate) documents_not_well_formed: Vec<XmlDocument>,
    /// Документов, которые программа читает, а стандарты их не допускают.
    ///
    /// Вторая половина того же вопроса, и до 10.09.2026 ее не считал никто.
    /// Числа выше говорят, что отказал разборщик ядра; это – что он пропустил.
    /// В пакете 2026-09-10 их семнадцать: четыре с голым `<` внутри значения
    /// атрибута – эти нарушают XML – и тринадцать с именем вида `<AO:-…>`, у
    /// которого нет локальной части: эти корректны как XML и нарушают
    /// пространства имен.
    pub(crate) beyond_this_parser: u32,
    /// По классам предела, включая класс с нулем.
    pub(crate) limits: Vec<XmlLimitCount>,
    /// Имена этих документов, в порядке манифеста.
    pub(crate) documents_beyond_this_parser: Vec<XmlLimitDocument>,
    /// Нарушают правила XML: непрочитанные плюс четыре с голым `<`.
    ///
    /// То самое число, на котором `xmllint --noout` выходит с ненулевым кодом:
    /// 210 в пакете 2026-09-10.
    pub(crate) not_well_formed_xml: u32,
    /// Нарушают правила XML либо правила пространств имен: 223 в том же пакете.
    ///
    /// **Единственное число с последствием для читателя** – эти документы не
    /// попадут в PDF. Это ровно те, которые отвергает модель документа
    /// (`aruna::document::Document::read`), и совпадение не счетом, а
    /// поименно: модель отвергает документ тогда и только тогда, когда его
    /// называет один из двух списков манифеста, и `cli/tests/document_model.rs`
    /// сверяет это по всему корпусу. Пакет, собранный до 10.09.2026, этого
    /// числа не несет – здесь ноль, и окно последствия тогда не заявляет.
    ///
    /// **Тринадцать из них – корректный XML.** Двоеточие и дефис входят в
    /// состав имени по XML 1.0, поэтому `AO:-LineNrExpl` – законное `Name`;
    /// нарушены у них «Пространства имен в XML», отдельный стандарт, и libxml
    /// зовет это `namespace error` и выходит с нулевым кодом. Слово
    /// «некорректный XML» к этому числу неприменимо – оно применимо к
    /// [`XmlSummary::not_well_formed_xml`].
    pub(crate) objected_to: u32,
}

/// Сколько документов у одной причины.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct XmlReasonCount {
    /// Ключ причины, как его пишет манифест.
    pub(crate) reason: String,
    pub(crate) documents: u32,
}

/// Один непрочитанный документ и место первой ошибки.
///
/// «Непрочитанный», а не «некорректный»: отказ принадлежит нашему разборщику, и
/// о правилах XML сам по себе не говорит – см. [`XmlSummary::unread`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct XmlDocument {
    /// Путь внутри пакета.
    pub(crate) file: String,
    pub(crate) reason: String,
    pub(crate) line: u32,
    pub(crate) column: u32,
}

/// Сколько документов у одного класса предела.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct XmlLimitCount {
    /// Ключ класса, как его пишет манифест.
    pub(crate) limit: String,
    pub(crate) documents: u32,
}

/// Один документ, который программа прочитала, а стандарт его не допускает.
///
/// Отдельный тип, а не [`XmlDocument`] с переименованным полем: там `reason` –
/// причина отказа, здесь `limit` – класс того, чего разборщик не увидел. Одно
/// имя на двоих читалось бы как одно и то же событие, а это разные события.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct XmlLimitDocument {
    /// Путь внутри пакета.
    pub(crate) file: String,
    pub(crate) limit: String,
    pub(crate) line: u32,
    pub(crate) column: u32,
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
pub(crate) enum StatsSource {
    Manifest,
    Walk,
}
