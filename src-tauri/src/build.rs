//! Жизненный цикл сборки окна: занятость, отмена, рабочий поток и события.

use crate::dto::{counted, BuildFailure, BuildProgress, BuildReport, PdfReport};
use crate::FONT_RESOURCES;

/// Синк прогресса, который шлет события в окно.
///
/// `report` обязан не паниковать: `catch_unwind` в проекте нет нигде, а паника
/// отсюда прошла бы сквозь `export::build` и вернулась бы обломком задания без
/// объяснения. Поэтому отказ отправки проглатывается: окно, которое закрыли на
/// середине сборки – это не сбой сборки.
struct WindowProgress {
    app: tauri::AppHandle,
    job: u32,
    /// Последнее число построенных PDF этого прогона ([`BuildProgress::carrying`]).
    built: std::sync::Mutex<Option<u32>>,
}

impl aruna::progress::Progress for WindowProgress {
    fn report(&self, event: aruna::progress::Event<'_>) {
        use tauri_specta::Event as _;
        let mut last = self
            .built
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = BuildProgress::of(self.job, &event)
            .carrying(&mut last)
            .emit(&self.app);
    }
}

/// Идет ли сборка, и чем ее остановить.
///
/// Флаг живет здесь, а не в команде: `cancel_build` приходит вторым вызовом,
/// когда кадр первого еще не вернулся – и `Job::with_id` написан ровно для
/// этого случая. `Cancel` клонируется поверх `Arc`, поэтому останавливает не тот
/// поток, который работает.
#[derive(Debug, Default)]
pub(crate) struct Building(std::sync::Mutex<Option<aruna::job::Cancel>>);

impl Building {
    /// Место под замком, и после чужой паники тоже.
    ///
    /// Под замком лежит `Option<Cancel>`, а секции, которые его трогают –
    /// присваивание, проверка и `Cancel::cancel` (запись в `AtomicBool`) – по
    /// одному шагу: паника не оставит значение наполовину измененным, и
    /// отравление ничего о нем не говорит. До 02.10.2026 отравленный замок
    /// означал «предыдущая сборка оборвалась» на каждую следующую сборку, а
    /// `release` и `stop` его молча пропускали – окно оставалось занятым до
    /// перезапуска (находка № 6 refactor-1).
    fn slot(&self) -> std::sync::MutexGuard<'_, Option<aruna::job::Cancel>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Занять место под сборку, если оно свободно.
    ///
    /// Отдельной функцией, а не строками внутри команды, по одной причине: это
    /// и есть правило «одна сборка за раз», и проверить его должно быть можно
    /// без Tauri вокруг. Место держит возвращенный страж и освобождает, когда
    /// его отпускают – в том числе раскруткой паники.
    pub(super) fn claim(&self, cancel: aruna::job::Cancel) -> Result<Claimed<'_>, BuildFailure> {
        let mut slot = self.slot();
        if slot.is_some() {
            return Err(BuildFailure::shell(
                "busy",
                "сборка уже идет",
                // Повторить имеет смысл – но после того, как закончится та.
                true,
            ));
        }
        *slot = Some(cancel);
        Ok(Claimed(self))
    }

    /// Освободить место, чем бы прогон ни кончился. Зовет его страж.
    fn release(&self) {
        *self.slot() = None;
    }

    /// Попросить текущую сборку остановиться. Молча, если ее нет.
    pub(super) fn stop(&self) {
        if let Some(cancel) = self.slot().as_ref() {
            cancel.cancel();
        }
    }
}

/// Занятое место под сборку: пока страж жив, вторая сборка получает `busy`.
///
/// Освобождение держит `Drop`, а не строка после прогона: строка не
/// исполняется, если до нее долетела паника, а `Drop` исполняется и при
/// раскрутке.
#[must_use = "место освобождается, как только страж отпущен"]
#[derive(Debug)]
pub(super) struct Claimed<'a>(&'a Building);

impl Drop for Claimed<'_> {
    fn drop(&mut self) {
        self.0.release();
    }
}

/// Прогон сборки в занятом месте, без окна вокруг.
///
/// Место занимается здесь и освобождается стражем, чем бы `work` ни
/// кончилась. Работа идет в потоке блокирующих задач; паника в ней
/// возвращается ошибкой `JoinHandle` (tokio ловит ее сам) и доходит до окна
/// существующим отказом `interrupted`.
pub(super) async fn in_slot<F>(
    building: &Building,
    cancel: aruna::job::Cancel,
    work: F,
) -> Result<BuildReport, BuildFailure>
where
    F: FnOnce() -> Result<BuildReport, BuildFailure> + Send + 'static,
{
    let _claimed = building.claim(cancel)?;
    match tauri::async_runtime::spawn_blocking(work).await {
        Ok(result) => result,
        // Задание не вернулось: рабочий поток снят или сорван паникой изнутри
        // зависимости – в собственном коде паник нет, это правило проекта.
        Err(_) => Err(BuildFailure::shell(
            "interrupted",
            "сборка оборвалась, не сказав почему",
            true,
        )),
    }
}

/// Папка, выбранная человеком – проверенная здесь, а не там, где в нее пишут.
///
/// Окно файловых ручек не получает и путей не толкует: строка приходит с той
/// стороны, и первое, что с ней делается – проверка, что за ней есть каталог.
/// Отказ на этом месте – предложение выбрать другую папку, а не ошибка сборки,
/// которой не было. `None` означает папку загрузок, то есть поведение консоли.
///
/// **Уже этой границы не сделать, и это записано, а не упущено** (аудит
/// 21.09.2026). Папку называет человек в системном диалоге, и назвать он вправе
/// любую – в этом и смысл кнопки «Собрать в папку…». Имени, как у пакета и
/// описи, у папки назначения нет, область путей свелась бы к `**`. Что держит
/// запись: в выбранной папке ядро трогает только имена от `PACKAGE` – сам
/// пакет, промежуточный `.{PACKAGE}.build.*` с меткой владельца и отложенный
/// прежний `.{PACKAGE}.previous` (`aruna::export::build`); под другими именами
/// там ничего не переписывается и не удаляется.
pub(super) fn chosen_destination(
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

pub(super) async fn start(
    app: tauri::AppHandle,
    state: &Building,
    destination: Option<String>,
) -> Result<BuildReport, BuildFailure> {
    use tauri::Manager as _;

    let chosen = chosen_destination(destination)?;
    let request = window_request(app.path().resource_dir().ok());
    let cancel = aruna::job::Cancel::new();

    let handle = app.clone();
    let flag = cancel.clone();
    // Задание строится внутри замыкания, и иначе нельзя: `Job<'a>` заимствует
    // и синк, и флаг, поэтому оно не может жить дольше вызова, который его
    // создал. Через границу потока переходят владеющие половины.
    in_slot(state, cancel, move || {
        let id = aruna::job::JobId::next();
        let sink = WindowProgress {
            app: handle,
            job: counted(id.get()),
            built: std::sync::Mutex::new(None),
        };
        build_once(id, &request, chosen.as_deref(), &flag, &sink)
    })
    .await
}

/// Что окно просит у ядра: пакет из закрепленной записи Zenodo, с PDF каждого
/// документа.
///
/// Ядро умеет читать архив с диска, окно этой возможностью не пользуется:
/// `None` – закрепленная запись Zenodo через кеш.
///
/// Шрифты – каталог `resources/FONT_RESOURCES`. `resources` – каталог
/// ресурсов приложения, который команда берет у Tauri в момент сборки. Нет его
/// – `fonts: None`, и ядро откажет `font_missing` до начала работы: искать
/// шрифты где-то еще окно не станет. Проверка шрифтов одна, в
/// `app::load_fonts` ядра (решение владельца 18.10.2026, 4.3): итог проверки
/// при запуске (`FontResources`) в путь сборки не входит.
pub(super) fn window_request(resources: Option<std::path::PathBuf>) -> aruna::app::CorpusRequest {
    aruna::app::CorpusRequest {
        local_archive: None,
        fonts: resources.map(|dir| dir.join(FONT_RESOURCES)),
    }
}

/// Сборка как работа: тот же путь, которым идет команда, но без Tauri вокруг.
///
/// Выделено 17.09.2026 ради шва, которого не было. Отмена идущей сборки через
/// окно до сих пор не была доказана ничем: `cancel_build` проверялся на флаге,
/// которому тест сам же его и вручил, а сборка при этом не шла. Здесь сборка
/// идет настоящая, флаг приходит из того же `Building`, что у команды, и
/// проверить можно и исход, и то, что под окончательным именем ничего не
/// осталось.
///
/// Контракт провода не затронут: `commands::build_corpus` сохраняет имя, аргументы и
/// форму ответа, а `bindings.ts` порождается из них.
pub(super) fn build_once(
    id: aruna::job::JobId,
    request: &aruna::app::CorpusRequest,
    chosen: Option<&std::path::Path>,
    cancel: &aruna::job::Cancel,
    progress: &dyn aruna::progress::Progress,
) -> Result<BuildReport, BuildFailure> {
    let job = aruna::job::Job::with_id(id, progress, cancel);
    // Две ветки, а не одна с подстановкой умолчания: место, где спрашивают
    // у платформы про папку загрузок, обязано остаться единственным, и оно
    // внутри `app::build_corpus`.
    match chosen {
        Some(folder) => aruna::app::build_corpus_into(request, folder, &job),
        None => aruna::app::build_corpus(request, &job),
    }
    .map(|report| BuildReport {
        job: counted(report.job.get()),
        package: report.package.root.display().to_string(),
        inventory: report.inventory.display().to_string(),
        documents: counted(report.package.documents),
        groups: counted(report.package.groups),
        disambiguated: counted(report.package.disambiguated),
        stylesheet_dropped: counted(report.package.stylesheet_dropped),
        pdfs: PdfReport {
            built: counted(report.package.pdfs.built),
            refused: counted(report.package.pdfs.refused),
        },
    })
    .map_err(|error| BuildFailure::of(&aruna::app::Failure::of(&error)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Отравленный мьютекс места не держит.**
    ///
    /// Под блокировкой лежит `Option<Cancel>`, и ни одна секция не оставляет
    /// его наполовину измененным: присваивание, проверка и `Cancel::cancel` –
    /// по одному шагу. Читать его после чужой паники безопасно, а отвечать
    /// «оборвалась» на каждую следующую сборку – значит держать окно мертвым
    /// до перезапуска.
    #[test]
    fn a_poisoned_slot_is_released_and_claimed_again() {
        let building = Building::default();
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _locked = building.0.lock();
            panic!("паника под блокировкой");
        }));
        assert!(unwound.is_err());
        assert!(
            building.0.is_poisoned(),
            "мьютекс не отравлен – тест ни о чем"
        );

        building.release();
        building.stop();
        let _next = building
            .claim(aruna::job::Cancel::new())
            .expect("после отравления место свободно");
    }
}
