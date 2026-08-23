//! The document the package is read through, checked against the package.
//!
//! `export_integration.rs` checks that the inventory promises what the folder
//! holds. This file checks the document itself: that every link lands on a file
//! that is there, that what came out of the archive is escaped, and that the
//! package carries its styles rather than reaching for them.
//!
//! **There was a second document until 2026-08-23**, an `index.html` in every
//! CTH folder, and most of this file was about those pages. They were given up
//! deliberately — a CTH label is a way of reading the table, not a place to go —
//! and the inventory now links each manuscript straight at its XML. So the
//! first assertion here is the one that keeps that decision: **no `index.html`
//! anywhere in the package, ever.** Left untested, the feature could return
//! unnoticed, which is the failure mode this file exists to prevent.
//!
//! Everything is asserted against a package built on disk rather than against
//! the renderer's return value: a link is only correct if the file it names
//! exists, and that is a question about a folder.

mod support;

use aruna::export::{self, PACKAGE};
use aruna::job::Job;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use support::{archive, manuscript, Json};
use tempfile::{tempdir, TempDir};

/// A built package, and the archive it came from.
struct Package {
    _dir: TempDir,
    root: PathBuf,
}

impl Package {
    /// An archive with the shapes that make links hard: a siglum holding a
    /// slash, a siglum repeated inside one group, a group filed under two
    /// folders, and text that has to be escaped before it can be written into
    /// a document.
    fn build() -> Package {
        let dir = tempdir().expect("tempdir");
        let zip = archive(
            &dir.path().join("corpus.zip"),
            &[
                (
                    "root/CTH 5_XML_HFR/a.xml",
                    manuscript("KBo 1.1", "FB", "2017-03-28"),
                ),
                (
                    "root/CTH 5_XML_TLH/b.xml",
                    manuscript("KBo 1.1", "GM", "2019-01-02"),
                ),
                (
                    "root/CTH 5_XML_HFR/c.xml",
                    manuscript("544/f", "FB", "2017-03-28"),
                ),
                (
                    // Everything a document has to survive being written into:
                    // the five HTML-significant characters, the URL-significant
                    // ones, and a sign above the Basic Multilingual Plane.
                    "root/CTH 9_XML_HFR/d.xml",
                    manuscript("A&B <c> \"d\" #e ?f 𒀀", "M&M", "2020-01-01"),
                ),
                (
                    "root/CTH 9_XML_TLH/e.xml",
                    manuscript("KUB 2.1", "GM", "2019-01-02"),
                ),
            ],
        );
        let destination = dir.path().join("out");
        std::fs::create_dir(&destination).expect("destination");
        export::build(&zip, &destination, "test source", &Job::unattended())
            .expect("the package builds");
        Package {
            root: destination.join(PACKAGE),
            _dir: dir,
        }
    }

    fn inventory(&self) -> String {
        std::fs::read_to_string(self.root.join(format!("{PACKAGE}.html"))).expect("the inventory")
    }

    /// Every CTH directory, by name, in sorted order.
    fn groups(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.root)
            .expect("read package")
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// Every file under the package, relative to it.
    fn files(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read_dir").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    out.push(
                        path.strip_prefix(&self.root)
                            .expect("under the package")
                            .to_path_buf(),
                    );
                }
            }
        }
        out.sort();
        out
    }
}

/// Turn one `href` into the path it names, relative to the page holding it.
///
/// Percent-decoding, not string trimming: the exporter escapes a slash inside a
/// siglum into the file *name* as `%2F`, and a link to that file therefore
/// carries `%252F` — the `%` of the file name, encoded. That looks like double
/// encoding and is the opposite: decoding it once has to give the name back.
fn target_of(page_dir: &Path, href: &str) -> PathBuf {
    let decoded = percent_decode(href.strip_prefix("./").unwrap_or(href));
    let mut path = page_dir.to_path_buf();
    for part in decoded.split('/') {
        if part == ".." {
            path.pop();
        } else if !part.is_empty() && part != "." {
            path.push(part);
        }
    }
    path
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).expect("a href decodes to UTF-8")
}

/// Every `href="…"` in a document, in order.
fn hrefs(html: &str) -> Vec<String> {
    aruna::export::hrefs(html)
        .into_iter()
        .map(str::to_string)
        .collect()
}

// ---------------------------------------------------------------------------
// The decision: no page for a CTH folder
// ---------------------------------------------------------------------------

/// No `index.html` is written, anywhere, ever.
///
/// The guard on a decision rather than a property of the output. The exporter
/// wrote one into every CTH folder until 2026-08-23; re-adding it would be a
/// silent return of a feature that was given up on purpose, and a silent return
/// is exactly what an untested decision gets.
#[test]
fn the_package_carries_no_index_page_for_any_group() {
    let package = Package::build();

    let pages: Vec<PathBuf> = package
        .files()
        .into_iter()
        .filter(|p| p.file_name().and_then(|n| n.to_str()) == Some("index.html"))
        .collect();
    assert!(
        pages.is_empty(),
        "the package holds {pages:?}; CTH folders have no pages"
    );

    assert!(
        !package.inventory().contains("index.html"),
        "the inventory names a page that is not written any more"
    );
}

/// A CTH heading is text. Only manuscripts are links.
#[test]
fn a_group_heading_is_not_a_link_and_every_link_is_a_manuscript() {
    let package = Package::build();
    let inventory = package.inventory();

    assert!(
        inventory.contains("<span class=\"group-label\">"),
        "the CTH label is not plain text"
    );
    assert!(
        !inventory.contains("<a class=\"group-label\""),
        "a CTH label is a link, which is the thing that was given up"
    );

    let links = hrefs(&inventory);
    assert!(!links.is_empty(), "an inventory with no links at all");
    for href in &links {
        assert!(
            href.ends_with(".xml"),
            "the inventory links {href}, which is not a manuscript"
        );
        assert!(
            target_of(&package.root, href).is_file(),
            "{href} names a file that is not there"
        );
    }
}

/// Every manuscript in the package is linked, and nothing else is.
#[test]
fn the_inventory_links_exactly_the_manuscripts_the_folder_holds() {
    let package = Package::build();

    let on_disk: BTreeSet<PathBuf> = package
        .files()
        .into_iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("xml"))
        .collect();
    let linked: BTreeSet<PathBuf> = hrefs(&package.inventory())
        .iter()
        .map(|h| {
            target_of(&package.root, h)
                .strip_prefix(&package.root)
                .expect("under the package")
                .to_path_buf()
        })
        .collect();

    assert_eq!(
        linked, on_disk,
        "the inventory and the folder do not hold the same manuscripts"
    );
}

// ---------------------------------------------------------------------------
// The document itself
// ---------------------------------------------------------------------------

/// Nothing in the document names a path on the machine that built it.
#[test]
fn the_inventory_carries_no_absolute_path() {
    let package = Package::build();
    let html = package.inventory();

    for forbidden in ["file://", "/Users/", "/private/", "/var/folders"] {
        assert!(
            !html.contains(forbidden),
            "the document carries {forbidden}, so the package is not portable"
        );
    }
}

/// A siglum with a slash survives the trip through the file system and back.
///
/// The exporter puts `%2F` in the file *name*, so the link to it has to carry
/// `%252F`. The pair looks like a double-encoding defect and is the thing that
/// prevents one — asserted from both ends here, because the only way to be sure
/// is to decode the link and find the file.
#[test]
fn a_siglum_holding_a_separator_is_escaped_once_in_the_name_and_once_in_the_link() {
    let package = Package::build();

    let name = std::fs::read_dir(package.root.join("CTH 5"))
        .expect("read group")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| n.starts_with("544"))
        .expect("the manuscript with a slash in its siglum");
    assert_eq!(name, "544%2Ff.xml", "the separator is not in the file name");

    let href = hrefs(&package.inventory())
        .into_iter()
        .find(|h| h.contains("544"))
        .expect("the inventory links it");
    assert_eq!(
        href, "./CTH%205/544%252Ff.xml",
        "the name's % is not escaped in the link"
    );
    assert!(
        target_of(&package.root, &href).is_file(),
        "decoding the link does not give the file back"
    );

    // And the reader still sees the siglum as it is written.
    assert!(
        package.inventory().contains(">544/f<"),
        "the label was escaped as well as the path"
    );
}

/// Text out of the archive is escaped for the context it lands in.
#[test]
fn text_from_the_archive_is_escaped_in_the_document() {
    let package = Package::build();
    let html = package.inventory();

    assert!(
        html.contains("A&amp;B"),
        "an ampersand reached the document as itself"
    );
    assert!(
        html.contains("&quot;d&quot;"),
        "a quotation mark reached the document as itself"
    );
    assert!(html.contains("M&amp;M"), "an editor's name was not escaped");
    // `#` and `?` end a URL; in the link they are percent-encoded, and in the
    // text they are ordinary characters and stay as they are.
    assert!(
        html.contains("%23e%20%3Ff"),
        "the link did not escape # and ?"
    );
    assert!(
        html.contains("#e ?f"),
        "the label escaped characters it need not"
    );
    // Above the Basic Multilingual Plane, and written as itself rather than as
    // an entity — the document is UTF-8 and says so.
    assert!(
        html.contains('\u{12000}'),
        "a cuneiform sign did not survive"
    );
    // Nothing the archive carried opened an element. Asserted over the table
    // rather than the whole body: the inventory legitimately carries a
    // `<script>` of its own — the search and the folding — and the question here
    // is only what the archive's text was allowed to become.
    let rows = html
        .split("<tbody>")
        .nth(1)
        .and_then(|rest| rest.split("</tbody>").next())
        .expect("the table's rows");
    for tag in ["<c>", "<script", "<style"] {
        assert!(!rows.contains(tag), "the archive's text opened {tag}");
    }
}

// ---------------------------------------------------------------------------
// Styling: carried, never fetched
// ---------------------------------------------------------------------------

/// The package holds no stylesheet of its own; the document carries its styles.
#[test]
fn the_package_holds_no_stylesheet_of_its_own() {
    let package = Package::build();

    let stylesheets: Vec<PathBuf> = package
        .files()
        .into_iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("css"))
        .collect();
    assert!(
        stylesheets.is_empty(),
        "the package holds {stylesheets:?}; every document carries its own styles"
    );

    for name in ["styles", "css", "assets", "static", "fonts", "theme"] {
        assert!(
            !package.root.join(name).exists(),
            "the exporter created a `{name}` directory the package does not need"
        );
    }
}

/// The document reaches for nothing outside itself.
///
/// Not only `.css`: a `<link rel="stylesheet">` at any URL would make the
/// package depend on something that is not in it, and one pointing at a CDN
/// would make it depend on the network.
#[test]
fn the_inventory_links_no_stylesheet_or_anything_else_off_the_disk() {
    let package = Package::build();
    let html = package.inventory();

    assert!(
        !html.contains("<link"),
        "the document links something instead of carrying it"
    );
    for remote in ["http://", "https://", "//fonts.", "cdn."] {
        assert!(
            !html.contains(remote),
            "the document reaches for {remote}, so it needs the network"
        );
    }
    assert!(
        html.contains("<style>"),
        "the document carries no styles at all"
    );
}

/// The document knows how to print itself.
#[test]
fn the_inventory_carries_screen_and_print_rules() {
    let package = Package::build();
    let html = package.inventory();

    assert!(html.contains("@media print"), "no print rules");
    assert!(html.contains("@page"), "no page box");
    assert!(
        html.contains(":root {"),
        "no design tokens, so nothing shares a value with anything"
    );
}

// ---------------------------------------------------------------------------
// The package as a whole
// ---------------------------------------------------------------------------

/// The folder holds the inventory, the manifest and the manuscripts. Nothing
/// else.
///
/// A build that left an intermediate representation, a temporary file or a
/// backup behind would still pass every link check above.
#[test]
fn the_package_holds_only_what_a_reader_needs() {
    let package = Package::build();

    for file in package.files() {
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .expect("a file name");
        let allowed =
            name.ends_with(".xml") || name == format!("{PACKAGE}.html") || name == "manifest.json";
        assert!(allowed, "the package holds {}", file.display());
        for junk in [".bak", ".tmp", ".part", ".orig", "~"] {
            assert!(
                !name.ends_with(junk),
                "the package holds {}, which is debris",
                file.display()
            );
        }
    }
}

/// The manifest, the inventory and the folder agree about the groups.
///
/// Three independent descriptions of one package; the exporter writes them from
/// one model, and this is what says so. The inventory's opinion is now read off
/// its fragment links — the folder each one goes through — because it no longer
/// names a group in any other way.
#[test]
fn the_manifest_the_inventory_and_the_folder_name_the_same_groups() {
    let package = Package::build();

    let manifest = Json::parse(
        &std::fs::read_to_string(package.root.join("manifest.json")).expect("manifest"),
    )
    .expect("the manifest is JSON");

    let from_folder: BTreeSet<String> = package.groups().into_iter().collect();
    let from_inventory: BTreeSet<String> = hrefs(&package.inventory())
        .into_iter()
        .map(|h| {
            let decoded = percent_decode(h.strip_prefix("./").unwrap_or(&h));
            decoded
                .split('/')
                .next()
                .expect("a group directory")
                .to_string()
        })
        .collect();

    assert_eq!(
        from_folder, from_inventory,
        "the inventory and the folder name different groups"
    );

    // The manifest is the third description of the same package, and it is the
    // machine-readable one: `groups[].label` must be the folders the package
    // has, and every document it names must sit under one of them.
    let groups = manifest
        .get("groups")
        .and_then(Json::as_arr)
        .expect("the manifest lists groups");
    assert!(!groups.is_empty());

    let from_manifest: BTreeSet<String> = groups
        .iter()
        .map(|g| {
            g.get("label")
                .and_then(Json::as_str)
                .expect("a group is labelled")
                .to_string()
        })
        .collect();
    assert_eq!(
        from_manifest, from_folder,
        "the manifest and the folder name different groups"
    );

    let linked: BTreeSet<String> = hrefs(&package.inventory())
        .into_iter()
        .map(|h| percent_decode(h.strip_prefix("./").unwrap_or(&h)))
        .collect();

    for group in groups {
        let label = group.get("label").and_then(Json::as_str).expect("a label");
        let documents = group
            .get("documents")
            .and_then(Json::as_arr)
            .expect("a group lists its documents");
        assert!(!documents.is_empty(), "{label}: a group with no documents");

        for document in documents {
            let file = document
                .get("file")
                .and_then(Json::as_str)
                .expect("a document names its file");
            assert!(
                package.root.join(file).is_file(),
                "the manifest names {file}, which is not in the package"
            );
            assert!(
                file.starts_with(&format!("{label}/")),
                "the manifest files {file} under {label}, which is not where it is"
            );
            assert!(
                linked.contains(file),
                "{file} is in the manifest and not linked from the inventory"
            );
        }
    }
}
