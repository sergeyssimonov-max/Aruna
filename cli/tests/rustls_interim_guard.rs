//! The guard of the four advisories excepted for one week in
//! `.cargo/audit.toml` and `deny.toml`: RUSTSEC-2026-0346, -0347, -0348 and
//! -0349 in `rustls` 0.23.45, which reaches this crate through `ureq` 2.12.1.
//! RUSTSEC-2026-0381 and -0382 are excepted beside them since 2026-10-21 (task
//! «webpki-interim-ignore»), with a guard of their own, and count here among
//! the ones that were there before.
//!
//! The exception is the owner's decision of 2026-10-15 (task
//! «rustls-interim-ignore»): the fix, rustls 0.23.46, was published on
//! 2026-10-14 09:54:15 UTC and may be taken from 2026-10-21 09:54 UTC
//! (specification 5.1). It stands on a proof, written out with files and lines
//! in `.cargo/audit.toml`, that none of the four applies to the configuration
//! ureq builds: no custom session store, no ECH, no `brotli` feature of rustls,
//! no post-quantum group in the ring provider. These tests fail when:
//!
//! 1. one of the four is missing from either file, or any other advisory joins
//!    the ones that were there before;
//! 2. while a rustls below 0.23.46 is in the lock, an assumption of the proof
//!    stops holding – a manifest names brotli or aws-lc, the rustls entry of
//!    Cargo.lock depends on brotli, aws-lc-rs or aws-lc-sys enters the lock, or
//!    a source names rustls, ECH, a session store or a TLS configuration;
//! 3. rustls in Cargo.lock is at 0.23.46 or later while the four still stand;
//! 4. the date is past 2026-10-22, in UTC – the exception holds through that
//!    day and not a day longer.
//!
//! Test 4 reads the system clock, in UTC: the only test of the workspace that
//! does. The task «rustls-0.23.46» removes the four entries and this file
//! together; once they are gone, test 1 here would fail, which is the point.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The four advisories excepted for the week.
const INTERIM: [&str; 4] = [
    "RUSTSEC-2026-0346",
    "RUSTSEC-2026-0347",
    "RUSTSEC-2026-0348",
    "RUSTSEC-2026-0349",
];

/// What `.cargo/audit.toml` excepted before the week.
const AUDIT_BEFORE: [&str; 8] = [
    "RUSTSEC-2026-0194",
    "RUSTSEC-2026-0195",
    "RUSTSEC-2025-0141",
    "RUSTSEC-2024-0320",
    "RUSTSEC-2026-0206",
    "RUSTSEC-2026-0192",
    // excepted beside, 2026-10-21, task webpki-interim-ignore
    "RUSTSEC-2026-0381",
    "RUSTSEC-2026-0382",
];

/// What `deny.toml` excepted before the week.
const DENY_BEFORE: [&str; 4] = [
    "RUSTSEC-2026-0194",
    "RUSTSEC-2026-0195",
    // excepted beside, 2026-10-21, task webpki-interim-ignore
    "RUSTSEC-2026-0381",
    "RUSTSEC-2026-0382",
];

/// The first rustls that fixes all four.
const FIXED: (u64, u64, u64) = (0, 23, 46);

/// The last day, in UTC, on which the exception may stand.
const LAST_DAY: (i64, u32, u32) = (2026, 10, 22);

/// Words no manifest may carry while the exception stands. ureq's own
/// `brotli` feature decodes response bodies and is not rustls's, but the check
/// is deliberately the word: any brotli in a manifest is a question to answer
/// before the exception may go on.
const FORBIDDEN_IN_MANIFEST: [&str; 3] = ["brotli", "aws-lc", "aws_lc"];

/// Words no source may carry: the crate itself, ECH, a session store and the
/// ways to hand ureq a TLS configuration of one's own.
const FORBIDDEN_IN_SOURCE: [&str; 8] = [
    "rustls",
    "EchMode",
    "ech_mode",
    "ClientSessionStore",
    "SessionValue",
    "tls_config",
    "tls_connector",
    "with_custom",
];

fn workspace_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("cli/ sits in the workspace")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every `.rs` file under `dir`, build output and generated code aside.
fn sources_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).expect("a readable source directory");
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !matches!(name, "target" | "fixtures" | "gen" | "node_modules") {
                sources_under(&path, out);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
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

/// What is wrong with the ignore list of `file`, or `None`.
fn ignore_list_problem(file: &str, config: &str, before: &[&str]) -> Option<String> {
    let ids = ignored_ids(config);
    let expected: Vec<&str> = before.iter().chain(INTERIM.iter()).copied().collect();
    let missing: Vec<&str> = expected
        .iter()
        .filter(|e| !ids.iter().any(|i| i == *e))
        .copied()
        .collect();
    let extra: Vec<&String> = ids
        .iter()
        .filter(|i| !expected.contains(&i.as_str()))
        .collect();
    if missing.is_empty() && extra.is_empty() {
        return None;
    }
    Some(format!(
        "{file}: the ignore list should be the {} excepted before and the four of \
         rustls; missing {missing:?}, not expected {extra:?} – correct {file}",
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

/// The versions of rustls Cargo.lock resolves.
fn rustls_versions(lock: &str) -> Vec<(u64, u64, u64)> {
    packages(lock)
        .iter()
        .filter(|p| p.name == "rustls")
        .map(|p| triple(&p.version))
        .collect()
}

/// The assumptions of the proof that no longer hold; empty while it stands.
/// `manifests` and `sources` are (path, text) pairs.
fn broken_assumptions(
    lock: &str,
    manifests: &[(String, String)],
    sources: &[(String, String)],
) -> Vec<String> {
    let mut broken = Vec::new();
    for p in packages(lock) {
        if p.name == "rustls" {
            for d in &p.dependencies {
                if d.starts_with("brotli") {
                    broken.push(format!(
                        "Cargo.lock: rustls {} depends on {d}, so its brotli feature is on \
                         and RUSTSEC-2026-0348 applies – remove the feature from the manifest \
                         that asks for it",
                        p.version
                    ));
                }
            }
        }
        if p.name == "aws-lc-rs" || p.name == "aws-lc-sys" {
            broken.push(format!(
                "Cargo.lock: {} {} is in the tree, and with it a post-quantum provider \
                 RUSTSEC-2026-0349 applies to – remove the dependency that brings it",
                p.name, p.version
            ));
        }
    }
    for (path, text) in manifests {
        for word in FORBIDDEN_IN_MANIFEST {
            if text.contains(word) {
                broken.push(format!(
                    "{path} names {word:?}; the exception of the four rustls advisories \
                     assumes no manifest does – correct {path}"
                ));
            }
        }
    }
    for (path, text) in sources {
        for word in FORBIDDEN_IN_SOURCE {
            if text.contains(word) {
                broken.push(format!(
                    "{path} names {word}; the exception of the four rustls advisories \
                     assumes ureq's own TLS configuration – correct {path} or redo the \
                     proof in .cargo/audit.toml"
                ));
            }
        }
    }
    broken
}

/// Why the exception has to go now, or `None` while it may stand.
fn expired(today: (i64, u32, u32), rustls: &[(u64, u64, u64)]) -> Option<String> {
    if today > LAST_DAY {
        let (y, m, d) = today;
        let (ly, lm, ld) = LAST_DAY;
        return Some(format!(
            "the four rustls advisories were excepted through {ly:04}-{lm:02}-{ld:02}; \
             today is {y:04}-{m:02}-{d:02} UTC – снять исключения: remove them from \
             .cargo/audit.toml and deny.toml, and this file"
        ));
    }
    if rustls.is_empty() {
        return Some(
            "Cargo.lock resolves no rustls, the exception excepts nothing – снять \
             исключения: remove the four from .cargo/audit.toml and deny.toml, and this file"
                .to_string(),
        );
    }
    if rustls.iter().all(|v| *v >= FIXED) {
        let found: Vec<String> = rustls
            .iter()
            .map(|(a, b, c)| format!("{a}.{b}.{c}"))
            .collect();
        let (fa, fb, fc) = FIXED;
        return Some(format!(
            "rustls in Cargo.lock is {} – at or past {fa}.{fb}.{fc}, which fixes all \
             four: снять исключения: remove them from .cargo/audit.toml and deny.toml, \
             and this file",
            found.join(", ")
        ));
    }
    None
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

fn workspace_sources() -> Vec<(String, String)> {
    let root = workspace_dir();
    let this = root.join("cli/tests/rustls_interim_guard.rs");
    let mut paths = Vec::new();
    sources_under(&root.join("cli"), &mut paths);
    sources_under(&root.join("src-tauri"), &mut paths);
    assert!(paths.contains(&this), "the scan reaches this file");
    assert!(
        paths.contains(&root.join("cli/src/download.rs")),
        "the scan reaches the code that builds the agents"
    );
    paths
        .into_iter()
        .filter(|p| *p != this)
        .map(|p| {
            let text = read(&p);
            let name = p.strip_prefix(&root).unwrap_or(&p).display().to_string();
            (name, text)
        })
        .collect()
}

#[test]
fn the_four_are_excepted_in_both_files_and_nothing_else_is_new() {
    let root = workspace_dir();
    let audit = read(&root.join(".cargo/audit.toml"));
    let deny = read(&root.join("deny.toml"));
    let problems: Vec<String> = [
        ignore_list_problem(".cargo/audit.toml", &audit, &AUDIT_BEFORE),
        ignore_list_problem("deny.toml", &deny, &DENY_BEFORE),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_proof_of_inapplicability_still_holds() {
    let lock = read(&workspace_dir().join("Cargo.lock"));
    let rustls = rustls_versions(&lock);
    if rustls.iter().all(|v| *v >= FIXED) {
        // Nothing left to prove; the next test says to remove the exception.
        return;
    }
    let broken = broken_assumptions(&lock, &workspace_manifests(), &workspace_sources());
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}

#[test]
fn the_exception_has_not_outlived_its_reason() {
    let lock = read(&workspace_dir().join("Cargo.lock"));
    if let Some(why) = expired(today_utc(), &rustls_versions(&lock)) {
        panic!("{why}");
    }
}

#[test]
fn each_check_fails_on_made_up_inputs() {
    // The calendar, on known points.
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(1_700_000_000 / 86_400), (2023, 11, 14));
    assert_eq!(civil_from_days(20_748), (2026, 10, 22));

    // The date and the version.
    let old = [(0, 23, 45)];
    assert_eq!(expired((2026, 10, 22), &old), None);
    assert!(expired((2026, 10, 23), &old).is_some_and(|w| w.contains("снять исключения")));
    assert!(expired((2027, 1, 1), &old).is_some());
    assert!(expired((2026, 10, 15), &[(0, 23, 46)]).is_some_and(|w| w.contains("0.23.46")));
    assert!(expired((2026, 10, 15), &[(0, 24, 0)]).is_some());
    assert_eq!(expired((2026, 10, 15), &[(0, 23, 45), (0, 23, 46)]), None);
    assert!(expired((2026, 10, 15), &[]).is_some_and(|w| w.contains("no rustls")));

    // The lists.
    let both = "x\nignore = [\n    \"RUSTSEC-2026-0194\",\n    \"RUSTSEC-2026-0195\",\n    \
                # RUSTSEC-2099-0001 in a comment is no entry\n    \"RUSTSEC-2026-0346\",\n    \
                \"RUSTSEC-2026-0347\",\n    \"RUSTSEC-2026-0348\",\n    \"RUSTSEC-2026-0349\",\n    \
                \"RUSTSEC-2026-0381\",\n    \"RUSTSEC-2026-0382\",\n]\n";
    assert_eq!(ignore_list_problem("deny.toml", both, &DENY_BEFORE), None);
    let short = both.replace("    \"RUSTSEC-2026-0348\",\n", "");
    assert!(ignore_list_problem("deny.toml", &short, &DENY_BEFORE)
        .is_some_and(|w| w.contains("RUSTSEC-2026-0348") && w.contains("correct deny.toml")));
    let more = both.replace("]\n", "    \"RUSTSEC-2026-0400\",\n]\n");
    assert!(ignore_list_problem("deny.toml", &more, &DENY_BEFORE)
        .is_some_and(|w| w.contains("RUSTSEC-2026-0400")));

    // The assumptions.
    let lock = "[[package]]\nname = \"rustls\"\nversion = \"0.23.45\"\ndependencies = [\n \"log\",\n \"ring\",\n]\n";
    let with_brotli = lock.replace(" \"ring\",\n", " \"brotli\",\n \"ring\",\n");
    let with_aws = format!("{lock}\n[[package]]\nname = \"aws-lc-sys\"\nversion = \"0.30.0\"\n");
    let clean = vec![("cli/Cargo.toml".to_string(), "ureq = \"2\"".to_string())];
    let source = vec![("cli/src/x.rs".to_string(), "fn f() {}".to_string())];
    assert!(broken_assumptions(lock, &clean, &source).is_empty());
    assert!(!broken_assumptions(&with_brotli, &clean, &source).is_empty());
    assert!(!broken_assumptions(&with_aws, &clean, &source).is_empty());
    let manifest = vec![(
        "cli/Cargo.toml".to_string(),
        "[features]\nbrotli = []".to_string(),
    )];
    assert!(broken_assumptions(lock, &manifest, &source)
        .iter()
        .any(|w| w.contains("correct cli/Cargo.toml")));
    let ech = vec![("cli/src/x.rs".to_string(), "EchMode::Enable".to_string())];
    assert!(!broken_assumptions(lock, &clean, &ech).is_empty());
}
