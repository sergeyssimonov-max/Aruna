//! Scratch: does building the package twice produce the same package?
//!
//! Reproducibility is the third of this project's priorities and was never
//! checked. It matters more for the next stage than for this one: a converter
//! that maps 23 936 documents to 23 936 PDFs has to place each one where the
//! last run placed it, or every re-run rewrites the whole corpus and no
//! incremental anything is possible.
use aruna::export::{self, PACKAGE};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() {
    let zip = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .expect("usage: determinism <zip>"),
    );
    let root = std::env::temp_dir().join(format!("aruna-determinism-{}", std::process::id()));
    let (a, b) = (root.join("a"), root.join("b"));
    for d in [&a, &b] {
        std::fs::create_dir_all(d).expect("mkdir");
    }

    let mut digests: Vec<BTreeMap<PathBuf, String>> = Vec::new();
    for (i, dest) in [&a, &b].into_iter().enumerate() {
        let start = Instant::now();
        let built = export::build(&zip, dest, "determinism", &aruna::job::Job::unattended())
            .expect("build");
        eprintln!(
            "build {}: {} documents in {} groups, {:.1}s",
            i + 1,
            built.documents,
            built.groups,
            start.elapsed().as_secs_f64()
        );
        digests.push(walk(&dest.join(PACKAGE)));
    }

    let (first, second) = (&digests[0], &digests[1]);
    let only_first: Vec<_> = first.keys().filter(|k| !second.contains_key(*k)).collect();
    let only_second: Vec<_> = second.keys().filter(|k| !first.contains_key(*k)).collect();
    let differing: Vec<_> = first
        .iter()
        .filter(|(k, v)| second.get(*k).is_some_and(|w| w != *v))
        .map(|(k, _)| k)
        .collect();

    println!();
    println!("files: {} and {}", first.len(), second.len());
    println!("only in the first build:  {}", only_first.len());
    println!("only in the second build: {}", only_second.len());
    println!("same path, different bytes: {}", differing.len());
    for path in differing.iter().take(5) {
        println!("  {}", path.display());
    }
    for path in only_first.iter().chain(&only_second).take(5) {
        println!("  unmatched: {}", path.display());
    }
    println!();
    if only_first.is_empty() && only_second.is_empty() && differing.is_empty() {
        println!("--- the two builds are byte-identical ---");
    } else {
        println!("--- THE BUILD IS NOT REPRODUCIBLE ---");
    }

    let _ = std::fs::remove_dir_all(&root);
}

/// Every file under `root`, relative, with the digest of its contents.
fn walk(root: &Path) -> BTreeMap<PathBuf, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read_dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let relative = path.strip_prefix(root).expect("under root").to_path_buf();
                out.insert(relative, aruna::md5::md5_file(&path).expect("digest"));
            }
        }
    }
    out
}
