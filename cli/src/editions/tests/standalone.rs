use super::{future, remember};
use crate::{error::ArunaError, job::Job};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

const CHILD: &str = "ARUNA_TEST_STANDALONE_HOME";

#[test]
fn standalone_inventory_uses_the_cached_next_edition() {
    standalone_case(
        "standalone_inventory_uses_the_cached_next_edition",
        None,
        "TLHdig_Beta_0.5_Zenodo_19000000.html",
        "TLHdig, revised edition (2027-01-15)",
        |cache| {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            zip.start_file(
                "root/CTH 1_XML_HFR/KBo 1.1.xml",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(include_bytes!("../../../fixtures/xml/valid/sections.xml"))
                .unwrap();
            let bytes = zip.finish().unwrap().into_inner();
            let mut edition = future(&bytes);
            edition.version = Some("Beta Version 0.5".into());
            fs::write(
                cache.join(crate::cache::archive_name(&edition.url(), &edition.md5)),
                bytes,
            )
            .unwrap();
            remember(cache, &edition).unwrap();
        },
    );
}

// Run explicitly with ARUNA_BETA04_ZIP pointing to the verified Zenodo archive.
#[test]
#[ignore = "requires the real Beta 0.4 archive in ARUNA_BETA04_ZIP"]
fn standalone_inventory_uses_the_real_beta_04_name() {
    let zip = PathBuf::from(std::env::var_os("ARUNA_BETA04_ZIP").expect("set ARUNA_BETA04_ZIP"));
    standalone_case(
        "standalone_inventory_uses_the_real_beta_04_name",
        Some(&zip),
        "TLHdig_Beta_0.4.html",
        "Beta Version 0.4 (2026-11-19)",
        |_| {},
    );
}

/// Downloads and proxy settings are process-wide. Run the public function in a
/// child so neither setting can affect another test or the user's Downloads.
fn standalone_case(
    name: &str,
    zip: Option<&Path>,
    filename: &str,
    edition_caption: &str,
    prepare_cache: impl FnOnce(&Path),
) {
    let Some(home) = std::env::var_os(CHILD) else {
        let home = tempfile::tempdir().unwrap();
        let cache = home.path().join("cache");
        fs::create_dir(&cache).unwrap();
        prepare_cache(&cache);
        let (_, module) = module_path!().split_once("::").unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &format!("{module}::{name}"), "--include-ignored"])
            .env(CHILD, home.path())
            .env("HOME", home.path())
            .env("CFFIXED_USER_HOME", home.path())
            .env("XDG_DOWNLOAD_DIR", home.path().join("Downloads"))
            .env(crate::cache::CACHE_DIR_ENV, cache)
            // Force the normal offline path. The synthetic receipt has no live record.
            .env("HTTPS_PROXY", "http://127.0.0.1:1")
            .env("NO_PROXY", "")
            .env("no_proxy", "")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "standalone test failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
        return;
    };

    let downloads = PathBuf::from(home).join("Downloads");
    assert_eq!(crate::paths::downloads_dir().unwrap(), downloads);
    let target = downloads.join(filename);
    let run = || crate::run(zip, &Job::unattended());
    assert_eq!(run().unwrap(), target);
    let published = fs::read_to_string(&target).unwrap();
    assert!(published.contains(edition_caption));
    assert!(published.trim_end().ends_with("</html>"));
    assert_eq!(fs::read_dir(&downloads).unwrap().count(), 1);

    // A directory at the final name allows the sibling write and forces the
    // final rename to fail. The occupant and the finished HTML must survive.
    fs::remove_file(&target).unwrap();
    fs::create_dir(&target).unwrap();
    fs::write(target.join("sentinel"), b"keep").unwrap();
    let failure = run().unwrap_err();
    let message = failure.to_string();
    let ArunaError::Replace {
        path,
        scratch,
        source,
    } = failure
    else {
        panic!("expected Replace, got {failure:?}");
    };
    assert_eq!(path, target);
    assert_eq!(source.kind(), std::io::ErrorKind::IsADirectory);
    assert_eq!(scratch.parent(), Some(downloads.as_path()));
    let scratch_name = scratch.file_name().unwrap().to_str().unwrap();
    assert!(scratch_name.starts_with(&format!("{filename}.")) && scratch_name.ends_with(".part"));
    assert!(message.contains(path.to_str().unwrap()));
    assert!(message.contains(scratch.to_str().unwrap()));
    assert_eq!(fs::read(target.join("sentinel")).unwrap(), b"keep");
    assert_eq!(fs::read_dir(&target).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&downloads).unwrap().count(), 2);
    let retained = fs::read_to_string(scratch).unwrap();
    assert_eq!(without_timestamp(&retained), without_timestamp(&published));
}

fn without_timestamp(html: &str) -> (&str, &str) {
    let (before, after) = html.split_once("<span>Generated: ").unwrap();
    (before, after.split_once("</span>").unwrap().1)
}
