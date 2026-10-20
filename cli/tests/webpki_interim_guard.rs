//! The guard of the two advisories excepted until their fixes mature, in
//! `.cargo/audit.toml` and `deny.toml`: RUSTSEC-2026-0381 in `webpki-roots`
//! 1.0.9 (and the 0.26.11 shim over it) and RUSTSEC-2026-0382 in the webpki of
//! the TLS crate ureq 2.12.1 brings, 0.103.15.
//!
//! The exception is the owner's decision of 2026-10-21 (task
//! «webpki-interim-ignore»), the second departure from the rule that a task
//! brings no new exception. The fixes are webpki-roots 1.0.10, published
//! 2026-10-19 20:11 UTC and mature from 2026-10-26 20:11 UTC, and webpki
//! 0.103.16, published 2026-10-20 08:58 UTC and mature from 2026-10-27
//! 08:58 UTC (specification 5.1). Both advisories are about trusting a
//! certificate path through distrusted emSign intermediates. The harm is
//! bounded, and the proof is in `.cargo/audit.toml`: the only TLS this crate
//! speaks is ureq's, to zenodo.org; the archive is held to ZENODO_ZIP_MD5 and
//! refused on a mismatch, and the record's API answer is advisory. These tests
//! fail when:
//!
//! 1. one of the two is missing from either file, or any advisory joins that is
//!    neither one of them, nor one excepted before, nor – while the guard of the
//!    four TLS advisories beside this file stands – one of those four;
//! 2. while the fix of either is not in the lock, an assumption of the proof
//!    stops holding – the archive digest or its refusal leaves
//!    cli/src/download.rs, a manifest names another HTTP or TLS stack, the
//!    webpki crate or webpki-roots gains another dependant in Cargo.lock, or
//!    the sources stop naming zenodo.org;
//! 3. either crate is at its fixed version everywhere in the lock while its
//!    exception still stands;
//! 4. the date is past 2026-10-28, in UTC.
//!
//! The name of the TLS crate is put together from two halves below: the guard
//! of the four TLS advisories refuses that name in any source but its own.
//! The task «tls-trust» removes the two entries and this file together.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The two advisories excepted until the fixes mature.
const INTERIM: [&str; 2] = ["RUSTSEC-2026-0381", "RUSTSEC-2026-0382"];

/// What `.cargo/audit.toml` excepts for good.
const AUDIT_BEFORE: [&str; 6] = [
    "RUSTSEC-2026-0194",
    "RUSTSEC-2026-0195",
    "RUSTSEC-2025-0141",
    "RUSTSEC-2024-0320",
    "RUSTSEC-2026-0206",
    "RUSTSEC-2026-0192",
];

/// What `deny.toml` excepts for good.
const DENY_BEFORE: [&str; 2] = ["RUSTSEC-2026-0194", "RUSTSEC-2026-0195"];

/// The four TLS advisories of the week before, which may stand beside these
/// while their own guard does.
const TLS_WEEK: [&str; 4] = [
    "RUSTSEC-2026-0346",
    "RUSTSEC-2026-0347",
    "RUSTSEC-2026-0348",
    "RUSTSEC-2026-0349",
];

/// The TLS crate, and its webpki, by name.
const TLS: &str = concat!("rust", "ls");
const WEBPKI: &str = concat!("rust", "ls-webpki");
const ROOTS: &str = "webpki-roots";

/// The guard of the four TLS advisories, whose presence lets them stand.
const TLS_WEEK_GUARD: &str = concat!("cli/tests/", "rust", "ls_interim_guard.rs");

/// The first fixed versions.
const WEBPKI_FIXED: (u64, u64, u64) = (0, 103, 16);
const ROOTS_FIXED: (u64, u64, u64) = (1, 0, 10);

/// The last day, in UTC, on which the exception may stand.
const LAST_DAY: (i64, u32, u32) = (2026, 10, 28);

/// Words no manifest of the workspace may carry while the exception stands:
/// another stack that could open a TLS connection of its own.
const FORBIDDEN_IN_MANIFEST: [&str; 4] = ["reqwest", "hyper", "curl", "native-tls"];

fn workspace_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("cli/ sits in the workspace")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The advisory ids inside the `ignore = [ … ]` array of a config, comment
/// lines aside. The prose around the array names ids too, and is not the list.
fn ignored_ids(config: &str) -> Vec<String> {
    let Some(start) = config.find("\nignore = [") else {
        return Vec::new();
    };
    let rest = &config[start + 1..];
    let end = rest.find("\n]").unwrap_or(rest.len());
    let mut ids = Vec::new();
    for line in rest[..end].lines() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        let mut tail = line;
        while let Some(at) = tail.find("RUSTSEC-") {
            let id: String = tail[at..].chars().take("RUSTSEC-0000-0000".len()).collect();
            let digits = id.as_bytes();
            let well_formed = id.len() == 17
                && digits[8..12].iter().all(u8::is_ascii_digit)
                && digits[12] == b'-'
                && digits[13..].iter().all(u8::is_ascii_digit);
            if well_formed {
                ids.push(id);
            }
            tail = &tail[at + "RUSTSEC-".len()..];
        }
    }
    ids
}

/// What is wrong with the ignore list of `file`, or `None`. The four TLS
/// advisories of the week before are allowed only while `tls_week` is true.
fn ignore_list_problem(
    file: &str,
    config: &str,
    before: &[&str],
    tls_week: bool,
) -> Option<String> {
    let ids = ignored_ids(config);
    let required: Vec<&str> = before.iter().chain(INTERIM.iter()).copied().collect();
    let missing: Vec<&str> = required
        .iter()
        .filter(|e| !ids.iter().any(|i| i == *e))
        .copied()
        .collect();
    let extra: Vec<&String> = ids
        .iter()
        .filter(|i| {
            !required.contains(&i.as_str()) && !(tls_week && TLS_WEEK.contains(&i.as_str()))
        })
        .collect();
    if missing.is_empty() && extra.is_empty() {
        return None;
    }
    Some(format!(
        "{file}: the ignore list should be the {} excepted for good and the two of \
         webpki; missing {missing:?}, not expected {extra:?} – correct {file}",
        before.len()
    ))
}

/// One `[[package]]` entry of Cargo.lock.
struct Package {
    name: String,
    version: String,
    dependencies: Vec<String>,
}

fn packages(lock: &str) -> Vec<Package> {
    let mut out = Vec::new();
    for block in lock.split("[[package]]").skip(1) {
        let field = |key: &str| {
            block
                .lines()
                .find_map(|l| l.strip_prefix(key))
                .map(|v| v.trim().trim_matches('"').to_string())
                .unwrap_or_default()
        };
        let dependencies = match block.find("dependencies = [") {
            Some(at) => {
                let list = &block[at..];
                let list = &list[..list.find(']').unwrap_or(list.len())];
                list.lines()
                    .skip(1)
                    .map(|l| l.trim().trim_end_matches(',').trim_matches('"').to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            }
            None => Vec::new(),
        };
        out.push(Package {
            name: field("name = "),
            version: field("version = "),
            dependencies,
        });
    }
    out
}

fn triple(version: &str) -> (u64, u64, u64) {
    let mut parts = version
        .split(['.', '-', '+'])
        .map(|p| p.parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// The versions of `name` Cargo.lock resolves.
fn versions(lock: &str, name: &str) -> Vec<(u64, u64, u64)> {
    packages(lock)
        .iter()
        .filter(|p| p.name == name)
        .map(|p| triple(&p.version))
        .collect()
}

/// Whether every `name` in the lock is at `fixed` or later.
fn all_fixed(lock: &str, name: &str, fixed: (u64, u64, u64)) -> bool {
    let found = versions(lock, name);
    !found.is_empty() && found.iter().all(|v| *v >= fixed)
}

/// The packages of the lock that depend on `name`, as `name version`.
fn dependants(lock: &str, name: &str) -> Vec<(String, String)> {
    packages(lock)
        .into_iter()
        .filter(|p| {
            p.dependencies
                .iter()
                .any(|d| d == name || d.starts_with(&format!("{name} ")))
        })
        .map(|p| (p.name, p.version))
        .collect()
}

/// The assumptions of the proof that no longer hold; empty while it stands.
/// `manifests` are (path, text) pairs; `download` and `zenodo` are the texts of
/// cli/src/download.rs and cli/src/zenodo.rs.
fn broken_assumptions(
    lock: &str,
    manifests: &[(String, String)],
    download: &str,
    zenodo: &str,
) -> Vec<String> {
    let mut broken = Vec::new();
    if !download.contains("pub const ZENODO_ZIP_MD5") || !download.contains("ChecksumMismatch") {
        broken.push(
            "cli/src/download.rs no longer holds the archive to ZENODO_ZIP_MD5 and refuses \
             it on a mismatch; the webpki exception assumes a forged archive is refused – \
             correct cli/src/download.rs or redo the proof in .cargo/audit.toml"
                .to_string(),
        );
    }
    for (path, text) in [
        ("cli/src/download.rs", download),
        ("cli/src/zenodo.rs", zenodo),
    ] {
        if !text.contains("https://zenodo.org/") {
            broken.push(format!(
                "{path} no longer names zenodo.org; the webpki exception assumes the hosts \
                 are Zenodo's – correct {path} or redo the proof in .cargo/audit.toml"
            ));
        }
    }
    for (path, text) in manifests {
        for word in FORBIDDEN_IN_MANIFEST {
            if text.contains(word) {
                broken.push(format!(
                    "{path} names {word:?}; the webpki exception assumes ureq is the only \
                     TLS client of the workspace – correct {path}"
                ));
            }
        }
    }
    for (name, version) in dependants(lock, WEBPKI) {
        if name != TLS {
            broken.push(format!(
                "Cargo.lock: {name} {version} depends on the webpki crate; the exception \
                 assumes only ureq's TLS does – remove the dependency that brings it"
            ));
        }
    }
    for (name, version) in dependants(lock, ROOTS) {
        let shim = name == ROOTS && triple(&version).0 == 0;
        if name != "ureq" && !shim {
            broken.push(format!(
                "Cargo.lock: {name} {version} depends on webpki-roots; the exception \
                 assumes only ureq and the 0.26 shim do – remove the dependency that brings it"
            ));
        }
    }
    broken
}

/// Why the exception has to go now, or `None` while it may stand. `webpki` and
/// `roots` say whether each crate is at its fix everywhere in the lock.
fn expired(today: (i64, u32, u32), webpki: bool, roots: bool) -> Option<String> {
    if today > LAST_DAY {
        let (y, m, d) = today;
        let (ly, lm, ld) = LAST_DAY;
        return Some(format!(
            "the two webpki advisories were excepted through {ly:04}-{lm:02}-{ld:02}; \
             today is {y:04}-{m:02}-{d:02} UTC – снять исключение: remove them from \
             .cargo/audit.toml and deny.toml, and this file"
        ));
    }
    let mut why = Vec::new();
    if roots {
        let (a, b, c) = ROOTS_FIXED;
        why.push(format!(
            "every webpki-roots in Cargo.lock is at {a}.{b}.{c} or later – снять исключение \
             RUSTSEC-2026-0381"
        ));
    }
    if webpki {
        let (a, b, c) = WEBPKI_FIXED;
        why.push(format!(
            "the webpki crate in Cargo.lock is at {a}.{b}.{c} or later – снять исключение \
             RUSTSEC-2026-0382"
        ));
    }
    if why.is_empty() {
        None
    } else {
        Some(format!(
            "{}: remove it from .cargo/audit.toml and deny.toml",
            why.join("; ")
        ))
    }
}

/// The civil date of a day count since 1970-01-01 (Howard Hinnant's
/// `civil_from_days`), so that no date crate comes in for one comparison.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m as u32, d as u32)
}

fn today_utc() -> (i64, u32, u32) {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is past 1970")
        .as_secs();
    civil_from_days((seconds / 86_400) as i64)
}

fn workspace_manifests() -> Vec<(String, String)> {
    let root = workspace_dir();
    ["Cargo.toml", "cli/Cargo.toml", "src-tauri/Cargo.toml"]
        .iter()
        .map(|p| (p.to_string(), read(&root.join(p))))
        .collect()
}

#[test]
fn the_two_are_excepted_in_both_files_and_nothing_else_is_new() {
    let root = workspace_dir();
    let tls_week = root.join(TLS_WEEK_GUARD).is_file();
    let audit = read(&root.join(".cargo/audit.toml"));
    let deny = read(&root.join("deny.toml"));
    let problems: Vec<String> = [
        ignore_list_problem(".cargo/audit.toml", &audit, &AUDIT_BEFORE, tls_week),
        ignore_list_problem("deny.toml", &deny, &DENY_BEFORE, tls_week),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_proof_of_bounded_harm_still_holds() {
    let root = workspace_dir();
    let lock = read(&root.join("Cargo.lock"));
    if all_fixed(&lock, WEBPKI, WEBPKI_FIXED) && all_fixed(&lock, ROOTS, ROOTS_FIXED) {
        // Nothing left to prove; the next test says to remove the exception.
        return;
    }
    let broken = broken_assumptions(
        &lock,
        &workspace_manifests(),
        &read(&root.join("cli/src/download.rs")),
        &read(&root.join("cli/src/zenodo.rs")),
    );
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}

#[test]
fn the_exception_has_not_outlived_its_reason() {
    let lock = read(&workspace_dir().join("Cargo.lock"));
    let webpki = all_fixed(&lock, WEBPKI, WEBPKI_FIXED);
    let roots = all_fixed(&lock, ROOTS, ROOTS_FIXED);
    if let Some(why) = expired(today_utc(), webpki, roots) {
        panic!("{why}");
    }
}

#[test]
fn each_check_fails_on_made_up_inputs() {
    // The calendar, on known points.
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(1_700_000_000 / 86_400), (2023, 11, 14));
    assert_eq!(civil_from_days(20_754), (2026, 10, 28));

    // The date, and each fix on its own.
    assert_eq!(expired((2026, 10, 28), false, false), None);
    assert!(expired((2026, 10, 29), false, false).is_some_and(|w| w.contains("снять исключение")));
    assert!(expired((2027, 1, 1), false, false).is_some());
    assert!(expired((2026, 10, 21), true, false)
        .is_some_and(|w| w.contains("RUSTSEC-2026-0382") && !w.contains("RUSTSEC-2026-0381")));
    assert!(expired((2026, 10, 21), false, true)
        .is_some_and(|w| w.contains("RUSTSEC-2026-0381") && !w.contains("RUSTSEC-2026-0382")));

    // Fixed means fixed everywhere: the 0.26 shim keeps webpki-roots unfixed.
    let lock = format!(
        "[[package]]\nname = \"{WEBPKI}\"\nversion = \"0.103.15\"\n\n\
         [[package]]\nname = \"webpki-roots\"\nversion = \"0.26.11\"\ndependencies = [\n \"webpki-roots 1.0.9\",\n]\n\n\
         [[package]]\nname = \"webpki-roots\"\nversion = \"1.0.9\"\n\n\
         [[package]]\nname = \"ureq\"\nversion = \"2.12.1\"\ndependencies = [\n \"{TLS}\",\n \"webpki-roots 0.26.11\",\n \"webpki-roots 1.0.9\",\n]\n\n\
         [[package]]\nname = \"{TLS}\"\nversion = \"0.23.45\"\ndependencies = [\n \"{WEBPKI}\",\n]\n"
    );
    assert!(!all_fixed(&lock, WEBPKI, WEBPKI_FIXED));
    let webpki_fixed = lock.replace("version = \"0.103.15\"", "version = \"0.103.16\"");
    assert!(all_fixed(&webpki_fixed, WEBPKI, WEBPKI_FIXED));
    let roots_one = lock
        .replace("\"1.0.9\"", "\"1.0.10\"")
        .replace("roots 1.0.9", "roots 1.0.10");
    assert!(
        !all_fixed(&roots_one, ROOTS, ROOTS_FIXED),
        "the shim is still 0.26.11"
    );

    // The lists.
    let both = "x\nignore = [\n    \"RUSTSEC-2026-0194\",\n    \"RUSTSEC-2026-0195\",\n    \
                # RUSTSEC-2099-0001 in a comment is no entry\n    \"RUSTSEC-2026-0346\",\n    \
                \"RUSTSEC-2026-0347\",\n    \"RUSTSEC-2026-0348\",\n    \"RUSTSEC-2026-0349\",\n    \
                \"RUSTSEC-2026-0381\",\n    \"RUSTSEC-2026-0382\",\n]\n";
    assert_eq!(
        ignore_list_problem("deny.toml", both, &DENY_BEFORE, true),
        None
    );
    assert!(ignore_list_problem("deny.toml", both, &DENY_BEFORE, false)
        .is_some_and(|w| w.contains("RUSTSEC-2026-0346")));
    let short = both.replace("    \"RUSTSEC-2026-0382\",\n", "");
    assert!(ignore_list_problem("deny.toml", &short, &DENY_BEFORE, true)
        .is_some_and(|w| w.contains("RUSTSEC-2026-0382") && w.contains("correct deny.toml")));
    let more = both.replace("]\n", "    \"RUSTSEC-2026-0400\",\n]\n");
    assert!(ignore_list_problem("deny.toml", &more, &DENY_BEFORE, true)
        .is_some_and(|w| w.contains("RUSTSEC-2026-0400")));

    // The assumptions.
    let download = "pub const ZENODO_ZIP_MD5: &str = \"x\";\nErr(ArunaError::ChecksumMismatch {})\n\"https://zenodo.org/records\"";
    let zenodo = "\"https://zenodo.org/api/records\"";
    let clean = vec![("cli/Cargo.toml".to_string(), "ureq = \"2\"".to_string())];
    assert!(broken_assumptions(&lock, &clean, download, zenodo).is_empty());
    let no_digest = download.replace("ChecksumMismatch", "Other");
    assert!(!broken_assumptions(&lock, &clean, &no_digest, zenodo).is_empty());
    assert!(
        broken_assumptions(&lock, &clean, download, "\"https://example.org/\"")
            .iter()
            .any(|w| w.contains("cli/src/zenodo.rs"))
    );
    let reqwest = vec![(
        "src-tauri/Cargo.toml".to_string(),
        "reqwest = \"0.13\"".to_string(),
    )];
    assert!(broken_assumptions(&lock, &reqwest, download, zenodo)
        .iter()
        .any(|w| w.contains("correct src-tauri/Cargo.toml")));
    let another = format!("{lock}\n[[package]]\nname = \"other\"\nversion = \"1.0.0\"\ndependencies = [\n \"{WEBPKI}\",\n]\n");
    assert!(!broken_assumptions(&another, &clean, download, zenodo).is_empty());
    let roots_user = format!("{lock}\n[[package]]\nname = \"other\"\nversion = \"1.0.0\"\ndependencies = [\n \"webpki-roots 1.0.9\",\n]\n");
    assert!(!broken_assumptions(&roots_user, &clean, download, zenodo).is_empty());
}
