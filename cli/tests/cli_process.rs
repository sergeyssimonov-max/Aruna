//! The program as a process: what a person running `aruna` actually meets.
//!
//! **One artifact since 2.3.0: the package.** A run writes the corpus as
//! folders of documents with an inventory inside that links at each one.
//! Until 2.2.0 the binary wrote only a standalone inventory and never called
//! the export at all; 2.2.0 wrote both, which put two files of the same name
//! in one folder — and the linkless one is the one a reader opens first.
//!
//! Everything else in this repository tests the library. That leaves the one
//! surface users touch — the binary, its exit code, what it prints and what it
//! leaves on disk — checked by nobody. These tests run the real executable.
//!
//! Hermetic by construction, and it has to be: the inventory is written into
//! the Downloads folder, and a test that used the real one would overwrite
//! whatever is there. Three environment variables make that safe —
//!
//! * `HOME` decides where Downloads is, so the output lands in a temporary
//!   directory that goes away with the test;
//! * `ARUNA_ZIP` supplies a local archive, so nothing reaches the network;
//! * `ARUNA_CACHE_DIR` puts the cache under the same temporary directory.
//!
//! — and every test here sets all three. No test touches the real home, the
//! real cache or Zenodo.

// Taken from the crate rather than spelled again, so a rename cannot leave
// these tests asserting the old name and passing.
use aruna::export::PACKAGE;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::{tempdir, TempDir};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// What the CLI names its output, from `paths::OUTPUT_FILE_NAME`.
const OUTPUT: &str = "TLHdig_Beta_0.3.html";

/// A manuscript in the shape the corpus writes them.
fn manuscript(siglum: &str) -> String {
    format!(
        r#"<AOxml xml:space="preserve"><AOHeader><docID>{siglum}</docID><meta><uebern editor="FB" date="2017-03-28"/></meta></AOHeader><body><text><l lg="Hit"/>text</text></body></AOxml>"#
    )
}

/// A sandbox: its own home, its own cache, its own archive.
struct Sandbox {
    dir: TempDir,
}

impl Sandbox {
    fn new() -> Self {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("Downloads")).expect("downloads");
        Self { dir }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn downloads(&self) -> PathBuf {
        self.path().join("Downloads")
    }

    /// The inventory a reader opens — inside the package since 2.3.0.
    ///
    /// There used to be a second one beside it, under the same name and
    /// without links. Two files called `TLHdig_Beta_0.3.html` in one folder,
    /// one of them linkless, is the file a reader opens first and the reason
    /// they conclude the links are missing. There is one now.
    fn output(&self) -> PathBuf {
        self.package_root().join(OUTPUT)
    }

    /// The package this run wrote.
    fn package_root(&self) -> PathBuf {
        self.downloads().join(PACKAGE)
    }

    /// An archive of `entries`, written where the run can be pointed at it.
    fn archive(&self, entries: &[(&str, &str)]) -> PathBuf {
        let path = self.path().join("corpus.zip");
        let file = fs::File::create(&path).expect("create archive");
        let mut zip = ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, body) in entries {
            use std::io::Write as _;
            zip.start_file(*name, options).expect("start");
            zip.write_all(body.as_bytes()).expect("write");
        }
        zip.finish().expect("finish");
        path
    }

    /// An archive with two manuscripts in two groups — the ordinary case.
    fn corpus(&self) -> PathBuf {
        self.archive(&[
            ("root/CTH 5_XML_HFR/KBo 1.1.xml", &manuscript("KBo 1.1")),
            ("root/CTH 9_XML_TLH/KUB 2.1.xml", &manuscript("KUB 2.1")),
        ])
    }

    /// Run the binary against `archive`, with the sandbox's home and cache.
    fn run(&self, archive: &Path) -> Output {
        self.run_with(archive, &[])
    }

    /// The same run, with arguments on the command line.
    ///
    /// Only one test passes any: this program takes none, and that is the
    /// property being pinned rather than a feature being exercised.
    fn run_with(&self, archive: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_aruna"))
            .args(args)
            .env("HOME", self.path())
            .env("ARUNA_ZIP", archive)
            .env("ARUNA_CACHE_DIR", self.path().join("cache"))
            // Not inherited: a stray one from the developer's shell would make
            // the test depend on their machine.
            .env_remove("XDG_CACHE_HOME")
            .stdin(Stdio::null())
            .output()
            .expect("the binary runs")
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Whatever else a run does, it must not be a panic. Checked everywhere below,
/// because a panic is the one failure that says the program did not consider
/// the case at all.
fn assert_no_panic(out: &Output) {
    let err = stderr(out);
    assert!(
        !err.contains("panicked") && !err.contains("RUST_BACKTRACE"),
        "the run panicked:\n{err}"
    );
}

/// Anything left half-written, under any name.
///
/// The two things a run is *supposed* to leave are not leftovers: the
/// inventory, and the package directory beside it. Everything else in the
/// output directory is something that should not be there.
fn leftovers(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name != OUTPUT && name != PACKAGE)
        .collect()
}

/// The package a run wrote: its root, the CTH folders in it, and its own
/// inventory.
fn package(downloads: &Path) -> (PathBuf, usize, PathBuf) {
    let root = downloads.join(PACKAGE);
    let groups = fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .count();
    let inventory = root.join(OUTPUT);
    (root, groups, inventory)
}

#[test]
fn an_ordinary_run_writes_the_inventory_and_says_where() {
    let sandbox = Sandbox::new();
    let out = sandbox.run(&sandbox.corpus());

    assert_no_panic(&out);
    assert!(
        out.status.success(),
        "exit: {:?}\n{}",
        out.status,
        stderr(&out)
    );

    let inventory = sandbox.output();
    assert!(
        inventory.is_file(),
        "no inventory at {}",
        inventory.display()
    );

    // The path it reports is the path it wrote.
    let said = stdout(&out);
    assert!(
        said.contains(&inventory.display().to_string()),
        "stdout does not name the file it wrote: {said}"
    );

    let html = fs::read_to_string(&inventory).expect("read");
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("KBo 1.1") && html.contains("KUB 2.1"));
    assert!(html.contains("CTH 5") && html.contains("CTH 9"));

    // Nothing half-written beside it.
    assert!(leftovers(&sandbox.downloads()).is_empty());
}

/// **A zero-argument program, and this is what that means today.**
///
/// `aruna` parses no command line at all: there is no argument parser in the
/// binary, so `--help` is not a request it can refuse — it is a word the
/// program never looks at, and the run proceeds exactly as if nothing had been
/// typed. The one input it does take arrives through the environment
/// (`ARUNA_ZIP`), which is what the rest of this file uses.
///
/// This is characterization, not endorsement. A person typing `--help` and
/// getting a full corpus run instead of a usage line is a reasonable thing to
/// change; the point of pinning it is that changing it then reads as a
/// decision — this test failing — rather than as a silent difference between
/// two versions. Until then, an argument parser bolted on without touching
/// this test would be an unnoticed change of contract.
#[test]
fn the_command_line_is_not_read_and_arguments_change_nothing() {
    let sandbox = Sandbox::new();
    let corpus = sandbox.corpus();

    let plain = sandbox.run(&corpus);
    let inventory = fs::read_to_string(sandbox.output()).expect("read");
    fs::remove_dir_all(sandbox.package_root()).expect("clear");

    for args in [
        vec!["--help"],
        vec!["--version"],
        vec!["nonsense"],
        vec!["-x", "--", "/etc/passwd"],
    ] {
        let out = sandbox.run_with(&corpus, &args);
        assert_no_panic(&out);
        assert_eq!(
            out.status.code(),
            plain.status.code(),
            "{args:?} changed the exit code"
        );
        assert_eq!(
            stdout(&out),
            stdout(&plain),
            "{args:?} changed what the program says"
        );
        assert_eq!(
            fs::read_to_string(sandbox.output()).expect("read"),
            inventory,
            "{args:?} changed what the program wrote"
        );
        fs::remove_dir_all(sandbox.package_root()).expect("clear");
    }
}

/// **The run writes the corpus, not only a list of it.**
///
/// The contract this test exists for: one run, two artifacts. Until 2.2.0 the
/// binary wrote the inventory and stopped, and the table it wrote had nothing
/// to click — the export that turns the archive into folders of documents was
/// reachable only by `cargo run --example export_beta`. A reader who installed
/// the application never saw it.
#[test]
fn an_ordinary_run_writes_the_package_with_its_inventory_inside() {
    let sandbox = Sandbox::new();
    let out = sandbox.run(&sandbox.corpus());
    assert_no_panic(&out);
    assert!(out.status.success(), "{}", stderr(&out));

    let (root, groups, inventory) = package(&sandbox.downloads());
    assert!(root.is_dir(), "no package at {}", root.display());
    assert_eq!(groups, 2, "one directory per CTH group of the archive");
    assert!(
        inventory.is_file(),
        "the package carries no inventory of its own"
    );

    // The documents themselves, under the group they belong to.
    for (group, siglum) in [("CTH 5", "KBo 1.1"), ("CTH 9", "KUB 2.1")] {
        let document = root.join(group).join(format!("{siglum}.xml"));
        assert!(document.is_file(), "no {}", document.display());
        let xml = fs::read_to_string(&document).expect("read");
        assert!(
            xml.starts_with("<?xml"),
            "the document lost its declaration"
        );
        assert!(xml.contains(siglum), "the document is not the one named");
    }

    // The difference between the two inventories, and the whole point of the
    // package: this one links at the documents beside it.
    let linked = fs::read_to_string(&inventory).expect("read");
    assert!(
        linked.contains("href=\"./CTH%205/KBo%201.1.xml\""),
        "the package's inventory does not link at its documents"
    );
    // There is no second inventory beside the package any more, and nothing
    // else in Downloads either.
    assert_eq!(
        fs::read_dir(sandbox.downloads()).expect("read").count(),
        1,
        "the run left something beside the package"
    );

    // And the run says where both the package and its inventory are, because a
    // reader cannot click what they were not told about.
    let said = stdout(&out);
    assert!(
        said.contains(&root.display().to_string()),
        "stdout does not name the package: {said}"
    );
    assert!(
        said.contains(&inventory.display().to_string()),
        "stdout does not name the inventory: {said}"
    );
}

#[test]
fn a_second_run_replaces_the_inventory_rather_than_adding_to_it() {
    let sandbox = Sandbox::new();
    let archive = sandbox.corpus();

    let first = sandbox.run(&archive);
    assert!(first.status.success());
    let before = fs::read(sandbox.output()).expect("read");

    let second = sandbox.run(&archive);
    assert_no_panic(&second);
    assert!(second.status.success());

    // The inventory carries a timestamp, so the two are not byte-identical —
    // but everything else about them is, and there is still exactly one file.
    let after = fs::read(sandbox.output()).expect("read");
    assert_eq!(
        before.len().abs_diff(after.len()),
        0,
        "the second run produced a different amount of inventory"
    );
    // Two entries, not three: the inventory and the package, each replaced
    // rather than added to.
    assert_eq!(
        fs::read_dir(sandbox.downloads()).expect("read").count(),
        1,
        "a second run left more than the package behind"
    );
    assert!(leftovers(&sandbox.downloads()).is_empty());
}

#[test]
fn a_missing_archive_fails_without_panicking_and_writes_nothing() {
    let sandbox = Sandbox::new();
    let out = sandbox.run(&sandbox.path().join("not-here.zip"));

    assert_no_panic(&out);
    assert!(!out.status.success(), "a missing archive must not succeed");
    assert!(
        stderr(&out).contains("Ошибка"),
        "no diagnosis: {}",
        stderr(&out)
    );
    assert!(!sandbox.output().exists(), "an inventory appeared anyway");
    assert!(leftovers(&sandbox.downloads()).is_empty());
}

#[test]
fn a_corrupt_archive_is_reported_rather_than_parsed() {
    let sandbox = Sandbox::new();
    let path = sandbox.path().join("corrupt.zip");
    fs::write(&path, b"this is not a zip file at all").expect("write");

    let out = sandbox.run(&path);
    assert_no_panic(&out);
    assert!(!out.status.success());
    assert!(!sandbox.output().exists());
}

#[test]
fn an_archive_with_no_manuscripts_is_refused_with_an_explanation() {
    let sandbox = Sandbox::new();
    // Named like manuscripts, but the content gate says otherwise.
    let archive = sandbox.archive(&[
        ("root/CTH 1_XML/notes.txt", "not xml at all"),
        (
            "root/CTH 1_XML/page.xml",
            "<html><body>an encrypted blob would look like this</body></html>",
        ),
    ]);

    let out = sandbox.run(&archive);
    assert_no_panic(&out);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(
        err.contains("Архив") || err.contains("XML"),
        "the message does not say what was wrong: {err}"
    );
    assert!(!sandbox.output().exists());
}

/// The destination is checked before the work, not after it.
///
/// A run costs a download and a full parse, and finding out at the end that the
/// inventory cannot be written is the same error delivered as late as possible.
/// `lib::run` asks first; this is what holds it to that.
#[cfg(unix)]
#[test]
fn an_unwritable_destination_fails_before_the_parsing_starts() {
    use std::os::unix::fs::PermissionsExt;

    let sandbox = Sandbox::new();
    let archive = sandbox.corpus();
    let downloads = sandbox.downloads();
    fs::set_permissions(&downloads, fs::Permissions::from_mode(0o500)).expect("chmod");

    let out = sandbox.run(&archive);

    // Put it back before asserting, so a failure here cannot leave an
    // undeletable directory behind for the tempdir to trip over.
    fs::set_permissions(&downloads, fs::Permissions::from_mode(0o755)).expect("chmod");

    assert_no_panic(&out);
    assert!(!out.status.success(), "an unwritable destination must fail");
    assert!(
        !stderr(&out).contains("Parsing XML manuscripts"),
        "it parsed the archive before finding out it could not write:\n{}",
        stderr(&out)
    );
    assert!(!sandbox.output().exists());
}

/// A cache that cannot be written is a reason to do without one, never a reason
/// to fail a run that would otherwise succeed.
#[cfg(unix)]
#[test]
fn an_unwritable_cache_directory_does_not_fail_the_run() {
    use std::os::unix::fs::PermissionsExt;

    let sandbox = Sandbox::new();
    let archive = sandbox.corpus();
    let cache = sandbox.path().join("cache");
    fs::create_dir_all(&cache).expect("mkdir");
    fs::set_permissions(&cache, fs::Permissions::from_mode(0o500)).expect("chmod");

    let out = sandbox.run(&archive);
    fs::set_permissions(&cache, fs::Permissions::from_mode(0o755)).expect("chmod");

    assert_no_panic(&out);
    assert!(
        out.status.success(),
        "an unusable cache stopped a run it should only have slowed:\n{}",
        stderr(&out)
    );
    assert!(sandbox.output().is_file());
}

/// A local archive is read, not fetched — the run must work with no network at
/// all, which is what `ARUNA_ZIP` is for and what every test here relies on.
#[test]
fn a_local_archive_run_makes_no_network_request() {
    let sandbox = Sandbox::new();
    // Proxy variables pointed at a port nothing listens on: any attempt to
    // reach out would fail loudly rather than quietly succeed.
    let out = Command::new(env!("CARGO_BIN_EXE_aruna"))
        .env("HOME", sandbox.path())
        .env("ARUNA_ZIP", sandbox.corpus())
        .env("ARUNA_CACHE_DIR", sandbox.path().join("cache"))
        .env("http_proxy", "http://127.0.0.1:1")
        .env("https_proxy", "http://127.0.0.1:1")
        .env("ALL_PROXY", "socks5://127.0.0.1:1")
        .stdin(Stdio::null())
        .output()
        .expect("runs");

    assert_no_panic(&out);
    assert!(
        out.status.success(),
        "a local archive should need no network:\n{}",
        stderr(&out)
    );
    assert!(sandbox.output().is_file());
}

/// An inventory already in place survives a failed run.
///
/// The write is atomic — scratch file, flush, rename — so a run that fails
/// must leave the previous inventory exactly as it was rather than truncated
/// or half-replaced.
#[test]
fn a_failed_run_leaves_an_existing_inventory_untouched() {
    let sandbox = Sandbox::new();
    assert!(sandbox.run(&sandbox.corpus()).status.success());
    let good = fs::read(sandbox.output()).expect("read");

    let failed = sandbox.run(&sandbox.path().join("not-here.zip"));
    assert_no_panic(&failed);
    assert!(!failed.status.success());

    assert_eq!(
        fs::read(sandbox.output()).expect("read"),
        good,
        "a failed run damaged the inventory from the successful one"
    );
    assert!(leftovers(&sandbox.downloads()).is_empty());
}

/// Interrupting a run must not leave a truncated inventory where a complete one
/// is expected. The scratch-then-rename write is what guarantees it: until the
/// rename there is nothing at the destination to damage.
#[cfg(unix)]
#[test]
fn an_interrupted_run_leaves_no_half_written_inventory() {
    use std::io::Read as _;

    let sandbox = Sandbox::new();
    // Enough manuscripts that the run is still working when the signal lands.
    let entries: Vec<(String, String)> = (0..4000)
        .map(|i| {
            (
                format!("root/CTH {}_XML_HFR/KBo {i}.xml", i % 50),
                manuscript(&format!("KBo {i}")),
            )
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = entries
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let archive = sandbox.archive(&borrowed);

    let mut child = Command::new(env!("CARGO_BIN_EXE_aruna"))
        .env("HOME", sandbox.path())
        .env("ARUNA_ZIP", &archive)
        .env("ARUNA_CACHE_DIR", sandbox.path().join("cache"))
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");

    // Wait for the run to actually start working rather than sleeping blind:
    // the program says so on stderr before it reads anything.
    let mut stderr_pipe = child.stderr.take().expect("stderr");
    let mut seen = Vec::new();
    let mut byte = [0u8; 1];
    while !String::from_utf8_lossy(&seen).contains("Reading headers") {
        match stderr_pipe.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => seen.extend_from_slice(&byte),
            Err(_) => break,
        }
    }

    unsafe {
        libc_kill(child.id() as i32, 15); // SIGTERM
    }
    let status = child.wait().expect("wait");

    // **The exit code is not the property, and racing for it is how this test
    // used to be written.** A four-document archive can finish between the
    // signal being sent and the process reading it, and after 2.3.0 — with the
    // standalone inventory's phase gone — it usually does. What has to hold
    // either way is on disk: whatever the run managed, the destination holds a
    // complete document or nothing, never a fragment.
    let _ = status;

    if sandbox.output().exists() {
        let html = fs::read_to_string(sandbox.output()).expect("read");
        assert!(
            html.trim_end().ends_with("</html>"),
            "the destination holds a truncated inventory"
        );
    }
}

#[cfg(unix)]
unsafe fn libc_kill(pid: i32, sig: i32) {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    unsafe { kill(pid, sig) };
}

/// Two runs at once, sharing a home and a cache.
///
/// Nothing stops a person starting the program twice, and both write the same
/// destination. The scratch files carry the process id for exactly this reason;
/// what that has to buy is two complete inventories rather than one truncated
/// one, and nothing left over.
#[test]
fn two_runs_at_once_do_not_interfere() {
    let sandbox = Sandbox::new();
    let archive = sandbox.corpus();

    let spawn = || {
        Command::new(env!("CARGO_BIN_EXE_aruna"))
            .env("HOME", sandbox.path())
            .env("ARUNA_ZIP", &archive)
            .env("ARUNA_CACHE_DIR", sandbox.path().join("cache"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn")
    };

    let (first, second) = (spawn(), spawn());
    let a = first.wait_with_output().expect("wait");
    let b = second.wait_with_output().expect("wait");

    // **What two simultaneous runs guarantee, and what they do not.** Both
    // stage under their own name, so neither destroys the other's work — that
    // was measured, and with a shared staging name both used to fail leaving
    // no package at all. What is still not serialised is publishing: one run
    // can replace the package while the other is verifying the copy it just
    // published, and that one then reports a validation failure. Measured on
    // the real corpus: the package on disk was complete and valid every time,
    // and at least one run always succeeded.
    for out in [&a, &b] {
        assert_no_panic(out);
    }
    assert!(
        a.status.success() || b.status.success(),
        "neither of two concurrent runs succeeded:\n{}\n{}",
        stderr(&a),
        stderr(&b)
    );

    let html = fs::read_to_string(sandbox.output()).expect("read");
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(
        html.trim_end().ends_with("</html>"),
        "the winner is truncated"
    );
    assert!(
        leftovers(&sandbox.downloads()).is_empty(),
        "concurrent runs left scratch files behind: {:?}",
        leftovers(&sandbox.downloads())
    );
}

/// The destination is a directory, not a file.
///
/// A rename onto a directory fails, and the finished inventory must not be
/// thrown away because of it — the error names where it was kept.
#[test]
fn a_directory_where_the_inventory_belongs_is_reported_not_forced() {
    let sandbox = Sandbox::new();
    fs::create_dir_all(sandbox.output()).expect("mkdir in place of the file");

    let out = sandbox.run(&sandbox.corpus());
    assert_no_panic(&out);
    assert!(
        !out.status.success(),
        "writing over a directory reported success"
    );
    assert!(
        sandbox.output().is_dir(),
        "the directory that was in the way is gone"
    );
    let err = stderr(&out);
    assert!(err.contains("Ошибка"), "no diagnosis: {err}");
}

/// Ten runs in a row leave exactly what one run leaves.
///
/// A soak in miniature: the check is not speed but accumulation — scratch
/// files, stale archives in the cache, a second inventory under another name.
/// Bounded on purpose, so it belongs in the ordinary suite.
#[test]
fn repeated_runs_do_not_accumulate_anything() {
    let sandbox = Sandbox::new();
    let archive = sandbox.corpus();
    let cache = sandbox.path().join("cache");

    for run in 1..=10 {
        let out = sandbox.run(&archive);
        assert_no_panic(&out);
        assert!(out.status.success(), "run {run} failed:\n{}", stderr(&out));

        assert_eq!(
            fs::read_dir(sandbox.downloads()).expect("read").count(),
            1,
            "after run {run} the Downloads folder holds more than the package"
        );
        assert!(leftovers(&sandbox.downloads()).is_empty(), "run {run}");
        let cached = fs::read_dir(&cache).map(|d| d.count()).unwrap_or(0);
        assert!(
            cached <= 1,
            "after run {run} the cache holds {cached} entries"
        );
    }
}
