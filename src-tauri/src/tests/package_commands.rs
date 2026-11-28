use super::{corpus_stats, corpus_xml};
use std::fs;

const OLDER_MANIFEST: &str = r#"{"schema":1,"counts":{"documents":7,"groups":2}}"#;

#[test]
fn the_window_can_read_counts_from_a_package_without_xml_statistics() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let package = dir.path().join(aruna::export::PACKAGE);
    fs::create_dir(&package).expect("package directory");
    fs::write(package.join(aruna::export::MANIFEST), OLDER_MANIFEST).expect("old manifest");
    let path = package.display().to_string();

    let stats = corpus_stats(path.clone()).expect("the old counts are available");
    let wire = serde_json::to_value(stats).expect("the response sent to the window");
    assert_eq!(wire["source"], "manifest");
    assert_eq!(wire["manuscripts"], 7);
    assert_eq!(wire["groups"], 2);
    assert_eq!(
        corpus_xml(path).expect_err("this package predates XML statistics"),
        "манифест пакета не содержит сведений о разметке"
    );
}

#[test]
fn both_read_commands_refuse_a_manifest_outside_the_named_package() {
    let other = tempfile::tempdir().expect("directory with a different name");
    fs::write(other.path().join(aruna::export::MANIFEST), OLDER_MANIFEST).expect("old manifest");
    let path = other.path().display().to_string();

    assert_eq!(
        corpus_stats(path.clone()).expect_err("foreign directory"),
        "это не пакет корпуса"
    );
    assert_eq!(
        corpus_xml(path).expect_err("foreign directory"),
        "это не пакет корпуса"
    );
}

#[test]
fn location_reports_a_broken_edition_without_falling_back_to_downloads() {
    let downloads = tempfile::tempdir().unwrap();
    let chosen = tempfile::tempdir().unwrap();
    let package = chosen.path().join("TLHdig_Beta_0.4");
    fs::create_dir(&package).unwrap();
    fs::write(package.join("TLHdig_Beta_0.4.html"), b"inventory").unwrap();
    fs::write(package.join(aruna::export::MANIFEST), b"{broken").unwrap();
    let result = super::locate_reader(downloads.path(), Some(chosen.path().to_owned()));
    assert!(
        result.is_err(),
        "a broken selected library must be reported"
    );
}
