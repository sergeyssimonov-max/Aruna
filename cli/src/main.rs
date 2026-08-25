//! Aruna CLI — zero-argument inventory generator for TLHdig (Zenodo).

// See the note in `lib.rs`: the compiler is what holds this project to no
// `unsafe` of its own.
#![forbid(unsafe_code)]

use aruna::error::ArunaError;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Optional override for offline / testing: ARUNA_ZIP=/path/to/archive.zip
    let local = std::env::var_os("ARUNA_ZIP").map(PathBuf::from);

    // The binary is the one front end that prints, and `progress::Stderr` is
    // where it says so: the library below has no opinion about stderr.
    //
    // The cancellation handle is created and dropped here without ever being
    // set. A terminal program is stopped with Ctrl-C, which this cannot catch
    // and does not try to — the flag exists so that the same core can be
    // driven by a window that has a Cancel button, and this binary is simply
    // the caller that never presses it.
    let cancel = aruna::job::Cancel::new();
    let job = aruna::job::Job::new(&aruna::progress::Stderr, &cancel);

    // Through `app`, not through `aruna::run` directly. The scenario is the
    // same one a window will call, and a binary that reached past it would be
    // the second answer to "what does building the inventory come to" — which
    // is the arrangement that layer exists to prevent.
    let request = aruna::app::CorpusRequest {
        local_archive: local,
    };
    match aruna::app::build_corpus(&request, &job) {
        Ok(report) => {
            // Two paths, named separately: the folder to browse, and the file
            // inside it to open. The inventory sits under the package root
            // rather than beside it, and a reader should not have to guess
            // which of the files in there is the one to open.
            println!("Готово.");
            println!("  Корпус: {}", report.package.root.display());
            println!("  Опись:  {}", report.inventory.display());
            println!(
                "  рукописей: {}, групп: {}",
                report.package.documents, report.package.groups
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            report(&err);
            ExitCode::FAILURE
        }
    }
}

/// Print the failure, its cause, and what the person in front of it can do.
///
/// A cancellation is not a failure and is not worded as one. It reaches this
/// function because it travels as an error — see `ArunaError::Cancelled` — and
/// the wording is where the two are told apart.
fn report(err: &ArunaError) {
    if let ArunaError::Cancelled { phase } = err {
        eprintln!("Остановлено на этапе: {phase}");
        return;
    }
    eprintln!("Ошибка: {err}");
    if let Some(src) = std::error::Error::source(err) {
        eprintln!("  причина: {src}");
    }
    if let Some(advice) = advice(err) {
        eprintln!("{advice}");
    }
}

/// What to try next, for the failures where there is something to try.
///
/// Separated from [`report`] so the wording of each case can be read — and
/// changed — without the printing around it, and so a new error variant that
/// deserves advice is a missing arm here rather than a line lost in a `match`
/// that also handles exit codes.
///
/// `None` means the error message says everything useful on its own.
fn advice(err: &ArunaError) -> Option<String> {
    Some(match err {
        ArunaError::Network { .. } => {
            "Проверьте сетевое соединение и доступность Zenodo.".to_string()
        }
        // A republished record is not something the reader can fix, and this
        // used to tell them to edit a source file they may well not have:
        // the advice reaches a reader who installed a .app just as often as one
        // with the repository checked out. The new release is the fix; the
        // constants are how it is made, and that line is for whoever makes it.
        ArunaError::Http {
            status: 404 | 410, ..
        } => "Zenodo больше не отдаёт этот файл — вероятно, запись перевыпущена.\n\
              Поставьте свежий выпуск Aruna: он приходит с новым адресом и новой суммой.\n\
                https://github.com/sergeyssimonov-max/Aruna/releases/latest\n\
              Если вы собираете из исходников — это ZENODO_ZIP_URL и ZENODO_ZIP_MD5\n\
              в cli/src/download.rs."
            .to_string(),
        ArunaError::Http { .. } => "Zenodo сейчас недоступен. Попробуйте позже.".to_string(),
        ArunaError::ChecksumMismatch { .. } => {
            "Архив скачался целиком, но его MD5 не совпал с ожидаемым.\n\
             Скорее всего, Zenodo перевыпустил архив. Повторный запуск не поможет —\n\
             сумма не изменится.\n\
             Поставьте свежий выпуск Aruna: он приходит с новой суммой.\n\
               https://github.com/sergeyssimonov-max/Aruna/releases/latest\n\
             Если вы собираете из исходников — сверьте сумму на странице записи\n\
             и обновите ZENODO_ZIP_MD5 в cli/src/download.rs."
                .to_string()
        }
        ArunaError::EmptyArchive | ArunaError::Zip(_) => {
            "Архив повреждён или не содержит XML-документов.".to_string()
        }
        ArunaError::DownloadsDir => "Не удалось определить каталог Downloads.".to_string(),
        // The finished inventory is not lost — say where it is, and what is
        // holding the old file open.
        ArunaError::Replace { scratch, .. } => format!(
            "Новая опись готова и никуда не делась — она лежит рядом:\n  {}\n\
             Закройте программу, которая держит старый файл открытым \
             (обычно это браузер), и запустите ещё раз.",
            scratch.display()
        ),
        // The message already says how much arrived and where the line is; what
        // it cannot say is that a body which outruns its own header is almost
        // never Zenodo, and that the one case where it is has a fix in the
        // source rather than in the network.
        ArunaError::Oversized { .. } => "Ответ оказался длиннее, чем сервер сам объявил.\n\
             Обычно это значит, что до Zenodo дотянулось что-то по дороге —\n\
             портал Wi-Fi, корпоративный прокси или подмена ответа.\n\
             Если же архив просто вырос — предел поднимается в исходниках\n\
             (MAX_DOWNLOAD в cli/src/download.rs), и выпуск с поднятым пределом\n\
             будет новее того, что у вас установлен."
            .to_string(),
        // The export refuses to overwrite one document with another; the
        // message already names both, and what to do about it is a question
        // about the corpus rather than about this program.
        ArunaError::ExportCollision { .. } => "Два документа претендуют на одно место в пакете.\n\
             Это расхождение в исходных данных, а не сбой сборки —\n\
             сверьте оба исходных пути, названных выше."
            .to_string(),
        // The export's own messages already carry the paths and the counts;
        // what they cannot say is that none of these three is something the
        // reader broke, and what to do about each differs.
        // The number in the message is the limit, not the diagnosis: an entry
        // this size is either a corrupted archive or one built to be expanded,
        // and neither is answered by trying again.
        ArunaError::ExportDocumentTooLarge { .. } => {
            "Один документ в архиве больше допустимого предела и не был прочитан.\n\
             Обычно это повреждённый архив или архив, собранный так, чтобы\n\
             раздуться при распаковке. Пакет не собран, память не израсходована."
                .to_string()
        }
        // Two ceilings that are not about one document but about the archive as
        // a whole; both mean the same thing to a reader, and neither is
        // answered by trying again.
        ArunaError::ArchiveTooManyEntries { .. } => {
            "В архиве больше записей, чем эта программа готова прочитать.\n\
             Корпус TLHdig — около 24 500; архив такого размера собран не из него.\n\
             Ничего не распаковано и не записано."
                .to_string()
        }
        ArunaError::ExportPackageTooLarge { .. } => {
            "Пакет вырос больше допустимого предела, сборка остановлена.\n\
             Опубликованного пакета это не коснулось: всё писалось во временную\n\
             папку, и она удалена."
                .to_string()
        }
        // The one failure that means the data was at risk rather than the run.
        ArunaError::ExportDistorted { .. } => {
            "Документ изменился при нормализации сверх разрешённого — сборка остановлена.\n\
             Ничего не опубликовано: это защита содержимого, а не сбой записи.\n\
             Сообщите, какой файл назван выше."
                .to_string()
        }
        ArunaError::ExportDestination { .. } => {
            "Каталог назначения занят чем-то, чего сборщик не создавал.\n\
             Он ничего не удалил — перенесите папку в сторону и повторите."
                .to_string()
        }
        ArunaError::ExportInvalid { .. } => {
            "Пакет собран, но не сошёлся со своей же моделью, поэтому не опубликован.\n\
             Это ошибка сборщика, а не ваших данных: сообщите, что именно перечислено выше."
                .to_string()
        }
        ArunaError::ExportIncomplete { .. } => {
            "Записано не столько документов, сколько размечено, — пакет не опубликован.\n\
             Это ошибка сборщика, а не ваших данных."
                .to_string()
        }
        // Nothing to advise: the reader stopped the run on purpose, and
        // `report` has already said so without calling it an error.
        ArunaError::Cancelled { .. } => return None,
        ArunaError::Truncated { .. } | ArunaError::Io { .. } => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two cases that are advised differently by status must stay
    /// distinguishable: a gone file needs the URL updated, a busy server needs
    /// waiting. They used to sit in one `match` with the exit code, where the
    /// arm order is what keeps them apart.
    #[test]
    fn a_gone_archive_and_a_busy_server_are_advised_differently() {
        let gone = advice(&ArunaError::Http {
            url: "u".into(),
            status: 404,
            retry_after: None,
        })
        .expect("404 has advice");
        assert!(gone.contains("ZENODO_ZIP_URL"));
        assert!(
            gone.contains("releases/latest"),
            "the fix a reader can apply is a new release, and it has to be named \
             before the constants, which most readers cannot reach: {gone}"
        );

        let busy = advice(&ArunaError::Http {
            url: "u".into(),
            status: 503,
            retry_after: Some(30),
        })
        .expect("503 has advice");
        assert!(busy.contains("Попробуйте позже"));
    }

    /// A stale digest is a republished record, and the reader's copy of this
    /// program cannot be repaired — the pin is compiled in. The advice has to
    /// say so, say that retrying is pointless, and point at the release that
    /// carries the new sum. See the note on `ZENODO_ZIP_MD5`.
    #[test]
    fn a_checksum_mismatch_sends_the_reader_to_a_new_release() {
        let stale = advice(&ArunaError::ChecksumMismatch {
            url: "u".into(),
            expected: "a".into(),
            got: "b".into(),
        })
        .expect("a mismatch has advice");
        assert!(stale.contains("releases/latest"), "{stale}");
        assert!(
            stale.contains("не поможет"),
            "retrying is ruled out: {stale}"
        );
    }

    /// A failed replace must name the file it kept — that path is the whole
    /// point of the variant.
    #[test]
    fn a_failed_replace_names_the_file_it_kept() {
        let advice = advice(&ArunaError::Replace {
            path: PathBuf::from("/out/inventory.html"),
            scratch: PathBuf::from("/out/inventory.html.123.part"),
            source: std::io::Error::other("busy"),
        })
        .expect("a kept inventory has advice");
        assert!(advice.contains("/out/inventory.html.123.part"));
    }

    /// An overrun is worth explaining rather than reporting: the number in the
    /// message is not what the reader needs to act on.
    #[test]
    fn an_oversized_body_points_at_what_is_usually_causing_it() {
        let advice = advice(&ArunaError::Oversized {
            url: "u".into(),
            limit: 4096,
            got: 4097,
        })
        .expect("an overrun has advice");
        assert!(advice.contains("прокси"), "names the usual cause: {advice}");
        assert!(
            advice.contains("MAX_DOWNLOAD"),
            "names the other one: {advice}"
        );
    }

    /// Errors whose own message is the whole story get no second paragraph.
    #[test]
    fn a_self_explanatory_error_is_left_alone() {
        assert!(advice(&ArunaError::Truncated {
            url: "u".into(),
            expected: 10,
            got: 4,
        })
        .is_none());
    }
}
