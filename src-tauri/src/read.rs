//! Сторона чтения окна: что окно узнает о готовом пакете – числа корпуса и
//! сказанное разборщиком о документах, – из `manifest.json` или обходом
//! каталога, когда манифест молчит.
//!
//! Команды находятся в `commands`, типы ответов – в `dto`. Чтение пакета
//! использует имена и правила ядра; обход каталога не подменяет XML-сводку.

use crate::dto::{
    counted, CommandError, CorpusStats, Fonts, GroupSize, Spread, StatsSource, XmlDocument,
    XmlLimitCount, XmlLimitDocument, XmlReasonCount, XmlSummary,
};

/// Почему числа получить не удалось.
///
/// Путей в сообщениях нет намеренно – тем же правилом живет ядро: адрес файла
/// в тексте ошибки попадает в окно, а окно показывают через плечо.
#[derive(Debug, thiserror::Error)]
pub(crate) enum StatsError {
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
pub(crate) enum XmlSummaryError {
    #[error("пакет по этому пути не найден")]
    Missing,
    #[error("манифест пакета не читается")]
    Unreadable,
    #[error("манифест пакета не содержит сведений о разметке")]
    Absent,
}

/// Граница двух читающих команд: каталог с тем именем, которое объявило ядро.
///
/// Сестра [`named_inventory`]. Путь приходит от окна, и до 22.09.2026 команды
/// читали `manifest.json` в любом каталоге, названном строкой, – приемочный
/// аудит 21.09 записал это находкой. Допустимые имена изданий и прежних пакетов задает ядро.
///
/// Есть ли каталог на месте, здесь не спрашивается: на это у читателей свой
/// отказ, `Missing`, и окно различает «не пакет» и «пакета нет».
pub(crate) fn named_package(path: &std::path::Path) -> Result<(), CommandError> {
    if !path
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(aruna::editions::is_package_name)
    {
        return Err(CommandError::NotPackage);
    }
    // И не ссылка под этим именем: она увела бы чтение манифеста куда угодно.
    // Ядро пакет ссылкой не пишет, и его собственная проверка назначения
    // ссылку под этим именем отвергает тоже.
    if is_link(path) {
        return Err(CommandError::NotPackage);
    }
    Ok(())
}

/// Символьная ли ссылка стоит под этим именем – сама, а не то, на что она ведет.
///
/// Путь сначала пересобирается из составных частей: `lstat` пути с косой чертой
/// на конце идет по ссылке, а `file_name` ту черту не видит, так что
/// `…/TLHdig_Beta_0.3/` проходил и как пакет, и как не ссылка (ревью 25.09).
pub(crate) fn is_link(path: &std::path::Path) -> bool {
    let path: std::path::PathBuf = path.components().collect();
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
}

/// Предел манифеста, как у проверки пакета в ядре: 64 МиБ при настоящих девяти.
pub(crate) const MANIFEST_LIMIT: u64 = 64 * 1024 * 1024;

/// Текст манифеста пакета – только обычного файла и не больше предела.
///
/// `read_to_string` шел за ссылкой и читал без конца: манифест-ссылка на
/// `/dev/zero` или именованный канал вместо него держали команду вечно
/// (ревью 25.09). Вид проверяется до открытия – открытие канала на чтение
/// само ждет пишущего.
pub(crate) fn read_manifest(package: &std::path::Path) -> Option<String> {
    use std::io::Read as _;
    let path = package.join(aruna::export::MANIFEST);
    let meta = std::fs::symlink_metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > MANIFEST_LIMIT {
        return None;
    }
    let mut text = String::new();
    std::fs::File::open(&path)
        .ok()?
        .take(MANIFEST_LIMIT + 1)
        .read_to_string(&mut text)
        .ok()?;
    (text.len() as u64 <= MANIFEST_LIMIT).then_some(text)
}

/// Секция `xml` манифеста – то, что из него читает окно.
mod manifest_xml {
    /// Из всего манифеста окну нужна одна секция; остальные поля serde
    /// пропускает.
    #[derive(serde::Deserialize)]
    pub(super) struct Manifest {
        pub(super) xml: Option<XmlSection>,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct XmlSection {
        pub(super) documents: usize,
        pub(super) well_formed: usize,
        pub(super) not_well_formed: usize,
        #[serde(default)]
        pub(super) by_reason: std::collections::BTreeMap<String, usize>,
        #[serde(default)]
        pub(super) not_well_formed_documents: Vec<Entry>,
        /// Секцию манифест несет с 10.09.2026. Пакет, собранный раньше, ее не
        /// имеет, и это не поломка: `default` дает нули и пустые списки, окно
        /// показывает то, что есть. Отдельного отказа тут не нужно – в отличие
        /// от секции `xml` целиком, отсутствие которой значит «пакет об этом
        /// не знает», здесь известно все, кроме второй половины.
        #[serde(default)]
        pub(super) beyond_this_parser: Option<BeyondSection>,
        #[serde(default)]
        pub(super) totals: Option<Totals>,
    }

    #[derive(serde::Deserialize, Default)]
    pub(super) struct BeyondSection {
        #[serde(default)]
        pub(super) documents: usize,
        #[serde(default)]
        pub(super) by_limit: std::collections::BTreeMap<String, usize>,
        #[serde(default)]
        pub(super) documents_beyond_this_parser: Vec<LimitEntry>,
    }

    #[derive(serde::Deserialize, Default)]
    pub(super) struct Totals {
        #[serde(default)]
        pub(super) not_well_formed_xml: Total,
        #[serde(default)]
        pub(super) objected_to_by_a_conforming_parser: Total,
    }

    #[derive(serde::Deserialize, Default)]
    pub(super) struct Total {
        #[serde(default)]
        pub(super) documents: usize,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct Entry {
        pub(super) file: String,
        pub(super) reason: String,
        pub(super) line: usize,
        pub(super) column: usize,
    }

    #[derive(serde::Deserialize)]
    pub(super) struct LimitEntry {
        pub(super) file: String,
        pub(super) limit: String,
        pub(super) line: usize,
        pub(super) column: usize,
    }
}

use manifest_xml::{BeyondSection, Manifest, Totals, XmlSection};

/// Секция `xml` манифеста – в форме провода.
fn summary(section: XmlSection, beyond: BeyondSection, totals: Totals) -> XmlSummary {
    XmlSummary {
        documents: counted(section.documents),
        read: counted(section.well_formed),
        unread: counted(section.not_well_formed),
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
    }
}

/// Команда без Tauri, чтобы ветки проверялись тестом.
///
/// Один источник – манифест, и обхода в запасе нет намеренно: числа сосчитал
/// тот же прогон, который раскладывал файлы, а второй счет другим кодом – это
/// второе поведение, расходящееся с первым при первой же правке.
pub(crate) fn read_xml_summary(package: &std::path::Path) -> Result<XmlSummary, XmlSummaryError> {
    if !package.is_dir() {
        return Err(XmlSummaryError::Missing);
    }
    let text = read_manifest(package).ok_or(XmlSummaryError::Unreadable)?;
    let manifest: Manifest =
        serde_json::from_str(&text).map_err(|_| XmlSummaryError::Unreadable)?;
    // Манифест старого пакета секции не несет, и это не поломка: собран он был
    // до 06.09.2026. Отдельный отказ, а не нули, – ноль некорректных документов
    // и «пакет об этом не знает» на экране выглядят одинаково, а значат разное.
    let mut section = manifest.xml.ok_or(XmlSummaryError::Absent)?;
    let beyond = section.beyond_this_parser.take().unwrap_or_default();
    let totals = section.totals.take().unwrap_or_default();

    Ok(summary(section, beyond, totals))
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
pub(crate) fn read_stats(package: &std::path::Path) -> Result<CorpusStats, StatsError> {
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
pub(crate) fn counts_from_manifest(package: &std::path::Path) -> Option<CorpusStats> {
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

    let text = read_manifest(package)?;
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
pub(crate) fn wire_spread(spread: aruna::export::Spread) -> Spread {
    Spread {
        largest: spread.largest.map(|group| GroupSize {
            label: group.label,
            fragments: counted(group.fragments),
        }),
        singletons: counted(spread.singletons),
        without_cth: counted(spread.without_cth),
    }
}
