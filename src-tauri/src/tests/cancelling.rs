use crate::build::{build_once, window_request, Building};
use crate::dto::{BuildFailure, BuildProgress, BuildReport, PdfReport, Stage};
use crate::{corpus_fixture, FONT_RESOURCES};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Окно, которое нажимает «Остановить», услышав названную стадию.
///
/// Это и есть кнопка: `stop()` – ровно то, что делает команда
/// `cancel_build`, и зовется он здесь по тому же состоянию.
struct WindowThatStops<'a> {
    at: &'static str,
    building: &'a Building,
    seen: Mutex<Vec<&'static str>>,
    presses: AtomicUsize,
}

impl aruna::progress::Progress for WindowThatStops<'_> {
    fn report(&self, event: aruna::progress::Event<'_>) {
        let name = match event {
            aruna::progress::Event::ParsingArchive => "ParsingArchive",
            aruna::progress::Event::Indexed { .. } => "Indexed",
            aruna::progress::Event::ReadingHeaders => "ReadingHeaders",
            aruna::progress::Event::WritingDocuments { .. } => "WritingDocuments",
            aruna::progress::Event::WritingPdfs { .. } => "WritingPdfs",
            aruna::progress::Event::CheckingPackage => "CheckingPackage",
            aruna::progress::Event::CheckingPublished => "CheckingPublished",
            _ => "other",
        };
        self.seen.lock().expect("не отравлен").push(name);
        if name == self.at && self.presses.fetch_add(1, Ordering::SeqCst) == 0 {
            self.building.stop();
        }
    }
}

/// Что лежит в каталоге назначения.
fn entries(root: &std::path::Path) -> Vec<String> {
    let Ok(dir) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = dir
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

/// Малый архив в форме корпуса: `count` рукописей в одной группе.
fn small_archive(dir: &std::path::Path, count: usize) -> std::path::PathBuf {
    use std::io::Write as _;
    let path = dir.join("small.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).expect("архив"));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for i in 1..=count {
        zip.start_file(format!("root/CTH 5_XML_HFR/KBo {i}.xml"), options)
            .expect("запись");
        write!(
            zip,
            r#"<AOxml xml:space="preserve"><AOHeader><docID>KBo 1.{i}</docID><meta><uebern editor="FB" date="2017-03-28"/></meta></AOHeader><body><text><l lg="Hit"/>text</text></body></AOxml>"#
        )
        .expect("тело");
    }
    zip.finish().expect("финиш");
    path
}

/// Один прогон окна на `zip`: окно нажимает «Отменить» на стадии `at`.
fn run_pressing_at(
    zip: std::path::PathBuf,
    at: &'static str,
) -> (
    Result<BuildReport, BuildFailure>,
    usize,
    Vec<String>,
    tempfile::TempDir,
) {
    let destination = tempfile::tempdir().expect("каталог назначения");
    let building = Building::default();
    let cancel = aruna::job::Cancel::new();
    let held = building.claim(cancel.clone()).expect("место свободно");
    let window = WindowThatStops {
        at,
        building: &building,
        seen: Mutex::new(Vec::new()),
        presses: AtomicUsize::new(0),
    };
    let request = aruna::app::CorpusRequest {
        local_archive: Some(zip),
        ..window_request(Some(resources()))
    };
    let outcome = build_once(
        aruna::job::JobId::next(),
        &request,
        Some(destination.path()),
        &cancel,
        &window,
    );
    drop(held);
    let _next = building
        .claim(aruna::job::Cancel::new())
        .expect("после прогона место снова свободно");
    let presses = window.presses.load(Ordering::SeqCst);
    let left = entries(destination.path());
    (outcome, presses, left, destination)
}

/// **Нажатие в окне останавливает идущую сборку – в обычном наборе.**
///
/// Тест ниже делает то же на архиве корпуса и потому стоит под `#[ignore]`:
/// без фикстуры он молча проходил, и доказательство отмены из окна жило
/// только в явном прогоне (Р8 прогона 23.09.2026). Этот строит архив сам и
/// идет всегда. Окно нажимает на стадии записи документов; сборка обязана
/// кончиться отменой, ничего не оставив в папке назначения, а прогон без
/// нажатия на том же архиве – целым пакетом под своим именем.
#[test]
fn a_running_build_stops_when_the_window_asks_on_a_small_archive() {
    let dir = tempfile::tempdir().expect("tempdir");

    let (outcome, presses, left, _keep) =
        run_pressing_at(small_archive(dir.path(), 5), "WritingDocuments");
    let refusal = outcome.expect_err("сборка обязана была остановиться");
    assert_eq!(refusal.code, "cancelled", "отказ не назвался отменой");
    assert!(refusal.cancelled);
    assert_eq!(presses, 1, "окно не нажало ровно один раз");
    assert_eq!(left, Vec::<String>::new(), "после отмены что-то осталось");

    let (outcome, presses, left, _keep) =
        run_pressing_at(small_archive(dir.path(), 5), "НетТакойСтадии");
    let report = outcome.expect("без нажатия сборка обязана дойти до конца");
    assert_eq!(presses, 0);
    assert_eq!(report.documents, 5);
    assert_eq!(left, vec![aruna::export::PACKAGE.to_string()]);
}

/// **Нажатие в окне останавливает идущую сборку, и пакета под окончательным
/// именем не остается.**
#[test]
#[ignore = "читает архив корпуса; запускать явно"]
fn a_running_build_stops_when_the_window_asks() {
    let Some(zip) = corpus_fixture() else {
        eprintln!("пропуск: архива корпуса нет");
        return;
    };
    // `run_pressing_at` и проверяет, что после отмены место снова
    // свободно: следующая сборка не упрется в «сборка уже идет».
    let (outcome, presses, left, _keep) = run_pressing_at(zip, "WritingDocuments");

    let refusal = outcome.expect_err("сборка обязана была остановиться");
    assert_eq!(refusal.code, "cancelled", "отказ не назвался отменой");
    assert!(refusal.cancelled, "отказ не помечен как отмена");
    assert!(presses >= 1, "окно так и не нажало");
    assert_eq!(
        left,
        Vec::<String>::new(),
        "после отмены в каталоге назначения что-то осталось"
    );
}

/// Отрицательный контроль к тесту выше.
///
/// Без нажатия тот же путь обязан собрать пакет целиком. Без этого теста
/// предыдущий доказывал бы только то, что сборка не доходит до конца –
/// что верно и для сборки, сломанной чем угодно другим.
#[test]
#[ignore = "читает архив корпуса; запускать явно"]
fn the_same_run_without_a_press_builds_the_whole_package() {
    let Some(zip) = corpus_fixture() else {
        eprintln!("пропуск: архива корпуса нет");
        return;
    };
    // Тот же приемник, но стадии, которой он ждет, в сборке не бывает.
    let (outcome, presses, left, _keep) = run_pressing_at(zip, "НетТакойСтадии");
    let report = outcome.expect("без нажатия сборка обязана дойти до конца");

    assert_eq!(presses, 0, "никто не нажимал");
    assert_eq!(report.documents, 23936);
    assert_eq!(report.groups, 663);
    assert_eq!(
        left,
        vec!["TLHdig".to_string()],
        "пакет не встал под окончательным именем"
    );
}

/// Файлы PDF под `root`, на любой глубине.
fn pdfs_under(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("каталог читается").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "pdf") {
                found.push(path);
            }
        }
    }
    found
}

/// Каталог ресурсов, каким его видит тест: `cli/resources`, где
/// `FONT_RESOURCES` – те самые шрифты, которые `tauri.conf.json` кладет в
/// ресурсы приложения.
fn resources() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../cli/resources")
}

/// Сборка запросом окна на `zip` в `into` – архив с диска вместо Zenodo,
/// все прочее как просит окно. Затем проверка пакета: PDF в нем столько,
/// сколько построено, и манифест и опись их называют.
fn the_window_builds(
    zip: std::path::PathBuf,
    into: &std::path::Path,
    progress: &dyn aruna::progress::Progress,
) -> BuildReport {
    let request = aruna::app::CorpusRequest {
        local_archive: Some(zip),
        ..window_request(Some(resources()))
    };
    let report = build_once(
        aruna::job::JobId::next(),
        &request,
        Some(into),
        &aruna::job::Cancel::new(),
        progress,
    )
    .expect("окно собирает пакет");
    let package = into.join(aruna::export::PACKAGE);
    let manifest =
        std::fs::read_to_string(package.join(aruna::export::MANIFEST)).expect("манифест");
    let inventory = std::fs::read_to_string(&report.inventory).expect("опись");
    assert_eq!(pdfs_under(&package).len(), report.pdfs.built as usize);
    assert!(manifest.contains("\"pdf"), "манифест не называет PDF");
    assert!(inventory.contains(".pdf"), "опись не ссылается на PDF");
    report
}

/// **Окно строит PDF рядом с каждым документом**: запрос несет каталог
/// шрифтов внутри ресурсов, и у каждого XML малого архива рядом лежит PDF
/// под тем же именем.
#[test]
fn the_window_builds_a_pdf_beside_every_document() {
    let request = window_request(Some(resources()));
    assert_eq!(request.fonts, Some(resources().join(FONT_RESOURCES)));
    assert_eq!(request.local_archive, None);
    // Каталога ресурсов нет – шрифтов не ищут нигде: ядро откажет само.
    assert_eq!(window_request(None).fonts, None);

    let dir = tempfile::tempdir().expect("tempdir");
    let out = tempfile::tempdir().expect("каталог назначения");
    let report = the_window_builds(
        small_archive(dir.path(), 5),
        out.path(),
        &aruna::progress::Silent,
    );
    assert_eq!(report.documents, 5);
    assert_eq!(
        report.pdfs,
        PdfReport {
            built: 5,
            refused: 0
        }
    );
    let package = out.path().join(aruna::export::PACKAGE);
    let mut xml = Vec::new();
    let mut stack = vec![package.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("каталог читается").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "xml") {
                xml.push(path);
            }
        }
    }
    assert_eq!(xml.len(), 5, "документов в пакете не пять");
    for document in &xml {
        assert!(
            document.with_extension("pdf").is_file(),
            "рядом с {} нет PDF",
            document.display()
        );
    }
}

/// Все файлы под `root` с содержимым, по относительному пути.
fn snapshot(root: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("каталог читается").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let bytes = std::fs::read(&path).expect("файл читается");
                files.push((
                    path.strip_prefix(root).expect("под корнем").to_path_buf(),
                    bytes,
                ));
            }
        }
    }
    files.sort();
    files
}

/// **Отмена в фазе PDF оставляет папку назначения прежней** (решение
/// владельца 30.09.2026, 4.10: фаза PDF идет внутри `Staging`, и отмена
/// убирает его тем же механизмом, что у записи документов).
///
/// В папке уже лежит пакет прошлой сборки. Окно нажимает «Отменить»,
/// услышав объявление фазы PDF; сборка обязана кончиться отменой фазы
/// `exporting`, промежуточного каталога в папке остаться не должно, а
/// прежний пакет – байт в байт тот же.
#[test]
fn stopping_in_the_pdf_phase_leaves_the_destination_as_it_was() {
    let dir = tempfile::tempdir().expect("tempdir");
    let destination = tempfile::tempdir().expect("каталог назначения");
    the_window_builds(
        small_archive(dir.path(), 5),
        destination.path(),
        &aruna::progress::Silent,
    );
    let package = destination.path().join(aruna::export::PACKAGE);
    let before = snapshot(&package);

    let building = Building::default();
    let cancel = aruna::job::Cancel::new();
    let held = building.claim(cancel.clone()).expect("место свободно");
    let window = WindowThatStops {
        at: "WritingPdfs",
        building: &building,
        seen: Mutex::new(Vec::new()),
        presses: AtomicUsize::new(0),
    };
    let request = aruna::app::CorpusRequest {
        local_archive: Some(small_archive(dir.path(), 5)),
        ..window_request(Some(resources()))
    };
    let outcome = build_once(
        aruna::job::JobId::next(),
        &request,
        Some(destination.path()),
        &cancel,
        &window,
    );
    drop(held);

    let refusal = outcome.expect_err("сборка обязана была остановиться");
    assert!(refusal.cancelled, "отказ не помечен как отмена");
    assert_eq!(refusal.phase.as_deref(), Some("exporting"));
    assert_eq!(window.presses.load(Ordering::SeqCst), 1);
    assert_eq!(
        entries(destination.path()),
        vec![aruna::export::PACKAGE.to_string()],
        "в папке назначения остался промежуточный каталог"
    );
    assert!(snapshot(&package) == before, "прежний пакет тронут");
}

/// Окно, которое копит то, что дошло бы до него по проводу.
///
/// Перекладывает событие тем же `BuildProgress::of` и `carrying`, что и
/// `WindowProgress`, и ничего не отсеивает, так что накопленное здесь и
/// есть то, что окно получило бы.
#[derive(Default)]
struct WindowThatListens(Mutex<(Vec<BuildProgress>, Option<u32>)>);

impl aruna::progress::Progress for WindowThatListens {
    fn report(&self, event: aruna::progress::Event<'_>) {
        let mut heard = self.0.lock().expect("не отравлен");
        let (events, last) = &mut *heard;
        events.push(BuildProgress::of(1, &event).carrying(last));
    }
}

/// **Фаза PDF доходит до окна** (part3-1, 18.10.2026; до того ее события
/// отбрасывались до провода по решению владельца 30.09.2026, вопрос 8).
///
/// Сборка идет путем окна со шрифтами из `cli/resources/fonts`. Видны
/// своя стадия, `built` – `None` до фазы, ноль в объявлении, число ядра в
/// тиках и последнее число после фазы, последний тик на всем числе и
/// числа PDF в отчете.
#[test]
fn the_pdf_phase_reaches_the_window() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = tempfile::tempdir().expect("каталог назначения");
    let request = aruna::app::CorpusRequest {
        local_archive: Some(small_archive(dir.path(), 5)),
        ..window_request(Some(resources()))
    };
    let window = WindowThatListens::default();
    let report = build_once(
        aruna::job::JobId::next(),
        &request,
        Some(out.path()),
        &aruna::job::Cancel::new(),
        &window,
    )
    .expect("сборка с PDF доходит до конца");

    let (heard, _) = window.0.into_inner().expect("не отравлен");
    let phase: Vec<&BuildProgress> = heard
        .iter()
        .filter(|p| p.stage == Stage::WritingPdfs)
        .collect();
    let (announced, ticks) = phase.split_first().expect("стадия PDF дошла до окна");
    assert_eq!(
        (announced.done, announced.total, announced.built),
        (Some(0), Some(5), Some(0)),
        "объявление стадии"
    );
    assert!(!ticks.is_empty(), "тиков фазы PDF нет");
    assert!(ticks
        .iter()
        .all(|t| t.built.is_some() && t.total == Some(5)));
    let last = ticks.last().expect("тик есть");
    assert_eq!(last.done, last.total, "последний тик не на всем числе");
    let start = heard
        .iter()
        .position(|p| p.stage == Stage::WritingPdfs)
        .expect("фаза есть");
    assert!(
        heard[..start].iter().all(|p| p.built.is_none()),
        "`built` до фазы PDF"
    );
    let end = heard
        .iter()
        .rposition(|p| p.stage == Stage::WritingPdfs)
        .expect("фаза есть");
    assert!(end + 1 < heard.len(), "после фазы событий нет");
    assert!(
        heard[end + 1..].iter().all(|p| p.built == last.built),
        "после фазы `built` не равен последнему тику"
    );

    let pdfs = report.pdfs;
    assert_eq!(pdfs.built + pdfs.refused, 5);
    assert_eq!(Some(pdfs.built), last.built, "отчет разошелся с ходом");
    assert!(
        pdfs.built > 0,
        "ни один PDF не построен – тест доказывает меньше"
    );
    let package = out.path().join(aruna::export::PACKAGE);
    assert_eq!(pdfs_under(&package).len(), pdfs.built as usize);
}

/// Синк, который замечает время фазы PDF: от объявления до последнего
/// тика, на стороне окна.
#[derive(Default)]
struct PdfStopwatch(Mutex<(Option<std::time::Instant>, Option<std::time::Instant>)>);

impl aruna::progress::Progress for PdfStopwatch {
    fn report(&self, event: aruna::progress::Event<'_>) {
        let mut marks = self.0.lock().expect("не отравлен");
        match event {
            aruna::progress::Event::WritingPdfs { .. } => {
                marks.0 = Some(std::time::Instant::now());
            }
            aruna::progress::Event::PdfsWritten { .. } => {
                marks.1 = Some(std::time::Instant::now());
            }
            _ => {}
        }
    }
}

/// **То же на архиве корпуса.** Пакет – в `ARUNA_WINDOW_PACKAGE`, если он
/// назван, чтобы сверить его с опорной суммой пакета. Время фазы PDF от
/// объявления до последнего тика
/// печатается одной строкой `window_pdf_phase_seconds` – это замер, а не
/// порог: предел держит заслон (`pdf_gate`).
#[test]
#[ignore = "читает архив корпуса; запускать явно"]
fn the_window_builds_the_corpus_with_pdfs() {
    let Some(zip) = corpus_fixture() else { return };
    let temporary = tempfile::tempdir().expect("tempdir");
    let into = std::env::var_os("ARUNA_WINDOW_PACKAGE").map(std::path::PathBuf::from);
    let watch = PdfStopwatch::default();
    let report = the_window_builds(zip, into.as_deref().unwrap_or(temporary.path()), &watch);
    assert_eq!((report.documents, report.groups), (23936, 663));
    let pdfs = report.pdfs;
    assert_eq!(pdfs.built + pdfs.refused, 23936);
    let (Some(start), Some(end)) = *watch.0.lock().expect("не отравлен") else {
        panic!("фаза PDF не объявлена или не дошла до тика");
    };
    eprintln!(
        "window_pdf_phase_seconds {:.1}",
        end.duration_since(start).as_secs_f64()
    );
}
