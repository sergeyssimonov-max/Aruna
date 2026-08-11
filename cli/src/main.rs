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
                ArunaError::Network { .. } | ArunaError::Http { .. } => {
                    eprintln!("Проверьте сетевое соединение и доступность Zenodo.");
                }
                ArunaError::EmptyArchive | ArunaError::Zip(_) => {
                    eprintln!("Архив повреждён или не содержит XML-документов.");
                }
                ArunaError::DownloadsDir => {
                    eprintln!("Не удалось определить каталог Downloads.");
                }
                _ => {}
            }
            ExitCode::FAILURE
        }
    }
}
