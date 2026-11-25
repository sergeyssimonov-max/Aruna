use super::read::{read_manifest, XmlSummaryError};
use crate::commands::named_inventory;
use crate::dto::CommandError;
use crate::read::{named_package, read_xml_summary};
use std::fs;

/// Опись – это файл с тем именем, которое объявило ядро, и лежать он может
/// где угодно: «Собрать в папку…» кладет его в папку, названную человеком.
#[test]
fn the_inventory_is_the_file_the_core_names_wherever_it_lies() {
    let dir = tempfile::tempdir().unwrap();
    let inventory = dir.path().join(aruna::paths::OUTPUT_FILE_NAME);
    fs::write(&inventory, b"<html></html>").unwrap();

    assert!(named_inventory(&inventory).is_ok());
}

#[test]
fn edition_inventory_and_package_names_must_match() {
    let dir = tempfile::tempdir().unwrap();
    let package = dir.path().join("TLHdig_Beta_0.4");
    fs::create_dir(&package).unwrap();
    let inventory = package.join("TLHdig_Beta_0.4.html");
    fs::write(&inventory, b"ok").unwrap();
    assert!(named_package(&package).is_ok());
    assert!(named_inventory(&inventory).is_ok());
    let wrong = dir.path().join("TLHdig_Beta_0.4.html");
    fs::rename(inventory, &wrong).unwrap();
    assert!(matches!(
        named_inventory(&wrong),
        Err(CommandError::NotInventory)
    ));
}

/// **Ничего, кроме описи, эта команда не открывает.**
///
/// Та граница, которую должна была дать область путей плагина и не дала:
/// сосед описи по каталогу – уже не опись.
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

/// Пакет читается под своим именем, где бы он ни лежал: «Собрать в
/// папку…» кладет его в папку, названную человеком.
#[test]
fn the_package_is_the_directory_the_core_names_wherever_it_lies() {
    let dir = tempfile::tempdir().unwrap();
    let package = dir.path().join(aruna::export::PACKAGE);

    assert!(named_package(&package).is_ok());
}

/// **Символьная ссылка под именем пакета или описи – не пакет и не опись.**
///
/// Р4 прогона 23.09.2026: граница сверяла только имя, и ссылка
/// `TLHdig_Beta_0.3`, положенная в папку загрузок, уводила чтение манифеста
/// куда угодно, а ссылка под именем описи отдавала opener'у то, на что она
/// указывает – хоть приложение. Ядро ни пакет, ни опись ссылкой не пишет.
#[test]
fn a_link_under_the_package_or_inventory_name_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("page.html"), "<p>x</p>").unwrap();

    let package = dir.path().join(aruna::export::PACKAGE);
    std::os::unix::fs::symlink(&elsewhere, &package).unwrap();
    assert!(
        matches!(named_package(&package), Err(CommandError::NotPackage)),
        "ссылка под именем пакета прошла как пакет"
    );

    let inventory = dir.path().join(aruna::paths::OUTPUT_FILE_NAME);
    std::os::unix::fs::symlink(elsewhere.join("page.html"), &inventory).unwrap();
    assert!(
        matches!(named_inventory(&inventory), Err(CommandError::NotInventory)),
        "ссылка под именем описи прошла как опись"
    );
}

/// **Косая черта на конце не проводит ссылку за пакет.**
#[test]
fn a_link_under_the_package_name_is_refused_with_a_trailing_slash() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    let package = dir.path().join(aruna::export::PACKAGE);
    std::os::unix::fs::symlink(&elsewhere, &package).unwrap();
    let slashed = std::path::PathBuf::from(format!("{}/", package.display()));
    assert!(
        matches!(named_package(&slashed), Err(CommandError::NotPackage)),
        "ссылка с чертой на конце прошла как пакет"
    );
}

/// **Манифест, который не обычный файл, не читается – и не держит команду.**
///
/// Ссылка ведет куда угодно, а канал без пишущего заставил бы чтение ждать
/// вечно. Без проверки вида тест падает уже на ссылке.
#[test]
fn a_manifest_that_is_not_a_plain_file_is_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let package = dir.path().join(aruna::export::PACKAGE);
    std::fs::create_dir(&package).unwrap();
    let manifest = package.join(aruna::export::MANIFEST);

    let real = dir.path().join("real.json");
    std::fs::write(&real, "{}").unwrap();
    std::os::unix::fs::symlink(&real, &manifest).unwrap();
    assert!(
        read_manifest(&package).is_none(),
        "манифест-ссылка прочитан"
    );

    std::fs::remove_file(&manifest).unwrap();
    let made = std::process::Command::new("mkfifo")
        .arg(&manifest)
        .status()
        .unwrap();
    assert!(made.success());
    assert!(read_manifest(&package).is_none(), "канал прочитан");
    assert!(matches!(
        read_xml_summary(&package),
        Err(XmlSummaryError::Unreadable)
    ));

    std::fs::remove_file(&manifest).unwrap();
    std::fs::write(&manifest, "{}").unwrap();
    assert_eq!(read_manifest(&package).as_deref(), Some("{}"));
}

/// **Каталог с другим именем читающие команды не трогают.**
///
/// Находка аудита 21.09.2026: `corpus_stats` и `corpus_xml` читали
/// `manifest.json` по любому пути, названному окном.
#[test]
fn nothing_but_the_package_is_read() {
    for other in ["/", "/etc", "/Users/someone/Documents"] {
        assert!(
            matches!(
                named_package(std::path::Path::new(other)),
                Err(CommandError::NotPackage)
            ),
            "{other} прошел как пакет"
        );
    }
}

/// **Ни в одном отказе команды нет пути файловой системы.**
///
/// Правило §3 контракта, и до 07.09.2026 его нарушал плагин, а не наш код:
/// `Not allowed to open path /Users/…/TLHdig_Beta_0.3.html` – по-английски
/// и с путем. Проверка держит уже свои тексты, чтобы правило не ушло вместе
/// с тем, кто его нарушал.
#[test]
fn no_command_failure_carries_a_path() {
    for failure in [
        CommandError::Downloads,
        CommandError::NotInventory,
        CommandError::InventoryGone,
        CommandError::Opening,
        CommandError::NotPackage,
    ] {
        let said = failure.to_string();
        assert!(
            !said.contains(std::path::MAIN_SEPARATOR),
            "отказ «{said}» несет путь"
        );
    }
}
