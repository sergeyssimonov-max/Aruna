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
#[derive(serde::Serialize)]
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

impl serde::Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// Сколько в собранном пакете рукописей и групп.
///
/// Числа окно показывает как есть, поэтому здесь они уже такие, какими их надо
/// показать: пересчета на стороне окна нет.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct CorpusStats {
    manuscripts: usize,
    groups: usize,
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
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Spread {
    /// Самая большая группа. `None` – в пакете нет ни одной.
    largest: Option<GroupSize>,
    /// Групп ровно с одним фрагментом.
    singletons: usize,
    /// Фрагменты, у которых CTH нет.
    ///
    /// Экспорт кладет их в группу с меткой `aruna::parse::MISSING`, и метка
    /// берется у ядра, а не пишется здесь строкой: она принадлежит разбору, и
    /// вторая ее копия разошлась бы с первой молча.
    without_cth: usize,
}

/// Группа и ее размер.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct GroupSize {
    label: String,
    fragments: usize,
}

/// Что манифест насчитал о письме корпуса при разборе.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct Fonts {
    /// Документы, чей текст пришел не в нормальной форме C.
    not_in_nfc: usize,
    /// Документы, где встречаются кодовые точки из области частного
    /// использования.
    with_private_use: usize,
    /// Сколько таких точек различают во всем корпусе.
    private_use_points: usize,
    /// Аномалии письма – все шесть счетчиков манифеста одним числом.
    anomalies: usize,
}

/// Откуда взяты числа.
///
/// Поле нужно не окну, а тому, кто разбирается в расхождении: манифест пишет
/// экспорт ядра в тот же миг, когда пакет складывается, а обход каталога
/// считает то, что на диске лежит сейчас. Разойтись они могут только если
/// пакет после сборки правили руками, и тогда важно знать, какой из двух
/// ответов получен.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
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

impl serde::Serialize for StatsError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[tauri::command]
fn corpus_location() -> Result<CorpusLocation, CommandError> {
    let downloads = aruna::paths::downloads_dir().map_err(|_| CommandError::Downloads)?;
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
#[tauri::command]
fn corpus_stats(path: String) -> Result<CorpusStats, StatsError> {
    read_stats(std::path::Path::new(&path))
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
        manuscripts: manifest.counts.documents,
        groups: manifest.counts.groups,
        source: StatsSource::Manifest,
        spread: spread_of(
            manifest
                .groups
                .into_iter()
                .map(|group| (group.label, group.documents.len())),
        ),
        fonts: manifest.fonts.map(|fonts| Fonts {
            not_in_nfc: fonts.documents_not_in_nfc,
            with_private_use: fonts.documents_with_private_use,
            private_use_points: fonts.private_use_points.len(),
            anomalies: fonts.anomalies.values().sum(),
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
            without_cth += fragments;
        }
        // Строго больше: при равенстве остается первая встреченная, а порядок
        // здесь – тот, в котором группы перечисляет опись.
        if largest
            .as_ref()
            .is_none_or(|biggest| fragments > biggest.fragments)
        {
            largest = Some(GroupSize { label, fragments });
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
        manuscripts,
        groups: sizes.len(),
        source: StatsSource::Walk,
        spread: spread_of(sizes),
        // Обход считает файлы, а не читает документы: как написан их текст, он
        // не знает и знать не может.
        fonts: None,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
        .invoke_handler(tauri::generate_handler![corpus_location, corpus_stats])
        .setup(|app| {
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
        let message = serde_json::to_string(&failure).unwrap();
        assert_eq!(message, r#""пакет по этому пути не найден""#);
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
    /// Типы на стороне TypeScript написаны вторым экземпляром – specta в
    /// проект не введена, – и этот файл держит два экземпляра вместе:
    /// здесь проверяется, что Rust сериализуется именно так, а в
    /// `frontend/src/App.test.ts` – что тот же файл совпадает с типизированным
    /// литералом `frontend/src/stats.ts`. Расхождение любой из трех сторон
    /// роняет одну из двух проверок.
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
