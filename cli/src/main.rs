//! Aruna CLI — zero-argument inventory generator for TLHdig (Zenodo).

// See the note in `lib.rs`: the compiler is what holds this project to no
// `unsafe` of its own.
#![forbid(unsafe_code)]

use aruna::error::ArunaError;

mod console;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Answered before anything else, because everything else is work: reading
    // the environment, then either a 71 MiB download or a full pass over the
    // corpus. Until now `--version` was a word the program never looked at, so
    // the answer to it was a package in the reader's Downloads folder — see
    // §7.3 of the specification, which recorded that as an open position and
    // named this as the minimal answer.
    if asks_what_this_is() {
        say(&usage());
        return ExitCode::SUCCESS;
    }

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
            say(&format!(
                "Готово.\n  Корпус: {}\n  Опись:  {}\n  рукописей: {}, групп: {}\n",
                report.package.root.display(),
                report.inventory.display(),
                report.package.documents,
                report.package.groups
            ));
            ExitCode::SUCCESS
        }
        Err(err) => {
            report(&err);
            ExitCode::FAILURE
        }
    }
}

/// Whether the first word on the command line is one of the two questions.
///
/// Two flags, and deliberately no parser. This program takes no arguments —
/// the corpus, the destination and the output name are all decided for it — so
/// there is no command line to read; what it needs is an answer to "what is
/// this and how do I point it at a file I already have".
///
/// Four spellings, not two. `-h` and `-V` were left out when the long forms
/// were added, and the short ones are the first reflex of anyone who meets an
/// unfamiliar binary — so `aruna -V` did not print a version, it started a full
/// corpus build, and without `ARUNA_ZIP` that meant fetching 71 MiB and leaving
/// a package in the reader's Downloads folder. Someone asking what this program
/// is has not asked for that. Added on 2026-08-31 at the owner's word.
///
/// A subcommand or a second flag further along the line is still not
/// recognised, and the run proceeds as it always has: inventing a command line
/// this program does not have would be a larger change than the one asked for,
/// and `tests/cli_process.rs` holds both halves of that.
fn asks_what_this_is() -> bool {
    matches!(
        std::env::args_os()
            .nth(1)
            .as_deref()
            .and_then(|arg| arg.to_str()),
        Some("--help" | "-h" | "--version" | "-V")
    )
}

/// The name, the version and the one environment variable that changes a run.
///
/// The version is taken from the manifest rather than written out: a number
/// repeated by hand is a number that stops being true — `download::user_agent`
/// told Zenodo `Aruna/1.0` through the whole 2.x line for exactly that reason.
fn usage() -> String {
    format!(
        "Aruna {} – опись корпуса TLHdig (Zenodo) в HTML.\n\
         \n\
         Запускается без аргументов: берет архив из кеша или скачивает его\n\
         с Zenodo (71 МиБ), собирает пакет с описью в папке Downloads.\n\
         \n\
         ARUNA_ZIP=/путь/к/архиву.zip – взять готовый архив вместо загрузки.\n",
        env!("CARGO_PKG_VERSION")
    )
}

/// Print the failure, its cause, and what the person in front of it can do.
///
/// A cancellation is not a failure and is not worded as one. It reaches this
/// function because it travels as an error — see `ArunaError::Cancelled` — and
/// the wording is where the two are told apart.
fn report(err: &ArunaError) {
    if let ArunaError::Cancelled { phase } = err {
        complain(&format!(
            "Остановлено на этапе: {}\n",
            console::phase(*phase)
        ));
        return;
    }
    // По-русски целиком: `Display` ошибки английский и остается таким для
    // журналов и провода, а здесь его пересказывает `console`.
    let mut text = format!("Ошибка: {}\n", console::headline(err));
    if let Some(why) = console::cause(err) {
        text.push_str(&format!("  причина: {why}\n"));
    }
    if let Some(advice) = advice(err) {
        text.push_str(&format!("{advice}\n"));
    }
    complain(&text);
}

/// Write `text` to standard output, and lose it if nobody is reading.
///
/// Not `print!`: it panics when the write fails, and `aruna | head -1` or a
/// terminal that went away would turn a finished build into exit code 101.
/// The package is the result; the lines about it are a courtesy.
fn say(text: &str) {
    use std::io::Write as _;
    let _ = std::io::stdout().write_all(text.as_bytes());
}

/// The same for standard error: a failure with nowhere to be told is still a
/// failure, and the exit code says so.
fn complain(text: &str) {
    use std::io::Write as _;
    let _ = std::io::stderr().write_all(text.as_bytes());
}

/// What to try next, for the failures where there is something to try.
///
/// Separated from [`report`] so the wording of each case can be read — and
/// changed — without the printing around it, and so a new error variant that
/// deserves advice is a missing arm here rather than a line lost in a `match`
/// that also handles exit codes.
///
/// `None` means the error message says everything useful on its own.
///
/// # This is one of two sets of Russian sentences, and the split is deliberate
///
/// The other lives in `frontend/src/App.svelte`, keyed by the failure code the
/// wire carries. They say the same things about the same failures, and until
/// 2026-09-19 nothing said why there are two — by which time they had already
/// drifted apart in four ways. The boundary, written down now:
///
/// * **Here** is for the person at a terminal, and that person is usually
///   building from source. So this may name paths, source constants, and the
///   releases page, and it calls the source `Zenodo`, which is the host the
///   program actually talks to.
/// * **There** is for the reader who has only the application. So no path,
///   no constant, no URL, one or two sentences — and the source is named
///   `Hethitologie-Portal Mainz (запись на Zenodo)`, because a reader looks for
///   the corpus by its publisher rather than by its host. That naming was the
///   owner's decision of 2026-09-19, taken for the window alone.
/// * **Neither may contradict the other about what happened.** Different words
///   for one audience or the other are fine; a different account of the failure
///   is a defect.
/// * **Both obey one typography** — `CLAUDE.md`, «Типографика русских текстов»:
///   middle dash, `е` and never `ё`. This set broke that rule in twelve places
///   until the day the boundary was written, and `advice_keeps_the_projects_typography`
///   below is what keeps it now.
///
/// Merging the two was considered and declined: the audiences differ, and so do
/// the naming decision and the level of detail each is allowed.
fn advice(err: &ArunaError) -> Option<String> {
    Some(match err {
        ArunaError::Network { .. } => {
            "Проверьте сетевое соединение и доступность Zenodo, а если в окружении\n\
             назван прокси (HTTPS_PROXY, ALL_PROXY, HTTP_PROXY) – и его."
                .to_string()
        }
        // A republished record is not something the reader can fix, and this
        // used to tell them to edit a source file they may well not have:
        // the advice reaches a reader who installed a .app just as often as one
        // with the repository checked out. The new release is the fix; the
        // constants are how it is made, and that line is for whoever makes it.
        ArunaError::Http {
            status: 404 | 410, ..
        } => "Zenodo больше не отдает этот файл – вероятно, запись перевыпущена.\n\
              Поставьте свежий выпуск Aruna: он приходит с новым адресом и новой суммой.\n\
                https://github.com/sergeyssimonov-max/Aruna/releases/latest\n\
              Если вы собираете из исходников – это ZENODO_ZIP_URL и ZENODO_ZIP_MD5\n\
              в cli/src/download.rs."
            .to_string(),
        ArunaError::Http { .. } => "Zenodo сейчас недоступен. Попробуйте позже.".to_string(),
        ArunaError::ChecksumMismatch { .. } => {
            "Архив скачался целиком, но его MD5 не совпал с ожидаемым.\n\
             Скорее всего, Zenodo перевыпустил архив. Повторный запуск не поможет –\n\
             сумма не изменится.\n\
             Поставьте свежий выпуск Aruna: он приходит с новой суммой.\n\
               https://github.com/sergeyssimonov-max/Aruna/releases/latest\n\
             Если вы собираете из исходников – сверьте сумму на странице записи\n\
             и обновите ZENODO_ZIP_MD5 в cli/src/download.rs."
                .to_string()
        }
        ArunaError::EmptyArchive | ArunaError::Zip(_) => {
            "Архив поврежден или не содержит XML-документов.".to_string()
        }
        ArunaError::DownloadsDir => "Не удалось определить каталог Downloads.".to_string(),
        // The finished inventory is not lost — say where it is, and what is
        // holding the old file open.
        ArunaError::Replace { scratch, .. } => format!(
            "Новая опись готова и никуда не делась – она лежит рядом:\n  {}\n\
             Закройте программу, которая держит старый файл открытым \
             (обычно это браузер), и запустите еще раз.",
            scratch.display()
        ),
        // The message already says how much arrived and where the line is; what
        // it cannot say is that a body which outruns its own header is almost
        // never Zenodo, and that the one case where it is has a fix in the
        // source rather than in the network.
        // Two ceilings raise this, and they are different accidents: the
        // archive outrunning what its own header promised, and a question
        // answered with more than a record document can be. Naming the wrong
        // constant sends the reader to the wrong line of the source.
        ArunaError::Oversized { limit, .. } if *limit == aruna::download::MAX_METADATA => {
            "Ответ на запрос о записи оказался длиннее, чем запись бывает.\n\
             Обычно это значит, что вместо Zenodo ответил кто-то другой –\n\
             портал Wi-Fi, корпоративный прокси или подмена ответа.\n\
             Если же запись просто выросла – предел поднимается в исходниках\n\
             (MAX_METADATA в cli/src/download.rs)."
                .to_string()
        }
        ArunaError::Oversized { .. } => "Ответ оказался длиннее, чем сервер сам объявил.\n\
             Обычно это значит, что до Zenodo дотянулось что-то по дороге –\n\
             портал Wi-Fi, корпоративный прокси или подмена ответа.\n\
             Если же архив просто вырос – предел поднимается в исходниках\n\
             (MAX_DOWNLOAD в cli/src/download.rs), и выпуск с поднятым пределом\n\
             будет новее того, что у вас установлен."
            .to_string(),
        // The export refuses to overwrite one document with another; the
        // message already names both, and what to do about it is a question
        // about the corpus rather than about this program.
        ArunaError::ExportCollision { .. } => "Два документа претендуют на одно место в пакете.\n\
             Это расхождение в исходных данных, а не сбой сборки –\n\
             сверьте оба исходных пути, названных выше."
            .to_string(),
        // Two groups spelled apart that the disk files as one: the same kind
        // of disagreement in the corpus, and the same answer.
        ArunaError::ExportFolderCollision { .. } => {
            "Две группы CTH записаны по-разному, а на диске легли бы в одну папку.\n\
             Это расхождение в исходных данных, а не сбой сборки –\n\
             сверьте оба исходных пути, названных выше."
                .to_string()
        }
        // Same shape as the collision above and the same answer: the archive
        // says two things at once, and only whoever built it can say which was
        // meant.
        ArunaError::ArchiveDuplicateEntry { .. } => {
            "В архиве два документа с одним и тем же именем.\n\
             Это расхождение в исходных данных, а не сбой сборки –\n\
             имя названо выше."
                .to_string()
        }
        // The export's own messages already carry the paths and the counts;
        // what they cannot say is that none of these three is something the
        // reader broke, and what to do about each differs.
        // The number in the message is the limit, not the diagnosis: an entry
        // this size is either a corrupted archive or one built to be expanded,
        // and neither is answered by trying again.
        ArunaError::ExportDocumentTooLarge { .. } => {
            "Один документ в архиве больше допустимого предела и не был прочитан.\n\
             Обычно это поврежденный архив или архив, собранный так, чтобы\n\
             раздуться при распаковке. Пакет не собран, память не израсходована."
                .to_string()
        }
        // Two ceilings that are not about one document but about the archive as
        // a whole; both mean the same thing to a reader, and neither is
        // answered by trying again.
        ArunaError::ArchiveTooManyEntries { .. } => {
            "В архиве больше записей, чем эта программа готова прочитать.\n\
             Корпус TLHdig – около 24 500; архив такого размера собран не из него.\n\
             Ничего не распаковано и не записано."
                .to_string()
        }
        ArunaError::ExportPackageTooLarge { .. } => {
            "Пакет вырос больше допустимого предела, сборка остановлена.\n\
             Опубликованного пакета это не коснулось: все писалось во временную\n\
             папку, и она удалена."
                .to_string()
        }
        // The one failure that means the data was at risk rather than the run.
        ArunaError::ExportDistorted { .. } => {
            "Документ изменился при нормализации сверх разрешенного – сборка остановлена.\n\
             Ничего не опубликовано: это защита содержимого, а не сбой записи.\n\
             Сообщите, какой файл назван выше."
                .to_string()
        }
        ArunaError::PublishBusy { .. } => {
            "Другой запуск публикует пакет в ту же папку и не отпускает ее.\n\
             Дождитесь его окончания и повторите. Если больше ни одна копия\n\
             Aruna не работает, удалите файл блокировки, названный выше."
                .to_string()
        }
        ArunaError::ExportDestination { .. } => {
            "Каталог назначения занят чем-то, чего сборщик не создавал.\n\
             Он ничего не удалил – перенесите папку в сторону и повторите."
                .to_string()
        }
        ArunaError::ExportInvalid { .. } => {
            "Пакет собран, но не сошелся со своей же моделью, поэтому не опубликован.\n\
             Это ошибка сборщика, а не ваших данных: сообщите, что именно перечислено выше."
                .to_string()
        }
        ArunaError::ExportIncomplete { .. } => {
            "Записано не столько документов, сколько размечено – пакет не опубликован.\n\
             Это ошибка сборщика, а не ваших данных."
                .to_string()
        }
        // The fonts either arrived with the application or they did not, and
        // neither case is about the corpus or the network. The advice differs
        // because the repair does: one is an install to redo, the other a file
        // that is not the one recorded — and this program will not quietly use
        // another in its place.
        ArunaError::FontMissing { .. } => {
            "Приложение установлено не полностью: файл шрифта не на месте.\n\
             Переустановите его из образа – шрифты лежат внутри приложения и не скачиваются."
                .to_string()
        }
        ArunaError::FontAltered { .. } => {
            "Файл шрифта не совпадает с записанным в docs/FONTS.md.\n\
             Другой шрифт вместо него подставлен не будет: в PDF это дало бы не тот знак."
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

    /// **Русские строки этого входа держат типографику проекта.**
    ///
    /// Правило записано в `CLAUDE.md` и до 19.09.2026 нарушалось здесь в
    /// двенадцати местах: буква «ё» и длинное тире. Нашлось это не проверкой, а
    /// разбором, который сравнил здешние подсказки с фразами окна – у окна свой
    /// сторож на то же правило, а у консоли не было никакого. Читается сам
    /// исходник: строки подсказок многострочные, и собирать их все вызовами
    /// `advice` значило бы перечислять варианты ошибок вручную и забыть
    /// новый.
    #[test]
    fn advice_keeps_the_projects_typography() {
        let source = include_str!("main.rs");
        let offenders: Vec<&str> = source
            .lines()
            .filter(|line| {
                let start = line.trim_start();
                !start.starts_with("//") && !start.starts_with('*') && !start.starts_with("/*")
            })
            .filter(|line| {
                line.chars()
                    .any(|c| ('а'..='я').contains(&c) || ('А'..='Я').contains(&c))
            })
            .filter(|line| line.contains('ё') || line.contains('Ё') || line.contains('—'))
            .collect();

        assert!(
            offenders.is_empty(),
            "русский текст этого входа нарушает типографику проекта: {offenders:#?}"
        );
    }

    /// **Все три расхождения в исходных данных советуют одно и то же и говорят
    /// это по-разному.**
    ///
    /// Три отказа – два документа на одно место, одно имя записи дважды и две
    /// группы в одной папке диска (с 26.09.2026) – одинаковы для читателя по
    /// действию: чинить надо не программу, а корпус.
    /// Ветка для второго появилась 30.08.2026 и до этого теста не выполнялась
    /// ни разу: `llvm-cov` показывал ее непокрытой. Совет, который никто не
    /// читал, легко удалить незаметно.
    #[test]
    fn every_kind_of_corpus_disagreement_is_advised_and_not_confused() {
        let duplicate = advice(&ArunaError::ArchiveDuplicateEntry {
            entry: "xml/KBo 1.1.xml".into(),
        })
        .expect("a duplicated entry has advice");
        let collision = advice(&ArunaError::ExportCollision {
            group: "CTH 5".into(),
            fragment: "KBo 1.1".into(),
            first: "a.xml".into(),
            second: "b.xml".into(),
            path: std::path::PathBuf::from("CTH 5/KBo 1.1.xml"),
        })
        .expect("a collision has advice");
        let folders = advice(&ArunaError::ExportFolderCollision {
            first_group: "CTH 5a".into(),
            second_group: "CTH 5A".into(),
            first: "a.xml".into(),
            second: "b.xml".into(),
        })
        .expect("a folder collision has advice");

        for text in [&duplicate, &collision, &folders] {
            assert!(
                text.contains("исходных данных"),
                "the reader is not told this is the corpus, not the program: {text}"
            );
        }
        for (one, other) in [
            (&duplicate, &collision),
            (&duplicate, &folders),
            (&collision, &folders),
        ] {
            assert_ne!(one, other, "two different faults must not read as one");
        }
    }

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
