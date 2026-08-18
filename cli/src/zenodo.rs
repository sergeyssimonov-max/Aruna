//! Asking Zenodo about the record before downloading from it.
//!
//! The archive itself is fetched as one file — see [`crate::download`] — and
//! that is the right shape: the record holds a single 71 MiB ZIP and no
//! per-document access, so reading it in pieces would mean 23 937 range
//! requests against a 133-per-minute rate limit, three hours to save 29 MiB.
//!
//! What the API is good for is the two things the program otherwise guesses at:
//! the digest the archive is published with, and whether a newer edition of the
//! corpus exists. Both come from one request.
//!
//! Nothing here is required. The metadata request is made only when the archive
//! is about to be downloaded anyway, and a failure is reported rather than
//! raised: a repository that will not answer questions about a file can still
//! serve it.

use crate::error::{ArunaError, Result};
use std::time::Duration;

mod json;

/// The published state of one Zenodo record.
pub struct Release {
    /// Record id — a new edition of the corpus is a new number.
    pub record_id: u64,
    /// Name of the single archive the record holds.
    pub file: String,
    /// MD5 as Zenodo publishes it, without the `md5:` prefix it arrives with.
    pub md5: Option<String>,
    /// Publication date, `YYYY-MM-DD`.
    pub published: Option<String>,
}

/// How long to wait for metadata.
///
/// Short on purpose: this is an aside, not the work. The document is a few tens
/// of KiB, so a connection that cannot deliver it in ten seconds will not be
/// delivering 71 MiB either — and the cost of giving up early is only that the
/// run goes ahead unadvised, which is what happens when Zenodo is unreachable
/// anyway.
const METADATA_TIMEOUT: Duration = Duration::from_secs(10);

/// Ask which record is the newest edition of `record_id`.
///
/// `…/versions/latest` redirects to the record itself while it is current, so
/// one request answers both questions: what the newest edition is, and — when
/// that is still ours — what digest it is published with.
pub fn latest_release(record_id: u64) -> Result<Release> {
    let url = format!("https://zenodo.org/api/records/{record_id}/versions/latest");
    let body = crate::download::fetch_text(&url, METADATA_TIMEOUT)?;
    parse_release(&body).ok_or_else(|| ArunaError::Network {
        url,
        source: "Zenodo answered with something that is not a record".into(),
    })
}

/// Read the fields we care about out of a record document.
fn parse_release(body: &str) -> Option<Release> {
    let record = json::parse(body)?;
    let file = record.get("files")?.at(0)?;
    Some(Release {
        record_id: record.get("id")?.as_u64()?,
        file: file.get("key")?.as_str()?.to_string(),
        // Zenodo writes the algorithm into the value: `md5:f9acbc…`.
        md5: file
            .get("checksum")
            .and_then(|c| c.as_str())
            .and_then(|c| c.strip_prefix("md5:"))
            .map(str::to_string),
        published: record
            .get("metadata")
            .and_then(|m| m.get("publication_date"))
            .and_then(|d| d.as_str())
            .map(str::to_string),
    })
}

/// What is worth telling the reader, comparing what Zenodo publishes against
/// what this build is pinned to. `None` when the two agree — silence is the
/// right answer to no news.
///
/// A pure function so every branch can be tested; [`report`] does the printing.
///
/// Deliberately advisory. The pinned digest stays the authority — it records
/// the archive this parser was tested against, and taking Zenodo's word for it
/// instead would turn a check of *which* archive arrived into a check of
/// whether the transfer corrupted it. A republished corpus would then be
/// accepted in silence, which is the failure this pin exists to prevent.
pub fn advice(pinned_record: u64, pinned_md5: &str, latest: &Release) -> Option<String> {
    if latest.record_id != pinned_record {
        return Some(format!(
            "A newer edition of the corpus is published: Zenodo record {} ({}), file {}.\n\
             This build is pinned to record {pinned_record} and will keep using it — \
             the parser is tested against that edition.",
            latest.record_id,
            latest.published.as_deref().unwrap_or("date unknown"),
            latest.file,
        ));
    }

    let published = latest.md5.as_deref()?;
    if published.eq_ignore_ascii_case(pinned_md5) {
        return None;
    }
    // The download goes ahead regardless, and fails on its own check a minute
    // later. Aborting here would save that minute in a case that has never
    // happened — Zenodo does not modify a published record — at the price of
    // refusing to run whenever the API is wrong or this parser misreads it.
    // A false refusal is worse than a wasted minute, so the pin stays the only
    // thing that can stop a download.
    Some(format!(
        "Zenodo publishes MD5 {published} for record {pinned_record}, but this build \
         expects {pinned_md5}.\n\
         The download will be checked against the expected digest and will fail."
    ))
}

/// Print [`advice`], if there is any.
pub fn report(pinned_record: u64, pinned_md5: &str, latest: &Release) {
    if let Some(message) = advice(pinned_record, pinned_md5, latest) {
        eprintln!("{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real response, trimmed to the parts this module reads.
    const RECORD: &str = r#"{
      "created": "2026-05-21T10:00:00.000000+00:00",
      "id": 20328284,
      "conceptrecid": "15459133",
      "files": [
        {
          "id": "744d460e-1d73-48ac-ae97-268f2b01c2ae",
          "key": "TLHbasisONLINE25_1_ZENODO_Beta_03.zip",
          "size": 74449198,
          "checksum": "md5:f9acbc8db3111cc7dd88d82f7819a912",
          "links": {"self": "https://zenodo.org/api/records/20328284/files/x/content"}
        }
      ],
      "metadata": {
        "title": "Thesaurus Linguarum Hethaeorum digitalis (TLHdig) Beta Version 0.3",
        "publication_date": "2026-05-21",
        "license": {"id": "cc-by-4.0"}
      }
    }"#;

    #[test]
    fn a_record_yields_the_id_the_file_and_the_digest() {
        let release = parse_release(RECORD).expect("a record parses");
        assert_eq!(release.record_id, 20328284);
        assert_eq!(release.file, "TLHbasisONLINE25_1_ZENODO_Beta_03.zip");
        assert_eq!(
            release.md5.as_deref(),
            Some("f9acbc8db3111cc7dd88d82f7819a912"),
            "the `md5:` prefix belongs to the wire format, not to the digest"
        );
        assert_eq!(release.published.as_deref(), Some("2026-05-21"));
    }

    /// The pinned digest is what this build was tested against, so it must be
    /// what the response is compared to — not the other way round.
    #[test]
    fn the_published_digest_matches_the_one_this_build_pins() {
        let release = parse_release(RECORD).unwrap();
        assert_eq!(
            release.md5.as_deref(),
            Some(crate::download::ZENODO_ZIP_MD5),
            "the fixture and the pin describe the same archive"
        );
    }

    /// A response that is not a record must be refused rather than half-read.
    #[test]
    fn anything_that_is_not_a_record_is_refused() {
        for body in [
            "",
            "not json at all",
            "{}",
            r#"{"id": 1}"#,                     // no files
            r#"{"id": 1, "files": []}"#,        // no file in them
            r#"{"files": [{"key": "a.zip"}]}"#, // no id
            r#"{"status": 404, "message": "PID does not exist."}"#,
            // What `/versions/latest` answers when a client does not follow
            // redirects. It is JSON, and it is not a record.
            r#"{"status": 301, "message": "Redirecting...", "location": "https://zenodo.org/api/records/20328284"}"#,
            "<html><body>502 Bad Gateway</body></html>",
            "[]",
            "null",
            // A record whose id is not a number.
            r#"{"id": "20328284", "files": [{"key": "a.zip"}]}"#,
            // Files as an object rather than a list.
            r#"{"id": 1, "files": {"key": "a.zip"}}"#,
            // A file with no name.
            r#"{"id": 1, "files": [{"checksum": "md5:abc"}]}"#,
        ] {
            assert!(parse_release(body).is_none(), "accepted {body:?}");
        }
    }

    fn release(id: u64, md5: Option<&str>) -> Release {
        Release {
            record_id: id,
            file: "corpus.zip".into(),
            md5: md5.map(str::to_string),
            published: Some("2026-05-21".into()),
        }
    }

    /// Agreement is silence. This is the case every ordinary run takes, and a
    /// line printed here would be printed on every download for ever.
    #[test]
    fn nothing_is_said_when_the_published_record_is_the_pinned_one() {
        assert_eq!(advice(7, "abc", &release(7, Some("abc"))), None);
        // Digests are hex; case is not part of the value.
        assert_eq!(advice(7, "ABC", &release(7, Some("abc"))), None);
        // No digest published is not a disagreement — the download is checked
        // against the pin regardless.
        assert_eq!(advice(7, "abc", &release(7, None)), None);
    }

    /// A newer edition must be named, and the reader told which one they are
    /// getting — otherwise the message reads as a warning to act on now.
    #[test]
    fn a_newer_edition_is_announced_without_switching_to_it() {
        let message = advice(7, "abc", &release(9, Some("zzz"))).expect("news");
        assert!(message.contains("record 9"), "{message}");
        assert!(
            message.contains("2026-05-21"),
            "the edition is dated: {message}"
        );
        assert!(message.contains("corpus.zip"), "{message}");
        assert!(
            message.contains("pinned to record 7") && message.contains("keep using it"),
            "the reader must learn which edition this run uses: {message}"
        );
    }

    /// A record with no date still produces a sentence rather than a hole.
    #[test]
    fn a_newer_edition_without_a_date_still_reads() {
        let mut newer = release(9, None);
        newer.published = None;
        let message = advice(7, "abc", &newer).expect("news");
        assert!(message.contains("date unknown"), "{message}");
    }

    /// A digest that disagrees is the one case where the run is heading for a
    /// failure, and the message has to say so — the download will refuse it.
    #[test]
    fn a_disagreeing_digest_warns_and_predicts_the_failure() {
        let message = advice(7, "abc", &release(7, Some("def"))).expect("warning");
        assert!(
            message.contains("def") && message.contains("abc"),
            "both digests: {message}"
        );
        assert!(message.contains("will fail"), "{message}");
    }

    /// A newer edition outranks a digest difference: of course the digest
    /// differs, it describes a different archive. Saying both would be noise.
    #[test]
    fn a_newer_edition_is_not_also_reported_as_a_digest_mismatch() {
        let message = advice(7, "abc", &release(9, Some("def"))).expect("news");
        assert!(
            !message.contains("will fail"),
            "one story, not two: {message}"
        );
    }

    /// A record without a checksum is still usable — the digest is an extra,
    /// and the download is verified against the pin either way.
    #[test]
    fn a_record_without_a_checksum_is_still_a_record() {
        let body = r#"{"id": 7, "files": [{"key": "a.zip"}]}"#;
        let release = parse_release(body).expect("id and file are enough");
        assert_eq!(release.record_id, 7);
        assert_eq!(release.md5, None);
        assert_eq!(release.published, None);
    }
}
