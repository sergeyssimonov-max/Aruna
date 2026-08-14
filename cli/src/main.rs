//! Aruna CLI — zero-argument inventory generator for TLHdig (Zenodo).

use aruna::error::ArunaError;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Optional override for offline / testing: ARUNA_ZIP=/path/to/archive.zip
    let local = std::env::var_os("ARUNA_ZIP").map(PathBuf::from);

    match aruna::run(local.as_deref()) {
        Ok(path) => {
            println!("Готово. Опись сохранена: {}", path.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Ошибка: {err}");
            if let Some(src) = std::error::Error::source(&err) {
                eprintln!("  причина: {src}");
            }
            // Extra context for common cases
            match &err {
                ArunaError::Network { .. } => {
                    eprintln!("Проверьте сетевое соединение и доступность Zenodo.");
                }
                ArunaError::Http { status: 404 | 410, .. } => {
                    eprintln!(
                        "Zenodo больше не отдаёт этот файл — вероятно, архив перевыпущен.\n\
                         Обновите ZENODO_ZIP_URL и ZENODO_ZIP_MD5 в cli/src/download.rs."
                    );
                }
                ArunaError::Http { .. } => {
                    eprintln!("Zenodo сейчас недоступен. Попробуйте позже.");
                }
                ArunaError::ChecksumMismatch { .. } => {
                    eprintln!(
                        "Архив скачался целиком, но его MD5 не совпал с ожидаемым.\n\
                         Скорее всего, Zenodo перевыпустил архив: сверьте сумму на странице\n\
                         записи и обновите ZENODO_ZIP_MD5 в cli/src/download.rs.\n\
                         Повторный запуск не поможет — сумма не изменится."
                    );
                }
                ArunaError::EmptyArchive | ArunaError::Zip(_) => {
                    eprintln!("Архив повреждён или не содержит XML-документов.");
                }
                ArunaError::DownloadsDir => {
                    eprintln!("Не удалось определить каталог Downloads.");
                }
                ArunaError::Replace { scratch, .. } => {
                    eprintln!(
                        "Новая опись готова и никуда не делась — она лежит рядом:\n  {}",
                        scratch.display()
                    );
                    eprintln!(
                        "Закройте программу, которая держит старый файл открытым \
                         (обычно это браузер), и запустите ещё раз."
                    );
                }
                _ => {}
            }
            ExitCode::FAILURE
        }
    }
}
