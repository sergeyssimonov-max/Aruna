//! The guard of TLS trust: what the certificate path of the one TLS
//! connection this workspace opens, ureq's to zenodo.org, is verified by and
//! against. Unlike the two guards before it – of RUSTSEC-2026-0346 to -0349
//! (2026-10-15 to 2026-10-21) and of RUSTSEC-2026-0381 and -0382 (2026-10-21
//! to the task «tls-trust», 2026-10-24) – it has no date: it stands as long as
//! the TLS crate does. These tests fail when:
//!
//! 1. `.cargo/audit.toml` or `deny.toml` excepts one of those six advisories,
//!    or any advisory about the TLS crate, its webpki, webpki-roots or
//!    webpki-root-certs – named in an entry, its reason or a comment inside the
//!    list, or in the prose of `.cargo/audit.toml` that explains an id, which
//!    every id there must have;
//! 2. Cargo.lock holds the TLS crate below 0.23.46, its webpki below 0.103.16,
//!    or a webpki-roots 1.x below 1.0.10, or a pre-release of any of them;
//! 3. a webpki-roots 0.26.x in the lock depends on anything but one
//!    webpki-roots 1.x – the shim has to stay a shim, with no roots of its own;
//! 4. a manifest of the workspace names reqwest, hyper, curl or native-tls:
//!    the TLS path is one, and the floors of test 2 are its floors.
//!
//! The names of the TLS crate and its webpki are put together from halves
//! below, as the guards before it did, so that no guard of names reads them
//! as a use.

use std::path::{Path, PathBuf};

/// The TLS crate, and its webpki, by name.
const TLS: &str = concat!("rust", "ls");
const WEBPKI: &str = concat!("rust", "ls-webpki");
const ROOTS: &str = "webpki-roots";
const ROOT_CERTS: &str = "webpki-root-certs";

/// The advisories about the TLS path once excepted here; none may return.
const ONCE_EXCEPTED: [&str; 6] = [
    "RUSTSEC-2026-0346",
    "RUSTSEC-2026-0347",
    "RUSTSEC-2026-0348",
    "RUSTSEC-2026-0349",
    "RUSTSEC-2026-0381",
    "RUSTSEC-2026-0382",
];

/// The floors: the first versions with every fix the advisories above ask for.
const TLS_FLOOR: (u64, u64, u64) = (0, 23, 46);
const WEBPKI_FLOOR: (u64, u64, u64) = (0, 103, 16);
const ROOTS_FLOOR: (u64, u64, u64) = (1, 0, 10);

/// Words no manifest of the workspace may carry: another stack that could open
/// a TLS connection of its own, past these floors.
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

/// The text before the `ignore = [ … ]` array of a config, and the array.
fn split_at_ignore(config: &str) -> (&str, &str) {
    let Some(start) = config.find("\nignore = [") else {
        return (config, "");
    };
    let rest = &config[start + 1..];
    let end = rest.find("\n]").map_or(rest.len(), |e| e + 2);
    (&config[..start], &rest[..end])
}

/// Every well-formed advisory id in `text`.
fn ids_in(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut tail = text;
    while let Some(at) = tail.find("RUSTSEC-") {
        let id: String = tail[at..].chars().take("RUSTSEC-0000-0000".len()).collect();
        let b = id.as_bytes();
        if id.len() == 17
            && b[8..12].iter().all(u8::is_ascii_digit)
            && b[12] == b'-'
            && b[13..].iter().all(u8::is_ascii_digit)
        {
            ids.push(id);
        }
        tail = &tail[at + "RUSTSEC-".len()..];
    }
    ids
}

/// The TLS package `text` names as a word of its own, if any: `webpki-roots`
/// inside a longer name is not it.
fn names_tls_package(text: &str) -> Option<&'static str> {
    let edge = |c: Option<char>| !c.is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_');
    [TLS, WEBPKI, ROOTS, ROOT_CERTS].into_iter().find(|name| {
        text.match_indices(name).any(|(at, _)| {
            edge(text[..at].chars().next_back()) && edge(text[at + name.len()..].chars().next())
        })
    })
}

/// What `file` excepts about the TLS path, as messages; empty while it excepts
/// nothing. Only the list is read for names: the licence comments of
/// `deny.toml` name the TLS crates, and they are no exception. With
/// `explained`, every id of the list must also be named in a paragraph of the
/// prose above it – `.cargo/audit.toml` lists bare ids, so the prose is where
/// the package of each is said, and that is read too.
fn tls_exceptions(file: &str, config: &str, explained: bool) -> Vec<String> {
    let (prose, list) = split_at_ignore(config);
    let mut found = Vec::new();
    for id in ids_in(list) {
        if ONCE_EXCEPTED.contains(&id.as_str()) {
            found.push(format!(
                "{file} excepts {id}, an advisory about the TLS path – remove it and take the fix"
            ));
        }
    }
    if let Some(name) = names_tls_package(list) {
        found.push(format!(
            "{file}: the ignore list names {name} – remove the exception and take the fix"
        ));
    }
    if explained {
        let paragraphs: Vec<&str> = prose.split("\n#\n").collect();
        for id in ids_in(list) {
            let about: Vec<&str> = paragraphs
                .iter()
                .copied()
                .filter(|p| p.contains(&id))
                .collect();
            if about.is_empty() {
                found.push(format!(
                    "{file} excepts {id} and says nowhere what it is about – write its reason \
                     above the list"
                ));
            }
            for paragraph in about {
                if let Some(name) = names_tls_package(paragraph) {
                    found.push(format!(
                        "{file} excepts {id}, explained as about {name} – remove the exception \
                         and take the fix"
                    ));
                }
            }
        }
    }
    found
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

/// The release triple of `version`, or `None` for a pre-release or anything
/// that is not one.
fn triple(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.split('+').next().unwrap_or(version);
    if core.contains('-') {
        return None;
    }
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(Some(a)), Some(Some(b)), Some(Some(c)), None) => Some((a, b, c)),
        _ => None,
    }
}

/// The entries of the lock below their floor, as messages; empty when none is.
/// webpki-roots is held to its floor in the 1.x line only; the 0.26 shim is
/// test 3's.
fn below_floor(lock: &str) -> Vec<String> {
    let floors = [
        (TLS, TLS_FLOOR, None),
        (WEBPKI, WEBPKI_FLOOR, None),
        (ROOTS, ROOTS_FLOOR, Some(1)),
    ];
    let mut found = Vec::new();
    for p in packages(lock) {
        for (name, floor, major) in floors {
            if p.name != name {
                continue;
            }
            let (a, b, c) = floor;
            match triple(&p.version) {
                Some(v) if major.is_some_and(|m| v.0 != m) => {}
                Some(v) if v >= floor => {}
                _ => found.push(format!(
                    "Cargo.lock: {name} {} is below {a}.{b}.{c}, the first with the fixes, or \
                     a pre-release – take {a}.{b}.{c} or later with cargo update --precise",
                    p.version
                )),
            }
        }
    }
    found
}

/// What is wrong with each webpki-roots 0.26.x of the lock, as messages: the
/// shim may depend on one webpki-roots 1.x and on nothing else.
fn shim_problems(lock: &str) -> Vec<String> {
    let mut found = Vec::new();
    for p in packages(lock) {
        if p.name != ROOTS || !p.version.starts_with("0.26.") {
            continue;
        }
        let one_roots = p.dependencies.len() == 1
            && p.dependencies[0]
                .strip_prefix(&format!("{ROOTS} "))
                .is_some_and(|v| v.starts_with("1."));
        if !one_roots {
            found.push(format!(
                "Cargo.lock: {ROOTS} {} depends on {:?}, not on one {ROOTS} 1.x – the shim \
                 would carry roots of its own; find what brought it",
                p.version, p.dependencies
            ));
        }
    }
    found
}

/// The manifests that name another HTTP or TLS stack, as messages.
fn other_stacks(manifests: &[(String, String)]) -> Vec<String> {
    let mut found = Vec::new();
    for (path, text) in manifests {
        for word in FORBIDDEN_IN_MANIFEST {
            if text.contains(word) {
                found.push(format!(
                    "{path} names {word:?}; ureq is the one TLS client of the workspace, held \
                     to the floors of this guard – correct {path}"
                ));
            }
        }
    }
    found
}

/// The root manifest and those of its members, as (path, text).
fn workspace_manifests() -> Vec<(String, String)> {
    let root = workspace_dir();
    let top = read(&root.join("Cargo.toml"));
    let members = top
        .lines()
        .find_map(|l| l.trim().strip_prefix("members = ["))
        .expect("the root manifest lists its members on one line");
    let mut paths = vec!["Cargo.toml".to_string()];
    paths.extend(
        members
            .trim_end_matches(']')
            .split(',')
            .map(|m| m.trim().trim_matches('"'))
            .filter(|m| !m.is_empty())
            .map(|m| format!("{m}/Cargo.toml")),
    );
    paths
        .into_iter()
        .map(|p| {
            let text = read(&root.join(&p));
            (p, text)
        })
        .collect()
}

#[test]
fn no_config_excepts_an_advisory_about_the_tls_path() {
    let root = workspace_dir();
    let mut found = tls_exceptions(
        ".cargo/audit.toml",
        &read(&root.join(".cargo/audit.toml")),
        true,
    );
    found.extend(tls_exceptions(
        "deny.toml",
        &read(&root.join("deny.toml")),
        false,
    ));
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn the_tls_crates_are_at_their_floors() {
    let found = below_floor(&read(&workspace_dir().join("Cargo.lock")));
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn the_roots_shim_carries_no_roots_of_its_own() {
    let found = shim_problems(&read(&workspace_dir().join("Cargo.lock")));
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn no_manifest_names_another_tls_stack() {
    let manifests = workspace_manifests();
    assert!(manifests.len() >= 3, "the root and its two members");
    let found = other_stacks(&manifests);
    assert!(found.is_empty(), "{}", found.join("\n"));
}

/// `config` with `line` added as the last line of its ignore list.
fn with_entry(config: &str, line: &str) -> String {
    let at = config.rfind("\n]").expect("a list to add to");
    format!("{}\n    {line}{}", &config[..at], &config[at..])
}

#[test]
fn each_check_fails_on_made_up_inputs() {
    // The lists.
    let audit = "# **Typst.** RUSTSEC-2026-0194 is in quick-xml 0.38.4.\n#\n\
                 # Not a list: RUSTSEC-2026-0381 named in prose only.\n\
                 [advisories]\nignore = [\n    \"RUSTSEC-2026-0194\",\n]\n";
    assert!(tls_exceptions("a", audit, true).is_empty());
    let back = with_entry(audit, "\"RUSTSEC-2026-0381\",");
    assert!(tls_exceptions("a", &back, true)
        .iter()
        .any(|w| w.contains("RUSTSEC-2026-0381")));
    let unexplained = with_entry(audit, "\"RUSTSEC-2026-0400\",");
    assert!(tls_exceptions("a", &unexplained, true)
        .iter()
        .any(|w| w.contains("says nowhere")));
    let about_roots = format!("# RUSTSEC-2026-0400 is in {ROOTS} 1.0.10.\n#\n{unexplained}");
    assert!(tls_exceptions("a", &about_roots, true)
        .iter()
        .any(|w| w.contains("explained as about webpki-roots")));
    let deny = format!(
        "# `{WEBPKI}` and `untrusted`, under `ureq`.\n    \"ISC\",\n\
         ignore = [\n    {{ id = \"RUSTSEC-2026-0194\", reason = \"quick-xml\" }},\n]\n"
    );
    assert!(
        tls_exceptions("d", &deny, false).is_empty(),
        "a licence comment is no exception"
    );
    let by_reason = with_entry(
        &deny,
        &format!("{{ id = \"RUSTSEC-2026-0400\", reason = \"{WEBPKI} 0.103.17\" }},"),
    );
    assert!(tls_exceptions("d", &by_reason, false)
        .iter()
        .any(|w| w.contains(WEBPKI)));
    let by_id = with_entry(&deny, "{ id = \"RUSTSEC-2026-0382\" },");
    assert!(tls_exceptions("d", &by_id, false)
        .iter()
        .any(|w| w.contains("RUSTSEC-2026-0382")));
    let in_comment = with_entry(&deny, &format!("# {TLS} until it matures"));
    assert!(!tls_exceptions("d", &in_comment, false).is_empty());
    assert_eq!(names_tls_package(&format!("{ROOTS}-extra")), None);
    assert_eq!(
        names_tls_package(&format!("`{ROOT_CERTS}`")),
        Some(ROOT_CERTS)
    );

    // The floors.
    let lock = format!(
        "[[package]]\nname = \"{TLS}\"\nversion = \"0.23.46\"\ndependencies = [\n \"{WEBPKI}\",\n]\n\n\
         [[package]]\nname = \"{WEBPKI}\"\nversion = \"0.103.16\"\n\n\
         [[package]]\nname = \"{ROOTS}\"\nversion = \"0.26.11\"\ndependencies = [\n \"{ROOTS} 1.0.10\",\n]\n\n\
         [[package]]\nname = \"{ROOTS}\"\nversion = \"1.0.10\"\ndependencies = [\n \"pki-types\",\n]\n"
    );
    assert!(below_floor(&lock).is_empty());
    assert!(shim_problems(&lock).is_empty());
    for (from, to, name) in [
        ("\"0.23.46\"", "\"0.23.45\"", TLS),
        ("\"0.103.16\"", "\"0.103.15\"", WEBPKI),
        ("\"0.103.16\"", "\"0.104.0-alpha.8\"", WEBPKI),
        ("version = \"1.0.10\"", "version = \"1.0.9\"", ROOTS),
    ] {
        let low = lock.replace(from, to);
        assert!(
            below_floor(&low)
                .iter()
                .any(|w| w.contains(&format!("{name} "))),
            "{name} at {to}"
        );
    }
    assert!(below_floor(&lock.replace("\"0.23.46\"", "\"0.23.47\"")).is_empty());

    // The shim.
    let own_roots = lock.replace(&format!(" \"{ROOTS} 1.0.10\",\n]"), " \"pki-types\",\n]");
    assert!(!shim_problems(&own_roots).is_empty());
    let two = lock.replace(
        &format!(" \"{ROOTS} 1.0.10\",\n]"),
        &format!(" \"{ROOTS} 1.0.10\",\n \"other\",\n]"),
    );
    assert!(!shim_problems(&two).is_empty());

    // The manifests.
    let clean = vec![(
        "cli/Cargo.toml".to_string(),
        "ureq = { version = \"2.12\" }".to_string(),
    )];
    assert!(other_stacks(&clean).is_empty());
    for word in FORBIDDEN_IN_MANIFEST {
        let named = vec![("cli/Cargo.toml".to_string(), format!("{word} = \"1\""))];
        assert!(other_stacks(&named)
            .iter()
            .any(|w| w.contains("correct cli/Cargo.toml")));
    }
}
