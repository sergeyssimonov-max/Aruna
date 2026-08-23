//! The catalog against the assumptions a reader of it has to make.
//!
//! This was written when `catalog::render` fed `scripts/build-inventory-bin.mjs`,
//! which packed the ARUN container the site downloaded. **That reader was
//! deleted with the React site on 2026-08-23**, so the test no longer guards a
//! live seam — it guards the *format*, which is the reason it was kept: the
//! document is still emitted by `emit_inventory_json`, and the failures below
//! are the ones any reader of it would hit.
//!
//! They are worth pinning because most of them fail *quietly* rather than
//! loudly:
//!
//! * `pool[ai] ?? "—"` — a metadata index past the end of the pool is not an
//!   error there, it is an em dash in a table cell for every visitor.
//! * `const [siglum, ai, yi, li = 0, ii = 0, ci = 0] = row` — a row with fewer
//!   than six fields is not an error either; the missing ones default to
//!   `pool[0]`, which is a real string belonging to a different manuscript.
//! * A group label the container cannot carry *is* refused — loudly, at site
//!   build time, in a job that needs the 71 MiB archive.
//!
//! So this file asserts, in Rust and without Node, the properties the builder
//! relies on and does not verify. The catalog's own unit tests cover what it
//! puts in the document; these cover what the document has to be for the other
//! half to read it correctly.

mod support;

use aruna::catalog;
use aruna::parse::{group_label, ManuscriptRecord};
use support::{mixed_archive, Json};
use tempfile::tempdir;

/// The version `catalog::render` stamps, and the one the format is at.
///
/// Written here as a literal rather than read from the crate: the constant is
/// private, and a test that imported it could not tell a deliberate bump from
/// an accidental one. Bumping the format means changing this line too, which is
/// the point — see `the_wire_version_is_the_one_the_builder_expects` below.
const WIRE_VERSION: i64 = 2;

/// Records from a real parse, so the document under test is one the program
/// actually produces rather than one the test composed.
fn records() -> Vec<ManuscriptRecord> {
    let dir = tempdir().expect("tempdir");
    let zip = mixed_archive(dir.path());
    aruna::archive::parse_zip(&zip, &aruna::job::Job::unattended()).expect("the archive parses")
}

fn catalog_of(records: &[ManuscriptRecord]) -> Json {
    let rendered = catalog::render(records, "test source");
    Json::parse(&rendered.json)
        .unwrap_or_else(|e| panic!("the catalog is not JSON any reader would accept: {e}"))
}

/// The document is an object with exactly the five members the builder reads.
///
/// A sixth member is data a reader silently drops; a missing one is a
/// `TypeError` in whatever destructures it. Both are worth catching in the
/// language that writes the document.
#[test]
fn the_catalog_carries_exactly_the_members_the_builder_reads() {
    let catalog = catalog_of(&records());
    let mut keys = catalog.keys();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["g", "m", "p", "s", "v"],
        "the catalog's members are not the five a reader destructures"
    );

    assert!(
        catalog.get("s").and_then(Json::as_str).is_some(),
        "s: source"
    );
    assert!(
        catalog.get("m").and_then(Json::as_int).is_some(),
        "m: count"
    );
    assert!(catalog.get("p").and_then(Json::as_arr).is_some(), "p: pool");
    assert!(
        catalog.get("g").and_then(Json::as_arr).is_some(),
        "g: groups"
    );
    assert!(
        catalog.get("v").and_then(Json::as_int).is_some(),
        "v: version"
    );
}

/// The version is a number, and it is this one.
///
/// `v` exists so a reshaped catalog can be refused instead of misread. Nothing
/// downstream had ever looked at it; the reader that finally did —
/// `scripts/build-inventory-bin.mjs`, with its own test as the other end of
/// this assertion — was deleted on 2026-08-23, which leaves this the only
/// thing holding the version honest.
#[test]
fn the_wire_version_is_the_one_the_builder_expects() {
    let catalog = catalog_of(&records());
    assert_eq!(
        catalog.get("v").and_then(Json::as_int),
        Some(WIRE_VERSION),
        "the catalog's version moved; every reader of the document has to move with it"
    );
}

/// Every metadata field is an index that lands inside the pool.
///
/// This is the one that fails silently downstream. `pool[ai] ?? "—"` turns an
/// index past the end into an em dash, so a catalog with an out-of-range id
/// builds, ships, and shows blank metadata for whichever manuscripts carried
/// it. Nothing anywhere would have said so.
#[test]
fn every_metadata_index_lands_inside_the_pool() {
    let catalog = catalog_of(&records());
    let pool_size = catalog
        .get("p")
        .and_then(Json::as_arr)
        .expect("a pool")
        .len() as i64;

    for (label, row) in rows(&catalog) {
        let siglum = row[0].as_str().expect("the siglum is a string");
        for (field, value) in ["auth", "year", "lang", "inv", "corpus"]
            .iter()
            .zip(&row[1..])
        {
            let id = value.as_int().unwrap_or_else(|| {
                panic!(
                    "{label}/{siglum}: {field} is a {}, not an index",
                    value.kind()
                )
            });
            assert!(
                (0..pool_size).contains(&id),
                "{label}/{siglum}: {field} is index {id} into a pool of {pool_size} — \
                 a reader would show an em dash here and nothing would report it"
            );
        }
    }
}

/// Every row has the six fields the builder destructures, no more and no fewer.
///
/// A short row is not an error for a reader: `li = 0, ii = 0, ci = 0` fills
/// the gaps with pool entry zero, which is another manuscript's metadata shown
/// against this one. A long row is data dropped in silence.
#[test]
fn every_row_has_the_six_fields_the_builder_destructures() {
    let catalog = catalog_of(&records());
    for (label, row) in rows(&catalog) {
        assert_eq!(
            row.len(),
            6,
            "{label}: a row of {} fields — the builder reads six and fills or drops the rest",
            row.len()
        );
    }
}

/// `m` is the number of manuscripts the document actually carries.
///
/// It is written from `records.len()` and the rows are written from the same
/// slice, so the two can only disagree through a change to the renderer — and
/// `m` is what the container's header publishes and the page prints as its
/// count.
#[test]
fn the_count_in_the_header_is_the_number_of_rows() {
    let records = records();
    let catalog = catalog_of(&records);
    assert_eq!(
        catalog.get("m").and_then(Json::as_int),
        Some(records.len() as i64)
    );
    assert_eq!(rows(&catalog).len(), records.len());
}

/// Groups are the runs the records already form, in the order they arrive.
///
/// The site lists manuscripts in the order the CLI does, and that is the whole
/// reason `group_runs` walks runs rather than collecting a map. A catalog whose
/// groups had been reordered or merged would still build and still render — as
/// a different inventory.
#[test]
fn the_groups_are_the_record_runs_in_record_order() {
    let records = records();
    let catalog = catalog_of(&records);

    let from_catalog: Vec<(String, usize)> = catalog
        .get("g")
        .and_then(Json::as_arr)
        .expect("groups")
        .iter()
        .map(|group| {
            let pair = group.as_arr().expect("a group is a pair");
            (
                pair[0].as_str().expect("a label").to_string(),
                pair[1].as_arr().expect("rows").len(),
            )
        })
        .collect();

    let from_records: Vec<(String, usize)> = aruna::parse::group_runs(&records)
        .map(|run| (group_label(&run[0]).to_string(), run.len()))
        .collect();

    assert_eq!(
        from_catalog, from_records,
        "the catalog's groups are not the runs the records form"
    );

    // The sigla, in order, are the records' sigla in order — the assertion the
    // shape check above cannot make.
    let catalogued: Vec<&str> = rows(&catalog)
        .iter()
        .map(|(_, row)| row[0].as_str().expect("a siglum"))
        .collect();
    let parsed: Vec<&str> = records.iter().map(|r| r.sigla.as_str()).collect();
    assert_eq!(catalogued, parsed);
}

/// Only one label shape reaches the builder besides `CTH <n>`, and it is the
/// one the builder refuses.
///
/// ARUN stores a group as a `u16` and the reader rebuilds the label as
/// `CTH ${n}`, so `build-inventory-bin.mjs` throws on anything that would not
/// come back the same. `group_label` returns the dash for a record with no CTH
/// at all — a manuscript the corpus has never had, and one that would stop the
/// site's data build rather than corrupt it.
///
/// Pinned so that stays a known, deliberate coupling: a release that introduced
/// `CTH 12.1`, or an ungrouped manuscript, fails here — in a test that needs no
/// archive and no Node — rather than in the job that needs 71 MiB.
#[test]
fn the_only_label_the_container_cannot_carry_is_the_one_for_no_group() {
    let records = records();
    let catalog = catalog_of(&records);
    for group in catalog.get("g").and_then(Json::as_arr).expect("groups") {
        let label = group.as_arr().expect("a pair")[0]
            .as_str()
            .expect("a label");
        let carried = label
            .strip_prefix("CTH ")
            .and_then(|n| n.parse::<u32>().ok())
            .is_some_and(|n| n <= u32::from(u16::MAX) && format!("CTH {n}") == label);
        assert!(
            carried,
            "group {label:?} is not a label ARUN can carry; \
             scripts/build-inventory-bin.mjs refuses it and the site's data build stops"
        );
    }

    // And the one shape that is not carried is reachable, so the coupling above
    // is a live constraint rather than a tautology about this fixture.
    let ungrouped: Vec<ManuscriptRecord> = records
        .iter()
        .cloned()
        .map(|mut r| {
            r.cth = None;
            r
        })
        .collect();
    let label_for_none = group_label(&ungrouped[0]).to_string();
    assert!(
        label_for_none.strip_prefix("CTH ").is_none(),
        "a record with no CTH now produces {label_for_none:?}, which the container would carry — \
         the refusal in scripts/build-inventory-bin.mjs is no longer the thing that catches it"
    );
}

/// An empty corpus is still a document the builder can read.
///
/// The pipeline refuses an archive with no manuscripts long before this, so the
/// case is unreachable from the CLI — but `catalog::render` is a public
/// function, and a document that was `undefined.map` on the other side would be
/// a poor way to find that out.
#[test]
fn an_empty_catalog_is_still_a_document() {
    let catalog = catalog_of(&[]);
    assert_eq!(catalog.get("m").and_then(Json::as_int), Some(0));
    assert_eq!(
        catalog.get("p").and_then(Json::as_arr).map(<[Json]>::len),
        Some(0)
    );
    assert_eq!(
        catalog.get("g").and_then(Json::as_arr).map(<[Json]>::len),
        Some(0)
    );
    assert_eq!(catalog.get("v").and_then(Json::as_int), Some(WIRE_VERSION));
}

/// The catalog holds no booleans and no nulls, anywhere.
///
/// The reader accepts both, because the manifest next door uses `false` and one
/// reader for the crate is better than one per document. What the *catalog* may
/// contain is this file's business: `pool[ai] ?? "—"` on the Node side turns a
/// `null` where an index belongs into an em dash in a table cell, and
/// destructuring a `true` gives a pool lookup at index `undefined`. Neither is
/// an error there, so it has to be one here.
#[test]
fn the_catalog_holds_no_booleans_or_nulls() {
    let catalog = catalog_of(&records());
    for value in catalog.walk() {
        assert!(
            !matches!(value, Json::Bool(_) | Json::Null),
            "the catalog carries a {} — the builder would read it as an index",
            value.kind()
        );
    }
}

/// Text that would break the document is escaped rather than emitted.
///
/// The catalog's own unit test covers the escaping; this one covers the result
/// — that a source string carrying a quote, a backslash and a control character
/// still parses. A document that did not would fail in `JSON.parse` at site
/// build time with a byte offset and no idea which manuscript caused it.
#[test]
fn text_that_would_break_the_document_survives_a_parse() {
    let hostile = "a \"quoted\" \\ backslash \u{1} control \u{7f} delete\nnewline\ttab";
    let rendered = catalog::render(&records(), hostile);
    let catalog = Json::parse(&rendered.json)
        .unwrap_or_else(|e| panic!("hostile source text broke the document: {e}"));
    assert_eq!(catalog.get("s").and_then(Json::as_str), Some(hostile));
}

// ---------------------------------------------------------------------------
// The instrument, checked before it is trusted
// ---------------------------------------------------------------------------

/// A reader that accepts anything proves nothing. These are the shapes a broken
/// catalog would actually take, and the parser above has to refuse each one —
/// otherwise the assertions in this file pass on documents Node would reject.
#[test]
fn the_reader_refuses_the_documents_a_broken_catalog_would_produce() {
    for bad in [
        r#"{"v":2"#,                     // ends inside the object
        r#"{"v":2}{"v":2}"#,             // two documents in one file
        r#"{"v":2,}"#,                   // trailing comma
        r#"{"p":[1,]}"#,                 // trailing comma in an array
        r#"{"v":tru}"#,                  // a bare word that only starts like one
        r#"{"v":nul}"#,                  //   "
        r#"{"m":1.5}"#,                  // a fraction where a count belongs
        r#"{"m":1e3}"#,                  //   "
        r#"{"s":"unterminated}"#,        // ends inside a string
        "{\"s\":\"raw \u{1} control\"}", // the escaping's whole job, undone
        r#"{"s":"\q"}"#,                 // an escape no writer produces
        r#"{v:2}"#,                      // an unquoted key
        r#""#,                           // nothing at all
    ] {
        assert!(
            Json::parse(bad).is_err(),
            "the reader accepted {bad:?}, so it cannot be trusted to reject a real defect"
        );
    }
}

/// And it accepts the shapes the catalog really contains, including the escapes
/// its writer produces and text outside the Basic Multilingual Plane — the
/// corpus is 376 cuneiform signs above it.
#[test]
fn the_reader_accepts_what_the_catalog_really_contains() {
    let document = r#"{"s":"a \"q\" \\ \u0001 \n","m":2,"p":["𒀀","—"],"g":[["CTH 5",[["KBo 1.1",0,1,1,1,1]]]],"v":2}"#;
    let value = Json::parse(document).expect("a well-formed catalog parses");
    assert_eq!(
        value.get("s").and_then(Json::as_str),
        Some("a \"q\" \\ \u{1} \n")
    );
    assert_eq!(
        value.get("p").and_then(Json::as_arr).map(|p| p[0].clone()),
        Some(Json::Str("𒀀".to_string())),
        "a cuneiform sign above the BMP survived the reader"
    );
}

// ---------------------------------------------------------------------------

/// Every row in the document, with the label of the group it sits in.
fn rows(catalog: &Json) -> Vec<(&str, &[Json])> {
    let mut out = Vec::new();
    for group in catalog.get("g").and_then(Json::as_arr).expect("groups") {
        let pair = group.as_arr().expect("a group is [label, rows]");
        assert_eq!(pair.len(), 2, "a group is a pair of label and rows");
        let label = pair[0].as_str().expect("a label is a string");
        for row in pair[1].as_arr().expect("rows are an array") {
            out.push((label, row.as_arr().expect("a row is an array")));
        }
    }
    out
}
