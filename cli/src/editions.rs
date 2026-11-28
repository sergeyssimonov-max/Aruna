//! Select and cite an edition of the TLHdig collection. A record id identifies
//! one immutable deposit; the collection DOI and Zenodo's version index identify its successors.
use crate::error::{ArunaError, Result};
use crate::job::{Job, Phase};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub(crate) const COLLECTION: &str = "15459133";
const LATEST: &str = "https://zenodo.org/api/records/15459133/versions/latest";
const BETA03: &str = include_str!("../resources/editions/20328284.json");
const BETA04: &str = include_str!("../resources/editions/24373727.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EditionIssue {
    #[error("unrecognised TLHdig record, archive or terms of use")]
    Record,
    #[error("unrecognised XML in a manuscript group")]
    Markup,
    #[error("conflicting descriptions of the same edition")]
    Conflict,
    #[error("the published edition precedes the one already verified")]
    Older,
    #[error("the installed edition cannot be identified from its manifest")]
    Installed,
    #[error("no verified archive of this edition is available offline")]
    Offline,
}

impl EditionIssue {
    pub fn russian(self) -> &'static str {
        match self {
            Self::Markup => "В группе рукописей обнаружен XML неизвестного формата. Новое издание требует проверки.",
            Self::Record => "Не удалось подтвердить издание TLHdig, состав архива или условия его использования.",
            Self::Conflict => "Сведения об одном и том же издании TLHdig расходятся.",
            Self::Older => "Запись на Zenodo относится к более раннему изданию корпуса.",
            Self::Installed => "По описи и сопроводительным сведениям не удалось установить издание библиотеки.",
            Self::Offline => "Без связи с Zenodo для сборки нужна сохраненная копия архива этого издания.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Edition {
    record: u64,
    index: u64,
    title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    date: String,
    creators: Vec<Creator>,
    file: String,
    pub(crate) md5: String,
    bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Creator {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    affiliation: Option<String>,
}

fn date_ok(date: &str) -> bool {
    let b = date.as_bytes();
    if b.len() != 10
        || !b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
    {
        return false;
    }
    let (Ok(y), Ok(m), Ok(d)) = (
        date[..4].parse::<u32>(),
        date[5..7].parse::<u8>(),
        date[8..].parse::<u8>(),
    ) else {
        return false;
    };
    let days = match m {
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    (1..=days).contains(&d)
}

fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value.get(name)?.as_str()
}

fn text_ok(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 512 && !s.chars().any(char::is_control)
}

impl Edition {
    fn parse(body: &str) -> Option<Self> {
        let v: Value = serde_json::from_str(body).ok()?;
        Self::from_record(&v)
    }

    fn from_record(v: &Value) -> Option<Self> {
        let record = v["id"].as_u64()?;
        let m = &v["metadata"];
        let relation = m["relations"]["version"].as_array()?;
        let [version] = relation.as_slice() else {
            return None;
        };
        if field(v, "conceptrecid")? != COLLECTION
            || field(v, "status")? != "published"
            || field(v, "doi")? != format!("10.5281/zenodo.{record}")
            || field(&version["parent"], "pid_type")? != "recid"
            || field(&version["parent"], "pid_value")? != COLLECTION
            || field(m, "access_right")? != "open"
            || field(&m["license"], "id")? != "cc-by-4.0"
        {
            return None;
        }
        let mut archive = None;
        for f in v["files"].as_array()? {
            let name = field(f, "key")?;
            if name.to_ascii_lowercase().ends_with(".zip") && archive.replace(f).is_some() {
                return None;
            }
        }
        let file = archive?;
        let edition = Self {
            record,
            index: version["index"].as_u64()?,
            title: field(m, "title")?.into(),
            version: field(m, "version")
                .filter(|s| text_ok(s))
                .map(str::to_owned),
            date: field(m, "publication_date")?.into(),
            creators: m["creators"]
                .as_array()?
                .iter()
                .map(|c| {
                    Some(Creator {
                        name: field(c, "name")?.to_owned(),
                        affiliation: field(c, "affiliation").map(str::to_owned),
                    })
                })
                .collect::<Option<Vec<_>>>()?,
            file: field(file, "key")?.into(),
            bytes: file["size"].as_u64()?,
            md5: field(file, "checksum")?
                .strip_prefix("md5:")?
                .to_ascii_lowercase(),
        };
        edition.valid().then_some(edition)
    }

    fn valid(&self) -> bool {
        self.record > 0
            && self.index > 0
            && self.version.as_deref().is_none_or(text_ok)
            && text_ok(&self.title)
            && date_ok(&self.date)
            && !self.creators.is_empty()
            && self.creators.len() <= 128
            && self
                .creators
                .iter()
                .all(|c| text_ok(&c.name) && c.affiliation.as_ref().is_none_or(|s| text_ok(s)))
            && self.file.len() <= 255
            && text_ok(&self.file)
            && !self.file.starts_with('.')
            && !self.file.contains(['/', '\\'])
            && self.file.to_ascii_lowercase().ends_with(".zip")
            && self.md5.len() == 32
            && self.md5.bytes().all(|c| c.is_ascii_hexdigit())
            && (1..=crate::download::MAX_DOWNLOAD).contains(&self.bytes)
    }

    fn url(&self) -> String {
        format!(
            "https://zenodo.org/records/{}/files/{}?download=1",
            self.record,
            crate::export::naming::encode_uri_component(&self.file)
        )
    }

    fn short_title(&self) -> &str {
        self.title
            .strip_prefix("Thesaurus Linguarum Hethaeorum digitalis (TLHdig) ")
            .unwrap_or(&self.title)
    }

    pub(crate) fn package_name(&self) -> String {
        match self.record {
            20328284 => crate::export::LEGACY_PACKAGE.into(),
            24373727 => BETA04_PACKAGE.into(),
            // A publisher can reuse a title. The record keeps later editions distinct.
            record => {
                let title = self
                    .version
                    .as_deref()
                    .unwrap_or_else(|| self.short_title());
                let version = if let Some(v) = title
                    .strip_prefix("Beta Version ")
                    .filter(|v| numeric_version(v))
                {
                    format!("Beta_{v}")
                } else if let Some(v) = title
                    .strip_prefix("Version ")
                    .filter(|v| numeric_version(v))
                {
                    format!("Version_{v}")
                } else if let Some(label) = version_label(title, self.version.is_some()) {
                    format!("Label_{label}")
                } else {
                    format!("Edition_{}", self.index)
                };
                format!("TLHdig_{version}_Zenodo_{record}")
            }
        }
    }

    pub(crate) fn folder_rank(&self, name: &str) -> Option<u8> {
        if name == self.package_name() {
            Some(2)
        } else if name == format!("TLHdig_Edition_{}_Zenodo_{}", self.index, self.record) {
            Some(1)
        } else {
            None
        }
    }

    pub(crate) fn inventory_source(&self) -> String {
        format!("{} ({})", self.title, self.date)
    }

    pub(crate) fn citation(&self) -> String {
        format!(
            "{} ({}). DOI 10.5281/zenodo.{}. CC BY 4.0",
            self.title, self.date, self.record
        )
    }

    pub(crate) fn authors(&self) -> String {
        self.creators
            .iter()
            .map(|c| {
                let name = c.name.split_once(", ").map_or_else(
                    || c.name.clone(),
                    |(family, given)| format!("{given} {family}"),
                );
                // The inventory has always named these three cities. Only attribute a city
                // when the published affiliation itself is recognised, including for a new creator.
                let city = match c.affiliation.as_deref() {
                    Some("University of Würzburg") => Some("Würzburg"),
                    Some("Johannes Gutenberg University Mainz") => Some("Mainz"),
                    Some("Philipps University of Marburg") => Some("Marburg"),
                    _ => None,
                };
                city.map_or(name.clone(), |place| format!("{name} ({place})"))
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub(crate) fn caption(&self) -> String {
        format!(
            "Издание корпуса – {} ({}).",
            self.short_title().replace("Beta Version ", "Beta "),
            self.date
        )
    }

    pub(crate) fn follows(&self, old: &Self) -> Result<()> {
        if self.index < old.index {
            return Err(EditionIssue::Older.into());
        }
        if self.record == old.record || self.index == old.index {
            // Zenodo allows bibliographic corrections to an existing deposit.
            if self.record != old.record
                || self.index != old.index
                || self.md5 != old.md5
                || self.file != old.file
                || self.bytes != old.bytes
            {
                return Err(EditionIssue::Conflict.into());
            }
        }
        Ok(())
    }

    pub(crate) fn verify_bytes(&self, path: &Path, hash: &str, bytes: u64) -> Result<()> {
        if self.md5 != hash {
            return Err(ArunaError::ChecksumMismatch {
                url: path.display().to_string(),
                expected: self.md5.clone(),
                got: hash.into(),
            });
        }
        if self.bytes != bytes {
            return Err(EditionIssue::Conflict.into());
        }
        Ok(())
    }
}

fn reference() -> Result<Edition> {
    Edition::parse(BETA04).ok_or(EditionIssue::Record.into())
}

pub(crate) fn known_digest(hash: &str) -> Option<Edition> {
    [BETA04, BETA03]
        .into_iter()
        .filter_map(Edition::parse)
        .find(|e| e.md5 == hash)
}

fn latest(job: &Job<'_>) -> Result<Edition> {
    latest_with(job, || {
        crate::download::fetch_text(LATEST, std::time::Duration::from_secs(10))
    })
}

// Only failures of the metadata question permit retry/offline selection.
// A successfully parsed conflicting or older edition remains a hard refusal.
fn metadata_unavailable(error: &ArunaError) -> bool {
    matches!(
        error,
        ArunaError::Network { .. }
            | ArunaError::Http { .. }
            | ArunaError::Oversized { .. }
            | ArunaError::Edition(EditionIssue::Record)
    )
}

fn latest_with(job: &Job<'_>, mut fetch: impl FnMut() -> Result<String>) -> Result<Edition> {
    job.check(Phase::Obtaining)?;
    let first = fetch().and_then(|body| current_record(&body));
    job.check(Phase::Obtaining)?;
    match first {
        Err(ref error) if metadata_unavailable(error) => {
            let second = fetch().and_then(|body| current_record(&body));
            job.check(Phase::Obtaining)?;
            second
        }
        other => other,
    }
}

fn current_record(body: &str) -> Result<Edition> {
    let value: Value = serde_json::from_str(body).map_err(|_| EditionIssue::Record)?;
    if value["metadata"]["relations"]["version"][0]["is_last"].as_bool() != Some(true) {
        return Err(EditionIssue::Record.into());
    }
    let edition = Edition::from_record(&value).ok_or(EditionIssue::Record)?;
    edition.follows(&reference()?)?;
    Ok(edition)
}

#[derive(Deserialize)]
struct Manifest {
    source: Option<Source>,
}
#[derive(Deserialize)]
struct Source {
    archive_md5: Option<String>,
    zenodo: Option<Edition>,
}

/// Existing packages without an edition field can still be identified by their actual archive digest.
pub(crate) fn installed(root: &Path) -> Result<Option<Edition>> {
    installed_named(
        root,
        root.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default(),
    )
}

// A saved package is verified before its original directory name is restored.
pub(crate) fn installed_named(root: &Path, package_name: &str) -> Result<Option<Edition>> {
    match fs::symlink_metadata(root) {
        Ok(m) if !m.is_dir() => return Err(EditionIssue::Installed.into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(ArunaError::io(root)(e)),
        _ => {}
    }
    let path = root.join(crate::export::MANIFEST);
    let meta = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(ArunaError::io(&path)(e)),
    };
    if !meta.is_file() || meta.len() > crate::export::validate::MAX_MANIFEST {
        return Err(EditionIssue::Installed.into());
    }
    let f = fs::File::open(&path).map_err(ArunaError::io(&path))?;
    let reader = std::io::BufReader::new(f.take(crate::export::validate::MAX_MANIFEST + 1));
    let Manifest { source } =
        serde_json::from_reader(reader).map_err(|_| EditionIssue::Installed)?;
    let Some(Source {
        archive_md5: Some(archive_md5),
        zenodo,
    }) = source
    else {
        // Early libraries can be read without source metadata. The neutral
        // destination is a sibling, so a manual upgrade can leave them intact.
        return if package_name == crate::export::LEGACY_PACKAGE {
            Ok(None)
        } else {
            Err(EditionIssue::Installed.into())
        };
    };
    match zenodo {
        Some(e) if e.valid() && e.md5 == archive_md5 => Ok(Some(e)),
        Some(_) => Err(EditionIssue::Installed.into()),
        None => Ok(known_digest(&archive_md5)),
    }
}

const RECEIPTS: &str = "tlhdig-editions";
const MAX_RECEIPT: u64 = 128 * 1024;

/// One receipt per successfully published edition. Separate immutable names
/// avoid an older concurrent build replacing a newer cache pointer.
pub(crate) fn remember(dir: &Path, edition: &Edition) {
    let folder = dir.join(RECEIPTS);
    if fs::create_dir(&folder).is_err() && !fs::symlink_metadata(&folder).is_ok_and(|m| m.is_dir())
    {
        return;
    }
    if let Ok(bytes) = serde_json::to_vec(edition) {
        let path = folder.join(format!("{}.json", edition.record));
        let _ = crate::paths::write_atomic(&path, &bytes);
    }
}

fn offline_editions(cache: Option<&Path>) -> Vec<Edition> {
    let mut editions: Vec<_> = [BETA04, BETA03]
        .into_iter()
        .filter_map(Edition::parse)
        .collect();
    if let Some(dir) = cache {
        let folder = dir.join(RECEIPTS);
        if fs::symlink_metadata(&folder).is_ok_and(|m| m.is_dir()) {
            if let Ok(entries) = fs::read_dir(folder) {
                editions.extend(entries.flatten().filter_map(|entry| {
                    let name = entry.file_name();
                    let record: u64 = name.to_str()?.strip_suffix(".json")?.parse().ok()?;
                    let metadata = fs::symlink_metadata(entry.path()).ok()?;
                    if !metadata.is_file() || metadata.len() > MAX_RECEIPT {
                        return None;
                    }
                    let file = fs::File::open(entry.path()).ok()?;
                    let edition: Edition =
                        serde_json::from_reader(file.take(MAX_RECEIPT + 1)).ok()?;
                    (edition.record == record && edition.valid()).then_some(edition)
                }));
            }
        }
    }
    editions.sort_by_key(|e| std::cmp::Reverse(e.index));
    editions.dedup_by_key(|e| e.record);
    editions
}

fn cached(dir: Option<&Path>, e: &Edition) -> Option<crate::cache::Archive> {
    match crate::cache::lookup(dir?, &e.url(), &e.md5) {
        crate::cache::Lookup::Hit(p) => Some(crate::cache::Archive::Cached(p)),
        _ => None,
    }
}

pub(crate) fn obtain(
    old: Option<&Edition>,
    job: &Job<'_>,
    selected: Option<&Edition>,
) -> Result<(crate::cache::Archive, Edition)> {
    select(
        old,
        job,
        || selected.map_or_else(|| latest(job), |e| Ok(e.clone())),
        crate::cache::cache_dir().as_deref(),
        |e, job| crate::cached_download(&e.url(), &e.md5, job, None),
    )
}

fn select(
    old: Option<&Edition>,
    job: &Job<'_>,
    lookup: impl FnOnce() -> Result<Edition>,
    cache: Option<&Path>,
    download: impl FnOnce(&Edition, &Job<'_>) -> Result<crate::cache::Archive>,
) -> Result<(crate::cache::Archive, Edition)> {
    job.check(Phase::Obtaining)?;
    let result = lookup();
    job.check(Phase::Obtaining)?;
    match result {
        Ok(e) => {
            if let Some(old) = old {
                e.follows(old)?;
            }
            job.progress()
                .report_zenodo_notice(&crate::zenodo::Notice::Selected {
                    title: e.short_title(),
                    published: &e.date,
                });
            let a = download(&e, job)?;
            Ok((a, e))
        }
        Err(e) if metadata_unavailable(&e) => {
            let editions = match old {
                Some(e) => vec![e.clone()],
                None => offline_editions(cache),
            };
            for e in editions {
                if let Some(a) = cached(cache, &e) {
                    job.progress()
                        .report_zenodo_notice(&crate::zenodo::Notice::Offline {
                            title: e.short_title(),
                            published: &e.date,
                        });
                    job.report(crate::progress::Event::ArchiveFromCache { path: a.path() });
                    return Ok((a, e));
                }
            }
            Err(EditionIssue::Offline.into())
        }
        Err(e) => Err(e),
    }
}

pub(crate) fn package_name(edition: Option<&Edition>) -> String {
    edition.map_or_else(|| crate::export::PACKAGE.to_owned(), Edition::package_name)
}

pub(crate) fn edition_inventory(edition: Option<&Edition>) -> String {
    format!("{}.html", package_name(edition))
}

const BETA04_PACKAGE: &str = "TLHdig_Beta_0.4";

fn numeric_version(s: &str) -> bool {
    s.len() <= 40
        && s.split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

// An explicit edition label need not contain digits. A free-form publication
// title only serves as a version label when it includes a number.
fn version_label(title: &str, explicit: bool) -> Option<String> {
    let label = title.split_whitespace().collect::<Vec<_>>().join("_");
    (valid_label(&label) && (explicit || label.bytes().any(|c| c.is_ascii_digit())))
        .then_some(label)
}

fn valid_label(label: &str) -> bool {
    (1..=64).contains(&label.len())
        && label.starts_with(char::is_alphanumeric)
        && label.ends_with(char::is_alphanumeric)
        && !label.contains("..")
        && label
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Restrict reader commands to the same single-component names the exporter emits.
pub fn is_package_name(name: &str) -> bool {
    if matches!(
        name,
        crate::export::PACKAGE | crate::export::LEGACY_PACKAGE | BETA04_PACKAGE
    ) {
        return true;
    }
    let Some((prefix, record)) = name.rsplit_once("_Zenodo_") else {
        return false;
    };
    record
        .parse::<u64>()
        .is_ok_and(|id| id > 0 && id.to_string() == record)
        && (["TLHdig_Beta_", "TLHdig_Version_"]
            .iter()
            .any(|lead| prefix.strip_prefix(lead).is_some_and(numeric_version))
            || prefix
                .strip_prefix("TLHdig_Label_")
                .is_some_and(valid_label)
            || prefix
                .strip_prefix("TLHdig_Edition_")
                .is_some_and(|n| n.parse::<u64>().is_ok_and(|i| i > 0 && i.to_string() == n)))
}

pub(crate) fn inventory_name(root: &Path) -> String {
    let stem = root
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| is_package_name(s))
        .unwrap_or(crate::export::PACKAGE);
    format!("{stem}.html")
}

/// Prefer the latest verified edition; older layouts remain readable in place.
pub fn library_paths(parent: &Path) -> Result<(PathBuf, PathBuf)> {
    let current = parent.join(crate::export::PACKAGE);
    let legacy = parent.join(crate::export::LEGACY_PACKAGE);
    let fallback = if !current.join(crate::paths::OUTPUT_FILE_NAME).is_file()
        && legacy.join(crate::paths::LEGACY_INVENTORY).is_file()
    {
        legacy
    } else {
        current
    };
    let mut best = fallback;
    let mut rank = installed(&best)?.map_or((0, 0, 0), |old| (old.index, 0, old.record));
    let entries = match fs::read_dir(parent) {
        Ok(entries) => Some(entries),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(ArunaError::io(parent)(e)),
    };
    if let Some(entries) = entries {
        for entry in entries {
            let entry = entry.map_err(ArunaError::io(parent))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if matches!(name, crate::export::PACKAGE | crate::export::LEGACY_PACKAGE)
                || !is_package_name(name)
                || !entry.file_type().is_ok_and(|t| t.is_dir())
            {
                continue;
            }
            let path = entry.path();
            if !fs::symlink_metadata(path.join(inventory_name(&path))).is_ok_and(|m| m.is_file()) {
                continue;
            }
            if let Some(e) = installed(&path)? {
                // Prefer the current spelling over an earlier index-based name for the same edition.
                let Some(spelling) = e.folder_rank(name) else {
                    continue;
                };
                let key = (e.index, spelling, e.record);
                if key > rank {
                    rank = key;
                    best = path;
                }
            }
        }
    }
    let inventory = best.join(inventory_name(&best));
    Ok((best, inventory))
}

/// Only the core can create an update: its verified record and destination stay together
/// between the desktop's background check and the cancellable build.
#[derive(Debug)]
pub struct Update {
    root: PathBuf,
    previous: Edition,
    edition: Edition,
}

pub fn check_update(root: &Path) -> Result<Option<Update>> {
    let Some(old) = installed(root)? else {
        return Ok(None);
    };
    let new = latest(&Job::unattended())?;
    new.follows(&old)?;
    Ok((new.index > old.index).then(|| Update {
        root: root.to_owned(),
        previous: old,
        edition: new,
    }))
}

impl Update {
    pub fn build(
        &self,
        request: &crate::app::CorpusRequest,
        job: &Job<'_>,
    ) -> Result<crate::app::CorpusReport> {
        let destination = self.root.parent().ok_or(EditionIssue::Installed)?;
        if library_paths(destination)?.0 != self.root
            || installed(&self.root)?.as_ref() != Some(&self.previous)
        {
            return Err(EditionIssue::Conflict.into());
        }
        crate::app::build_selected(request, destination, job, Some(&self.edition))
    }
}

pub fn installed_caption(root: &Path) -> Result<Option<String>> {
    Ok(installed(root)?.map(|e| e.caption()))
}

#[cfg(test)]
mod tests;
