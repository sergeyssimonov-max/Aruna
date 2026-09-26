//! The seams a window will drive, held to what a window needs of them.
//!
//! None of this is interface work and none of it names Tauri. It is the part of
//! "a desktop application will sit on top of this" that is already a question
//! about reliability, and therefore already answerable:
//!
//! * the work runs on a thread that is not the caller's, and stops when the
//!   caller — on another thread — says so;
//! * the library says what it is doing through the sink it was given and
//!   through nothing else, and never ends the process;
//! * where the package goes is the caller's decision, not the environment's.
//!
//! Each is cheap to check now and impossible to retrofit once a window is
//! written against the current shape.

mod support;

use aruna::app::{self, CorpusRequest};
use aruna::error::ArunaError;
use aruna::export::{self, PACKAGE};
use aruna::job::{Cancel, Job};
use aruna::progress::{Event, Progress};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Mutex;
use support::{archive, manuscript};
use tempfile::tempdir;

/// An archive of `n` manuscripts over a few groups.
fn corpus(dir: &Path, n: usize) -> PathBuf {
    let entries: Vec<(String, String)> = (0..n)
        .map(|i| {
            (
                format!("root/CTH {}_XML_HFR/doc {i}.xml", i % 5),
                manuscript(&format!("KBo {i}"), "FB", "2017-03-28"),
            )
        })
        .collect();
    let borrowed: Vec<(&str, String)> = entries
        .iter()
        .map(|(path, body)| (path.as_str(), body.clone()))
        .collect();
    archive(&dir.join("corpus.zip"), &borrowed)
}

/// Everything under `root`, relative to it.
fn files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path.strip_prefix(root).expect("under root").to_path_buf());
            }
        }
    }
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// Off the calling thread
// ---------------------------------------------------------------------------

/// A sink that stops at one stage and waits to be released.
///
/// The handshake is what makes the test deterministic rather than timed. A test
/// that cancelled after a sleep would be asserting how fast the machine is: on
/// a quick one the build finishes first and the run is not cancelled at all.
/// Here the worker announces the stage and blocks inside `report` until the
/// other thread has set the flag, so the next `job.check` is guaranteed to see
/// it — and what is proven is exactly the thing a window needs, that a flag set
/// on one thread stops work running on another.
struct Handshake {
    at: &'static str,
    reached: Sender<()>,
    /// Behind a `Mutex` because `Progress` is `Sync` and a `Receiver` is not.
    /// That requirement is the point rather than an inconvenience: a sink a
    /// window installs is read from whatever thread the work landed on, and
    /// the compiler is what holds every implementation to it.
    released: Mutex<Receiver<()>>,
}

impl Progress for Handshake {
    fn report(&self, event: Event<'_>) {
        let name = match event {
            Event::ReadingHeaders => "ReadingHeaders",
            Event::WritingDocuments { .. } => "WritingDocuments",
            _ => "other",
        };
        if name != self.at {
            return;
        }
        // Both sides are ignored on failure: once the other thread has stopped
        // listening, this run is on its way out and has nothing to say.
        let _ = self.reached.send(());
        if let Ok(released) = self.released.lock() {
            let _ = released.recv();
        }
    }
}

/// The build runs on a thread of its own and is stopped from the caller's.
#[test]
fn a_build_on_another_thread_is_stopped_from_this_one() {
    let dir = tempdir().expect("tempdir");
    let zip = corpus(dir.path(), 40);
    let destination = dir.path().join("out");
    std::fs::create_dir(&destination).expect("destination");

    let cancel = Cancel::new();
    let (reached, at_the_stage) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let sink = Handshake {
        at: "WritingDocuments",
        reached,
        released: Mutex::new(released),
    };

    let outcome = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let job = Job::new(&sink, &cancel);
            export::build(&zip, &destination, "label", &job)
        });

        at_the_stage.recv().expect("the worker reached the stage");
        cancel.cancel();
        let _ = release.send(());

        worker.join().expect("the worker did not panic")
    });

    assert!(
        matches!(outcome, Err(ArunaError::Cancelled { .. })),
        "a flag set on this thread did not stop the work on the other: {outcome:?}"
    );
    assert_eq!(
        files(&destination),
        Vec::<PathBuf>::new(),
        "the build stopped from another thread left something behind"
    );
}

// ---------------------------------------------------------------------------
// Nothing printed, nothing exited
// ---------------------------------------------------------------------------

/// Whether `source` calls `name` — as a whole name, not as a tail of one.
///
/// `println!` is a suffix of `eprintln!`, so a plain substring search reported
/// the one file that is allowed to print as the one file that must not.
fn calls(source: &str, name: &str) -> bool {
    source.match_indices(name).any(|(at, _)| {
        at == 0
            || !source[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// Every `.rs` file under `src`, with its path.
fn library_sources() -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read src").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let relative = path.strip_prefix(&root).expect("under src").to_path_buf();
                out.push((relative, std::fs::read_to_string(&path).expect("read")));
            }
        }
    }
    assert!(out.len() > 10, "the source scan found almost nothing");
    out
}

/// The library says what it is doing through the sink and nowhere else, and it
/// never ends the process.
///
/// A grep, and deliberately so: the property is about the whole of `src`, and
/// there is no way to observe "nobody anywhere called `println!`" from a
/// running test. What it protects is the thing that makes a window possible at
/// all — a file descriptor is not something a window can show, and a library
/// that calls `process::exit` takes the application down with the operation.
///
/// Two files are exempt, each for one reason. `main.rs` is the front end: it is
/// what prints, and the whole point of the split is that it is the only one.
/// `progress.rs` holds `Stderr`, the sink the binary brings — the one place
/// where an event becomes a line, chosen by the caller rather than by the code
/// doing the work.
#[test]
fn the_library_neither_prints_nor_ends_the_process() {
    let found: Vec<String> = library_sources()
        .iter()
        .flat_map(|(path, source)| offences(&path.to_string_lossy(), source))
        .collect();
    assert!(
        found.is_empty(),
        "a library that writes to a file descriptor or ends the process cannot \
         be driven by a window; report an Event through the job instead: {found:#?}"
    );
}

/// What in `source` writes to a standard stream or ends the process, for the
/// file `name`.
///
/// `main.rs` is the entry point and may do all of it. `progress.rs` holds the
/// binary's `Stderr` sink, the one place an event becomes a line, and may
/// write to standard error and nothing else. `dbg!`, `eprint!` and the streams
/// taken directly were not looked for until 2026-09-23; none is used today,
/// and this keeps it so.
fn offences(name: &str, source: &str) -> Vec<String> {
    let anywhere = [
        "println!",
        "print!",
        "dbg!",
        "io::stdout(",
        "process::exit",
        "process::abort",
    ];
    let stderr = ["eprintln!", "eprint!", "io::stderr("];
    let mut found = Vec::new();
    for forbidden in anywhere {
        if calls(source, forbidden) && name != "main.rs" {
            found.push(format!("{name}: {forbidden}"));
        }
    }
    for forbidden in stderr {
        if calls(source, forbidden) && name != "main.rs" && name != "progress.rs" {
            found.push(format!("{name}: {forbidden}"));
        }
    }
    found
}

/// **The guard above has teeth.** Each thing it forbids, planted in a file of
/// the library, is named; the same text in `main.rs` is not.
#[test]
fn the_print_guard_names_what_is_planted() {
    for planted in [
        "println!(\"x\")",
        "print!(\"x\")",
        "dbg!(x)",
        "std::io::stdout()",
        "std::process::exit(1)",
        "eprintln!(\"x\")",
        "eprint!(\"x\")",
        "std::io::stderr()",
    ] {
        let source = format!("fn f() {{ {planted}; }}");
        assert_eq!(
            offences("export/mod.rs", &source).len(),
            1,
            "`{planted}` in the library went unnoticed"
        );
        assert!(
            offences("main.rs", &source).is_empty(),
            "main.rs may: {planted}"
        );
    }
    assert!(offences("export/mod.rs", "fn f() { let printer = 1; }").is_empty());
}

// ---------------------------------------------------------------------------
// The destination is the caller's to name
// ---------------------------------------------------------------------------

/// The scenario builds where it is told, not where the environment says.
///
/// `build_corpus` still answers "wherever this platform keeps downloads", which
/// is right for the binary and wrong for anything else: a window that lets
/// someone choose a folder had no way to say so, and a test had to move `HOME`
/// to keep the corpus out of the real Downloads directory. The destination is
/// now an argument, and this is what says the argument is honoured.
#[test]
fn the_corpus_is_built_where_the_caller_says() {
    let dir = tempdir().expect("tempdir");
    let zip = corpus(dir.path(), 12);
    let destination = dir.path().join("chosen");
    std::fs::create_dir(&destination).expect("destination");

    let report = app::build_corpus_into(
        &CorpusRequest {
            local_archive: Some(zip),
        },
        &destination,
        &Job::unattended(),
    )
    .expect("builds");

    assert_eq!(report.package.root, destination.join(PACKAGE));
    assert!(report.inventory.is_file(), "the inventory is where it says");
    assert!(
        report.inventory.starts_with(&destination),
        "the inventory landed outside the folder the caller named"
    );

    // And nowhere near the folder the environment would have chosen.
    if let Ok(downloads) = aruna::paths::downloads_dir() {
        assert!(
            !report.package.root.starts_with(&downloads),
            "the caller named a folder and the package went to {}",
            downloads.display()
        );
    }
}

/// **Два прогона в одном процессе дают побайтово одно и то же.**
///
/// Свойство, без которого окно не построить: пользователь нажимает «Собрать»
/// второй раз, не перезапуская программу. Ловится здесь не расхождение сумм, а
/// три вещи, которые к нему приводят: состояние, пережившее операцию;
/// инициализация, срабатывающая один раз на процесс; и общий изменяемый
/// объект, до которого добрались оба прогона.
///
/// Каталоги назначения разные нарочно – в них не должно быть ничего от
/// прошлого раза, и сравнение идет по содержимому, а не по времени файлов.
#[test]
fn two_runs_in_one_process_produce_the_same_bytes() {
    let dir = tempdir().expect("tempdir");
    let zip = corpus(dir.path(), 24);

    let build = |name: &str| {
        let destination = dir.path().join(name);
        std::fs::create_dir(&destination).expect("destination");
        let report = app::build_corpus_into(
            &CorpusRequest {
                local_archive: Some(zip.clone()),
            },
            &destination,
            &Job::unattended(),
        )
        .expect("builds");
        report.package.root
    };

    let first = build("first");
    let second = build("second");

    let listing = |root: &Path| {
        let mut names: Vec<PathBuf> = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read_dir").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    names.push(path.strip_prefix(root).expect("under root").to_path_buf());
                }
            }
        }
        names.sort();
        names
    };

    let names = listing(&first);
    assert_eq!(
        names,
        listing(&second),
        "the two runs wrote different files"
    );
    assert!(!names.is_empty(), "the run wrote nothing at all");

    for name in &names {
        let a = std::fs::read(first.join(name)).expect("read first");
        let b = std::fs::read(second.join(name)).expect("read second");
        assert!(
            a == b,
            "{} differs between the first run and the second",
            name.display()
        );
    }
}

/// **С приемником прогресса и без него получается одно и то же, а этапы идут
/// в одном порядке.**
///
/// Приемник заведен под окно, и это проверка обоих его свойств сразу: он
/// слышит ход работы – этапы приходят в том порядке, в котором выполняются, и
/// число документов в объявлении совпадает с тем, что операция вернула, – и он
/// ни на что не влияет: пакет, собранный с ним, побайтово равен собранному без
/// него.
///
/// Чего здесь нет намеренно: проверки счетчика по документам. Событие о записи
/// приходит один раз на весь этап, а не по документу, так что полосу с
/// движущимся числом по нему не нарисовать. Это шов, которого не хватает, и
/// добавить его – значит изменить перечень событий; находка вынесена в отчет,
/// а не сделана здесь.
#[test]
fn a_run_with_a_sink_reports_its_stages_and_changes_nothing() {
    #[derive(Default)]
    struct Stages {
        seen: Mutex<Vec<&'static str>>,
        documents: Mutex<Vec<usize>>,
    }

    impl Progress for Stages {
        fn report(&self, event: Event<'_>) {
            let name = match event {
                Event::ReadingHeaders => "ReadingHeaders",
                Event::HeadersRead { .. } => "HeadersRead",
                Event::WritingDocuments { documents } => {
                    self.documents.lock().expect("not poisoned").push(documents);
                    "WritingDocuments"
                }
                Event::CheckingPackage => "CheckingPackage",
                Event::CheckingPublished => "CheckingPublished",
                _ => "other",
            };
            self.seen.lock().expect("not poisoned").push(name);
        }
    }

    let dir = tempdir().expect("tempdir");
    let zip = corpus(dir.path(), 30);

    let build = |name: &str, job: &Job<'_>| {
        let destination = dir.path().join(name);
        std::fs::create_dir(&destination).expect("destination");
        let built = export::build(&zip, &destination, "seam", job).expect("builds");
        (destination.join(PACKAGE), built)
    };

    let watched = Stages::default();
    let cancel = Cancel::new();
    let (with_sink, report) = build("watched", &Job::new(&watched, &cancel));
    let (without_sink, quiet) = build("silent", &Job::unattended());

    // Этапы: те, что должны быть, и в том порядке, в каком идут.
    let seen = watched.seen.lock().expect("not poisoned").clone();
    let at = |name: &str| seen.iter().position(|s| *s == name);
    for name in [
        "ReadingHeaders",
        "WritingDocuments",
        "CheckingPackage",
        "CheckingPublished",
    ] {
        assert!(at(name).is_some(), "{name} не прозвучал: {seen:?}");
    }
    assert!(
        at("ReadingHeaders") < at("WritingDocuments"),
        "запись объявлена раньше чтения заголовков: {seen:?}"
    );
    assert!(
        at("WritingDocuments") < at("CheckingPackage"),
        "проверка объявлена раньше записи: {seen:?}"
    );
    assert!(
        at("CheckingPackage") < at("CheckingPublished"),
        "опубликованное проверено раньше собранного: {seen:?}"
    );

    let announced = watched.documents.lock().expect("not poisoned").clone();
    assert_eq!(
        announced,
        vec![report.documents],
        "объявленное число документов не совпало с записанным"
    );

    // И приемник ничего не изменил: два пакета побайтово равны.
    assert_eq!(report.documents, quiet.documents);
    let inventory = |root: &Path| std::fs::read(root.join(aruna::paths::OUTPUT_FILE_NAME));
    assert!(
        inventory(&with_sink).expect("watched inventory")
            == inventory(&without_sink).expect("silent inventory"),
        "опись, собранная с приемником, отличается от собранной без него"
    );
}

/// The work divides by CTH group, and a group's part is the whole's part.
///
/// A second output — a PDF per group rather than one document for the corpus —
/// needs the run to be addressable by group, and needs that address to mean the
/// same thing the full run means. The seam for it exists:
/// `export::group_slices` cuts the ordered records and their placements into
/// runs, and the manifest is its one caller today.
///
/// What nothing checked is that the cut is lossless. The doc comment on
/// `group_slices` says slicing panics if records and placements stop being
/// parallel, which covers the shapes; it does not cover the arithmetic. A run
/// that dropped the last group, or one that overlapped two, would still be
/// parallel and would still be wrong — and the manifest would describe a
/// package that is not the one on disk.
///
/// So: the groups concatenate back to exactly what was placed, in order, with
/// no document in two groups and none in none. No second pass over the archive
/// and no second copy of the selection rule — the same `place` result the build
/// writes from is the one cut here.
#[test]
fn a_group_is_the_part_of_the_whole_that_belongs_to_it() {
    let dir = tempdir().expect("tempdir");
    let zip = corpus(dir.path(), 60);

    let mut fragments = export::collect_fragments(&zip).expect("headers read");
    aruna::order::sort_by_display_order(&mut fragments, |f| &f.record);
    let records: Vec<_> = fragments.iter().map(|f| f.record.clone()).collect();
    let placed = export::place(&fragments).expect("placed");

    let mut labels = Vec::new();
    let mut records_again = Vec::new();
    let mut placed_again = Vec::new();
    for (label, run, slice) in export::group_slices(&records, &placed) {
        assert_eq!(
            run.len(),
            slice.len(),
            "a group's two halves differ in size"
        );
        assert!(!run.is_empty(), "an empty group was cut out of the whole");
        labels.push(label.to_string());
        records_again.extend(run.iter().map(|r| r.sigla.clone()));
        placed_again.extend(slice.iter().map(|p| p.relative.clone()));
    }

    let mut distinct = labels.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(
        labels.len(),
        distinct.len(),
        "one group was cut twice: {labels:?}"
    );

    assert_eq!(
        records_again,
        records.iter().map(|r| r.sigla.clone()).collect::<Vec<_>>(),
        "the groups do not add up to the records the build placed"
    );
    assert_eq!(
        placed_again,
        placed
            .iter()
            .map(|p| p.relative.clone())
            .collect::<Vec<_>>(),
        "the groups do not add up to the placements the build writes"
    );

    // And the whole is what the build actually wrote: the same corpus, built,
    // holds one file per placement and nothing else besides the five root
    // files — inventory, manifest, the font and its terms, the CTH titles'
    // terms.
    let destination = dir.path().join("out");
    std::fs::create_dir(&destination).expect("destination");
    export::build(&zip, &destination, "seams", &Job::unattended()).expect("builds");
    let written = files(&destination.join(PACKAGE));
    assert_eq!(
        written.len(),
        placed.len() + aruna::export::ROOT_FILES.len(),
        "the package holds something other than the placements plus the root files"
    );
    for placement in &placed {
        assert!(
            written.contains(&placement.relative),
            "a placed document is not in the package: {:?}",
            placement.relative
        );
    }
}

// ---------------------------------------------------------------------------
// Every failure the core can report reaches the window as a Russian sentence
// ---------------------------------------------------------------------------

/// One value of every variant, with a path from the machine in each field that
/// can carry one.
///
/// `covers` is a `match` with no wildcard: a variant added to `ArunaError`
/// stops this file compiling until it is listed here too, which is the only
/// way a list of "every variant" stays one.
fn every_failure() -> Vec<ArunaError> {
    use aruna::job::Phase;
    let secret = PathBuf::from("/Users/nobody/Downloads/secret-corpus");
    let io = || std::io::Error::other("busy");
    let all = vec![
        ArunaError::Network {
            url: "https://example.invalid/a.zip".into(),
            source: Box::new(io()),
        },
        ArunaError::Http {
            url: "u".into(),
            status: 503,
            retry_after: None,
        },
        ArunaError::Http {
            url: "u".into(),
            status: 404,
            retry_after: None,
        },
        ArunaError::Truncated {
            url: "u".into(),
            expected: 10,
            got: 4,
        },
        ArunaError::Oversized {
            url: "u".into(),
            limit: 1,
            got: 2,
        },
        ArunaError::FontMissing {
            path: secret.join("fonts/UllikummiA.ttf"),
            covers: "private use",
        },
        ArunaError::FontAltered {
            path: secret.join("fonts/UllikummiA.ttf"),
            expected: "00",
            found: "ff".into(),
        },
        ArunaError::ChecksumMismatch {
            url: "u".into(),
            expected: "a".into(),
            got: "b".into(),
        },
        ArunaError::Zip(zip::result::ZipError::FileNotFound),
        ArunaError::EmptyArchive,
        ArunaError::Cancelled {
            phase: Phase::Exporting,
        },
        ArunaError::Io {
            path: secret.clone(),
            source: io(),
        },
        ArunaError::Replace {
            path: secret.clone(),
            scratch: secret.join("scratch"),
            source: io(),
        },
        ArunaError::ExportCollision {
            group: "CTH 5".into(),
            fragment: "KBo 1.1".into(),
            first: "a.xml".into(),
            second: "b.xml".into(),
            path: secret.join("CTH 5/KBo 1.1.xml"),
        },
        ArunaError::ExportFolderCollision {
            first_group: "CTH 5a".into(),
            second_group: "CTH 5A".into(),
            first: "a.xml".into(),
            second: "b.xml".into(),
        },
        ArunaError::ArchiveDuplicateEntry {
            entry: "CTH 5/KBo 1.1.xml".into(),
        },
        ArunaError::ExportDocumentTooLarge {
            entry: "CTH 5/KBo 1.1.xml".into(),
            limit: 1,
        },
        ArunaError::ExportDistorted {
            entry: "CTH 5/KBo 1.1.xml".into(),
            reason: "encoding".into(),
        },
        ArunaError::ExportIncomplete {
            expected: 2,
            written: 1,
        },
        ArunaError::ArchiveTooManyEntries {
            entries: 2,
            limit: 1,
        },
        ArunaError::ExportPackageTooLarge {
            written: 2,
            limit: 1,
        },
        ArunaError::ExportInvalid {
            root: secret.clone(),
            count: 1,
            first: format!("{} is missing", secret.display()),
        },
        ArunaError::PublishBusy {
            path: secret.clone(),
            holder: "pid 1, since 1.0".into(),
        },
        ArunaError::ExportDestination {
            path: secret.clone(),
            reason: "it holds files this exporter did not write".into(),
        },
        ArunaError::DownloadsDir,
    ];
    for error in &all {
        covers(error);
    }
    all
}

fn covers(error: &ArunaError) {
    use ArunaError::*;
    match error {
        Network { .. }
        | Http { .. }
        | Truncated { .. }
        | Oversized { .. }
        | FontMissing { .. }
        | FontAltered { .. }
        | ChecksumMismatch { .. }
        | Zip(_)
        | EmptyArchive
        | Cancelled { .. }
        | Io { .. }
        | Replace { .. }
        | ExportCollision { .. }
        | ExportFolderCollision { .. }
        | ArchiveDuplicateEntry { .. }
        | ExportDocumentTooLarge { .. }
        | ExportDistorted { .. }
        | ExportIncomplete { .. }
        | ArchiveTooManyEntries { .. }
        | ExportPackageTooLarge { .. }
        | ExportInvalid { .. }
        | PublishBusy { .. }
        | ExportDestination { .. }
        | DownloadsDir => {}
    }
}

/// The keys of the window's table of failure sentences, read from its source.
///
/// Read rather than restated: the table lives in `App.svelte`, and a copy here
/// would be a third description of one contract. The block is found by its
/// declaration and ends at the first line that closes it.
fn window_sentences() -> std::collections::BTreeSet<String> {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../frontend/src/App.svelte"),
    )
    .expect("App.svelte");
    let start = source
        .find("const FAILED: Record<string, string | undefined> = {")
        .expect("the window's table of failure sentences");
    let body = &source[start..];
    let body = &body[..body.find("\n  }\n").expect("the end of the table")];
    body.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let (key, _) = line.split_once(':')?;
            (!key.is_empty() && key.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
                .then(|| key.to_string())
        })
        .collect()
}

/// The codes whose core message the window shows under its own sentence –
/// `DETAILED` in `App.svelte`, read from there so the two cannot drift.
fn detailed_codes() -> std::collections::BTreeSet<String> {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../frontend/src/App.svelte"),
    )
    .expect("App.svelte");
    let start = source
        .find("const DETAILED: ReadonlySet<string> = new Set([")
        .expect("the window's set of detailed codes");
    let body = &source[start..];
    let body = &body[body.find('[').expect("[") + 1..body.find(']').expect("]")];
    body.split(',')
        .map(|code| code.trim().trim_matches('\'').to_string())
        .filter(|code| !code.is_empty())
        .collect()
}

/// English words a line under a Russian sentence may not carry. The data in
/// [`every_failure`] – group, sigla, entry names – has none of them.
fn english_words_in(message: &str) -> Vec<&'static str> {
    let padded = format!(" {} ", message.to_lowercase());
    [
        " is ",
        " by ",
        " both ",
        " and ",
        " the ",
        " names ",
        " twice",
        " claimed ",
    ]
    .into_iter()
    .filter(|word| padded.contains(word))
    .collect()
}

/// Whether `message` carries anything that reads as a path on this machine.
fn carries_a_path(message: &str) -> bool {
    message.contains("/Users/") || message.contains("secret-corpus")
}

/// **Every code the core reports has a Russian sentence in the window, and no
/// two variants that mean different things share one.**
///
/// The window shows `FAILED[code]` and falls back to the core's English
/// sentence for a code it does not know. That fallback is deliberate — an
/// English line is visible and gets fixed — but it is also how a new code
/// reaches a Russian screen in English without any test noticing: the
/// window's own test lists the codes by hand. This reads the list from the
/// core and the sentences from the window, so the two cannot drift apart.
///
/// `cancelled` is the exception the window makes on purpose: its sentence is
/// chosen by phase, not by code.
#[test]
fn every_failure_reaches_the_window_as_a_russian_sentence_and_without_a_path() {
    let sentences = window_sentences();
    let mut codes = std::collections::BTreeMap::<&str, Vec<String>>::new();
    for error in every_failure() {
        let failure = app::Failure::of(&error);
        assert!(
            !carries_a_path(&failure.message),
            "{} carries a path to the window: {}",
            failure.code,
            failure.message
        );
        assert!(
            failure.code == "cancelled" || sentences.contains(failure.code),
            "the window has no sentence for `{}`; it would show the core's English",
            failure.code
        );
        codes.entry(failure.code).or_default().push(
            format!("{error:?}")
                .split([' ', '(', '{'])
                .next()
                .unwrap_or("")
                .to_string(),
        );
    }
    // A line the window shows as it came: names, and not an English sentence
    // around them. Rule 4.9 of the specification – no English line on the
    // screen on any path – was kept everywhere but here until 2026-09-24.
    let detailed = detailed_codes();
    assert!(
        detailed.contains("collision") && detailed.contains("archive_duplicate"),
        "the reader of DETAILED found {detailed:?}"
    );
    for error in every_failure() {
        let failure = app::Failure::of(&error);
        if detailed.contains(failure.code) {
            let found = english_words_in(&failure.message);
            assert!(
                found.is_empty(),
                "`{}` puts English on the window's screen: {:?} {found:?}",
                failure.code,
                failure.message
            );
        }
    }
    assert!(
        !english_words_in("CTH 1: KBo 1 is claimed by both a and b").is_empty(),
        "the English check has no teeth"
    );

    // One code, one kind of failure. `Http` answers to two codes by status,
    // never the other way round.
    for (code, variants) in &codes {
        let mut kinds = variants.clone();
        kinds.dedup();
        assert_eq!(kinds.len(), 1, "`{code}` is shared by {kinds:?}");
    }

    // Negative controls: the two checks above have teeth.
    assert!(
        !sentences.contains("quantum_flux"),
        "the table reader accepts a code the window does not have"
    );
    assert!(
        sentences.len() >= 20,
        "the table reader found {} keys; it is not reading the table",
        sentences.len()
    );
    assert!(carries_a_path(
        "I/O error at /Users/nobody/Downloads/secret-corpus: busy"
    ));
}
