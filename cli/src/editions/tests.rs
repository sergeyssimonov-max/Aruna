use super::*;
use crate::job::Job;
use serde_json::json;

fn future(bytes: &[u8]) -> Edition {
    let mut v: Value = serde_json::from_str(BETA04).unwrap();
    // The order is the publisher's version index, not the record id or a Beta suffix.
    v["id"] = json!(19000000);
    v["doi"] = json!("10.5281/zenodo.19000000");
    v["metadata"]["relations"]["version"][0]["index"] = json!(3);
    v["metadata"]["title"] = json!("TLHdig, revised edition");
    v["metadata"]["publication_date"] = json!("2027-01-15");
    v["metadata"]["creators"] = json!([{"name":"Example <Editor>"}]);
    v["files"] = json!([{"key":"notes.txt"}, {"key":"edition.zip", "size":bytes.len(), "checksum":format!("md5:{}", crate::md5::md5_hex(bytes))}]);
    Edition::parse(&v.to_string()).unwrap()
}

#[test]
fn the_two_deposits_are_editions_of_the_same_collection_and_their_references_are_distinct() {
    let old = Edition::parse(BETA03).unwrap();
    let new = reference().unwrap();
    assert_eq!((old.record, old.index, old.bytes), (20328284, 1, 74449198));
    assert_eq!((new.record, new.index, new.bytes), (24373727, 2, 78999594));
    assert_eq!(new.md5, "a31a1ac9808ac77432466a910ed85969");
    assert_eq!(new.caption(), "Издание корпуса – Beta 0.4 (2026-11-19).");
    assert!(new.citation().contains("10.5281/zenodo.24373727"));
    assert_eq!(old.package_name(), "TLHdig_Beta_0.3");
    assert_eq!(new.package_name(), "TLHdig_Beta_0.4");
    assert!(new.follows(&old).is_ok());
    assert!(matches!(
        old.follows(&new),
        Err(ArunaError::Edition(EditionIssue::Older))
    ));
}

#[test]
fn only_a_record_marked_current_by_zenodo_can_start_an_update() {
    let mut record: Value = serde_json::from_str(BETA04).unwrap();
    assert_eq!(
        current_record(&record.to_string()).unwrap().record,
        24373727
    );
    for marker in [json!(false), Value::Null] {
        record["metadata"]["relations"]["version"][0]["is_last"] = marker;
        assert!(matches!(
            current_record(&record.to_string()),
            Err(ArunaError::Edition(EditionIssue::Record))
        ));
        // It still identifies its own edition for offline use and citation.
        assert!(Edition::parse(&record.to_string()).is_some());
    }
}

#[test]
fn a_future_archive_name_is_encoded_as_one_url_segment_and_fits_the_cache() {
    let mut record: Value = serde_json::from_str(BETA04).unwrap();
    record["files"][0]["key"] = json!("Édition TLHdig #1%.zip");
    let edition = Edition::parse(&record.to_string()).unwrap();
    assert!(edition
        .url()
        .ends_with("/%C3%89dition%20TLHdig%20%231%25.zip?download=1"));
    record["files"][0]["key"] = json!(format!("{}.zip", "𒀀".repeat(60)));
    let edition = Edition::parse(&record.to_string()).unwrap();
    assert!(crate::cache::archive_name(&edition.url(), &edition.md5).len() <= 255);
}

#[test]
fn an_unrelated_restricted_ambiguous_or_incomplete_publication_is_refused() {
    for (pointer, value) in [
        ("/conceptrecid", json!("999")),
        ("/doi", json!("10.5281/zenodo.1")),
        ("/status", json!("draft")),
        ("/metadata/access_right", json!("restricted")),
        ("/metadata/license/id", json!("other")),
        (
            "/metadata/relations/version/0/parent/pid_value",
            json!("999"),
        ),
        ("/metadata/publication_date", json!("2026-02-30")),
        ("/metadata/creators", json!([])),
        ("/files/0/key", json!("../foreign.zip")),
        ("/files/0/key", json!("archive.txt")),
        ("/files/0/size", json!(crate::download::MAX_DOWNLOAD + 1)),
        ("/files/0/checksum", json!("md5:12")),
        ("/files", json!([{"key":"one.zip"},{"key":"two.zip"}])),
    ] {
        let mut v: Value = serde_json::from_str(BETA04).unwrap();
        *v.pointer_mut(pointer).unwrap() = value;
        assert!(Edition::parse(&v.to_string()).is_none(), "{pointer}");
    }
}

fn unavailable() -> Result<Edition> {
    Err(ArunaError::Network {
        url: LATEST.into(),
        source: std::io::Error::other("offline").into(),
    })
}

#[test]
fn an_old_cache_hit_cannot_hide_a_new_edition_and_offline_uses_only_verified_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = b"archive of a later edition";
    let old = reference().unwrap();
    let later = future(bytes);
    let archive = dir
        .path()
        .join(crate::cache::archive_name(&later.url(), &later.md5));
    let (got, chosen) = select(
        Some(&old),
        &Job::unattended(),
        || Ok(later.clone()),
        Some(dir.path()),
        |e, _| {
            assert_eq!(e, &later);
            fs::write(&archive, bytes).unwrap();
            Ok(crate::cache::Archive::Cached(archive.clone()))
        },
    )
    .unwrap();
    assert_eq!(got.path(), archive);
    remember(dir.path(), &chosen);
    for installed in [Some(&chosen), None] {
        let (got, e) = select(
            installed,
            &Job::unattended(),
            unavailable,
            Some(dir.path()),
            |_, _| panic!("offline made a request"),
        )
        .unwrap();
        assert_eq!(e, later);
        assert_eq!(fs::read(got.path()).unwrap(), bytes);
    }
    fs::write(archive, b"damaged copy").unwrap();
    assert!(matches!(
        select(
            Some(&chosen),
            &Job::unattended(),
            unavailable,
            Some(dir.path()),
            |_, _| panic!()
        ),
        Err(ArunaError::Edition(EditionIssue::Offline))
    ));
}

#[test]
fn bad_metadata_and_conflicting_or_older_records_never_start_the_download() {
    let old = future(b"old");
    let mut changed = old.clone();
    changed.md5 = "0".repeat(32);
    for response in [
        Err(EditionIssue::Record.into()),
        Ok(changed),
        Ok(reference().unwrap()),
    ] {
        assert!(matches!(
            select(
                Some(&old),
                &Job::unattended(),
                || response,
                None,
                |_, _| panic!("invalid edition fetched")
            ),
            Err(ArunaError::Edition(_))
        ));
    }
}

#[test]
fn after_an_update_the_manifest_inventory_and_pdf_agree_and_failed_updates_leave_every_byte_in_place(
) {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let zip = dir.path().join("fixture.zip");
    let write_archive = |entries: &[(&str, &str)]| {
        let mut writer = zip::ZipWriter::new(fs::File::create(&zip).unwrap());
        for &(name, xml) in entries {
            writer
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(xml.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
    };
    let first_path = "test/CTH 1_XML_HFR/KBo 1.1.xml";
    let second_path = "test/CTH 2_XML_HFR/KBo 2.1.xml";
    let original = include_str!("../../fixtures/xml/valid/sections.xml");
    write_archive(&[(first_path, original)]);
    let bytes = fs::read(&zip).unwrap();
    let e = future(&bytes);
    let fonts =
        crate::pdf::Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/fonts"))
            .unwrap();
    let build = |e: &Edition| {
        crate::export::build_within(
            &zip,
            dir.path(),
            crate::export::Origin::Zenodo(e),
            &fonts,
            &Job::unattended(),
            crate::export::MAX_PACKAGE,
        )
    };
    let (report, published) = build(&e).unwrap();
    assert_eq!(
        published,
        dir.path().join("TLHdig_Edition_3_Zenodo_19000000")
    );
    assert_eq!((report.documents, report.pdfs.built), (1, 1));
    let (root, inventory) = library_paths(dir.path());
    assert_eq!(root, published);
    assert_eq!(installed(&root).unwrap(), Some(e.clone()));
    let html = fs::read_to_string(&inventory).unwrap();
    assert!(
        !html.contains("DOI 10.5281/zenodo.19000000")
            && !html.contains("CC BY 4.0")
            && html.contains("2027-01-15")
            && html.contains("Example &lt;Editor&gt;")
            && !html.contains("Beta 0.3")
    );
    fn snapshot(
        root: &Path,
        path: &Path,
        files: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>,
    ) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                snapshot(root, &path, files)
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut before = std::collections::BTreeMap::new();
    snapshot(&root, &root, &mut before);
    // Keep one manuscript's name but correct its text, and add another.
    let revised = original.replace("first section", "corrected reading");
    let added = include_str!("../../fixtures/xml/valid/minimal.xml").replace("KBo 1.1", "KBo 2.1");
    let unreadable = added.replace("</text>", "<x:y/></text>");
    write_archive(&[(first_path, &revised), (second_path, &unreadable)]);
    let mut next = future(&fs::read(&zip).unwrap());
    next.record += 2;
    next.index += 1;
    next.title = "Release 12.1-rc2".into();
    let (updated, next_root) = build(&next).unwrap();
    assert_eq!(
        (updated.documents, updated.pdfs.built, updated.pdfs.refused),
        (2, 1, 1)
    );
    assert_eq!(
        next_root,
        dir.path()
            .join("TLHdig_Label_Release_12.1-rc2_Zenodo_19000002")
    );
    let old_pdf = before
        .keys()
        .find(|p| p.extension().is_some_and(|s| s == "pdf"))
        .unwrap();
    assert_ne!(fs::read(next_root.join(old_pdf)).unwrap(), before[old_pdf]);
    assert_eq!(
        fs::read(next_root.join(old_pdf.with_extension("xml"))).unwrap(),
        crate::export::normalize_document(revised.as_bytes())
    );
    assert_eq!(library_paths(dir.path()).0, next_root);
    let manifest: Value =
        serde_json::from_slice(&fs::read(next_root.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["package"], next.package_name());
    assert_eq!(manifest["inventory"], edition_inventory(Some(&next)));
    assert!(next_root.join(edition_inventory(Some(&next))).is_file());
    let inv = fs::read_to_string(next_root.join(edition_inventory(Some(&next)))).unwrap();
    let refused = crate::export::hrefs(&inv)
        .into_iter()
        .find(|s| s.ends_with(".xml"))
        .unwrap();
    // A hidden current edition must be found even while a different, older
    // working edition is still visible in the same destination.
    let backup = dir
        .path()
        .join(format!(".{}.previous", next.package_name()));
    fs::rename(&next_root, &backup).unwrap();
    let (restored, inventory) = crate::export::reader_paths(dir.path()).unwrap();
    assert_eq!(restored, next_root);
    assert_eq!(fs::read_to_string(inventory).unwrap(), inv);
    // The publisher reuses the label, withdraws the first document and repairs the second.
    write_archive(&[(second_path, &added)]);
    let mut final_edition = future(&fs::read(&zip).unwrap());
    final_edition.title.clone_from(&next.title);
    final_edition.record = next.record + 1;
    final_edition.index = next.index + 1;
    let (corrected, corrected_root) = build(&final_edition).unwrap();
    assert_ne!(corrected_root, next_root);
    assert_eq!(library_paths(dir.path()).0, corrected_root);
    assert_eq!(
        (
            corrected.documents,
            corrected.pdfs.built,
            corrected.pdfs.refused
        ),
        (1, 1, 0)
    );
    assert!(!corrected_root.join(old_pdf).exists());
    let corrected_inventory =
        fs::read_to_string(corrected_root.join(edition_inventory(Some(&final_edition)))).unwrap();
    assert!(crate::export::hrefs(&corrected_inventory)
        .contains(&refused.replace(".xml", ".pdf").as_str()));
    fs::write(&zip, &bytes).unwrap();
    let mut stale = e.clone();
    stale.index -= 1;
    stale.record += 1;
    assert!(matches!(
        build(&stale),
        Err(ArunaError::Edition(EditionIssue::Older))
    ));
    let mut altered = e.clone();
    altered.md5 = "a".repeat(32);
    assert!(matches!(
        build(&altered),
        Err(ArunaError::ChecksumMismatch { .. })
    ));
    let mut changed = zip::ZipWriter::new_append(
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&zip)
            .unwrap(),
    )
    .unwrap();
    changed
        .start_file(
            "test/CTH 2_XML_TLH/a.xml",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
    changed.write_all(b"<unknown-transcription/>").unwrap();
    changed.finish().unwrap();
    let changed_edition = future(&fs::read(&zip).unwrap());
    assert!(matches!(
        build(&changed_edition),
        Err(ArunaError::Edition(EditionIssue::Markup))
    ));
    fs::write(&zip, b"not a ZIP").unwrap();
    assert!(build(&e).is_err());
    let mut after = std::collections::BTreeMap::new();
    snapshot(&root, &root, &mut after);
    assert_eq!(before, after);
}

#[test]
fn only_recognized_versions_can_be_used_as_path_components() {
    let mut e = future(b"test");
    for (title, name) in [
        ("Beta Version 0.12", "TLHdig_Beta_0.12_Zenodo_19000000"),
        (
            "Release 12.1-rc2",
            "TLHdig_Label_Release_12.1-rc2_Zenodo_19000000",
        ),
        ("Version 1.0", "TLHdig_Version_1.0_Zenodo_19000000"),
        ("Beta Version ../bad", "TLHdig_Edition_3_Zenodo_19000000"),
    ] {
        e.title = title.into();
        assert_eq!(e.package_name(), name);
        assert!(is_package_name(name));
    }
    let mut record: Value = serde_json::from_str(BETA04).unwrap();
    record["metadata"]["version"] = json!("Весеннее издание");
    let mut explicit = Edition::from_record(&record).unwrap();
    explicit.record = 77;
    assert_eq!(
        explicit.package_name(),
        "TLHdig_Label_Весеннее_издание_Zenodo_77"
    );
    assert!(is_package_name(&explicit.package_name()));
    for name in [
        "TLHdig_Label_a/b_Zenodo_7",
        "TLHdig_Beta_../bad_Zenodo_7",
        "TLHdig_Beta_0..5_Zenodo_7",
        "TLHdig_Edition_0_Zenodo_7",
        "TLHdig_Version_1_Zenodo_0",
    ] {
        assert!(!is_package_name(name));
    }
}

#[test]
fn a_versioned_copy_takes_precedence_over_the_same_edition_in_the_neutral_folder() {
    let dir = tempfile::tempdir().unwrap();
    let edition = reference().unwrap();
    for name in [crate::export::PACKAGE.to_owned(), edition.package_name()] {
        let root = dir.path().join(&name);
        fs::create_dir(&root).unwrap();
        fs::write(root.join(inventory_name(&root)), b"inventory").unwrap();
        fs::write(
            root.join(crate::export::MANIFEST),
            json!({"source":{"archive_md5": edition.md5, "zenodo":edition}}).to_string(),
        )
        .unwrap();
        assert_eq!(library_paths(dir.path()).0, root);
    }
    assert!(dir.path().join("TLHdig/TLHdig.html").is_file());
}

#[test]
fn historical_paths_are_readable_and_a_working_current_inventory_takes_precedence() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join(crate::export::LEGACY_PACKAGE);
    fs::create_dir(&old).unwrap();
    fs::write(old.join(crate::export::MANIFEST), b"{\"counts\":{}}").unwrap();
    assert_eq!(installed(&old).unwrap(), None);
    let inv = old.join(crate::paths::LEGACY_INVENTORY);
    fs::write(&inv, b"original").unwrap();
    fs::write(
        old.join(crate::export::MANIFEST),
        json!({"source":{"archive_md5":crate::download::ZENODO_ZIP_MD5}}).to_string(),
    )
    .unwrap();
    assert_eq!(library_paths(dir.path()), (old.clone(), inv.clone()));
    assert_eq!(installed(&old).unwrap().unwrap().record, 20328284);
    let current = dir.path().join(crate::export::PACKAGE);
    fs::create_dir(&current).unwrap();
    assert_eq!(library_paths(dir.path()).0, old);
    fs::write(current.join(crate::paths::OUTPUT_FILE_NAME), b"new").unwrap();
    assert_eq!(library_paths(dir.path()).0, current);
    assert_eq!(fs::read(inv).unwrap(), b"original");
}
