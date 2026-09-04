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
    for (path, source) in library_sources() {
        let name = path.to_string_lossy();
        for forbidden in ["println!", "print!", "process::exit", "process::abort"] {
            assert!(
                !calls(&source, forbidden) || name == "main.rs",
                "{name} contains {forbidden}: a library that writes to a file \
                 descriptor or ends the process cannot be driven by a window"
            );
        }
        assert!(
            !calls(&source, "eprintln!") || name == "main.rs" || name == "progress.rs",
            "{name} contains eprintln!: report an Event through the job instead, \
             and let the caller decide where it goes"
        );
    }
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
    // holds one file per placement and nothing else besides the two root files.
    let destination = dir.path().join("out");
    std::fs::create_dir(&destination).expect("destination");
    export::build(&zip, &destination, "seams", &Job::unattended()).expect("builds");
    let written = files(&destination.join(PACKAGE));
    assert_eq!(
        written.len(),
        placed.len() + 2,
        "the package holds something other than the placements plus inventory and manifest"
    );
    for placement in &placed {
        assert!(
            written.contains(&placement.relative),
            "a placed document is not in the package: {:?}",
            placement.relative
        );
    }
}
