//! One run of one operation: who is listening, and how it is stopped.
//!
//! Until now a long operation took a `&dyn Progress` and nothing else. That was
//! enough for a program with one front end and a terminal in front of it: the
//! only way to stop a run was Ctrl-C, and Ctrl-C is a fine answer when the
//! process exists to do this one thing.
//!
//! It is not an answer for a window. A download is about a minute and a package
//! build about six seconds; a person who clicks *Cancel* is asking for the work
//! to stop, not for the application to die, and the two are different requests.
//! An application that offers no third option offers *quit*.
//!
//! So the pair travels together. [`Job`] carries what a run needs from its
//! caller — where to report, whether to keep going, and which run this is —
//! and it is passed as one parameter rather than as three that could be got out
//! of step.
//!
//! ```text
//!   caller                     core
//!     │                          │
//!     ├── Job { id, progress, cancel } ──►  loop {
//!     │                          │             job.check()?      // stop here
//!     │                          │             …one document…
//!     ├◄── Event ────────────────┤             job.report(…)
//!     │                          │          }
//!     └── cancel.cancel() ──────►│
//! ```
//!
//! **Nothing here knows about Tauri, or about a window at all.** `Cancel` is an
//! atomic flag and a handle; a Tauri command holds the handle in its state and
//! sets it when the frontend asks. A CLI holds one it never sets. A test holds
//! one it sets after two documents to prove that the third is not written.

use crate::error::{ArunaError, Result};
use crate::progress::{Event, Progress};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Which run this is.
///
/// Small and copyable, because it travels in every progress event and every
/// error: a window running two conversions has to be able to tell one's
/// progress from the other's, and an event that arrives after its job was
/// cancelled has to be recognisable as stale rather than acted on.
///
/// Allocated from a counter rather than from a clock or a random source: two
/// ids from the same process are distinct, which is the whole requirement, and
/// a counter is reproducible in a test where a timestamp is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobId(u64);

impl JobId {
    /// The next id this process will hand out.
    pub fn next() -> JobId {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        JobId(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    /// The number, for a caller that has to put it in a message.
    pub fn get(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Which part of a run something happened in.
///
/// One name per stage that can fail or be stopped, so an error and a
/// cancellation can both say *where* without the caller parsing prose. A window
/// shows it, a log records it, and a machine-readable error carries it.
///
/// Deliberately coarse. These are the stages a person watching a progress bar
/// can distinguish; a finer list would be a description of the code rather than
/// of what the program is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Deciding between the cache and the network, and fetching if it must.
    Obtaining,
    /// Reading manuscripts out of the archive.
    Parsing,
    /// Writing the package.
    Exporting,
    /// Checking what was written against the model it came from.
    Validating,
    /// Putting the finished work in place.
    Publishing,
}

impl Phase {
    /// The stable name a machine reads.
    ///
    /// Not `Debug`: this crosses a process boundary and appears in a contract,
    /// so it is written out rather than derived from a Rust type name that
    /// could be changed by a rename.
    pub fn code(self) -> &'static str {
        match self {
            Phase::Obtaining => "obtaining",
            Phase::Parsing => "parsing",
            Phase::Exporting => "exporting",
            Phase::Validating => "validating",
            Phase::Publishing => "publishing",
        }
    }
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

/// A flag one side sets and the other reads.
///
/// Cloning shares the flag — that is the point. The caller keeps a handle and
/// the work keeps a handle, and neither has to know where the other lives.
///
/// `Relaxed` throughout, and that is not a shortcut. The flag guards no data:
/// nothing is published through it, and the only question ever asked is whether
/// it has been set. A stronger ordering would buy an ordering guarantee against
/// other memory that no reader here depends on. What matters is that the store
/// becomes visible, and it does — the loops below read it every iteration.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    /// A run that has not been asked to stop.
    pub fn new() -> Cancel {
        Cancel::default()
    }

    /// Ask the run to stop at its next opportunity.
    ///
    /// Idempotent, and safe from any thread. It does not interrupt a syscall in
    /// flight: a document already being written finishes being written, and the
    /// loop stops before the next one. That is the difference between stopping
    /// and killing, and it is why a cancelled build leaves no half-written
    /// document behind.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether the run has been asked to stop.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// What a run needs from whoever started it.
///
/// Borrowed rather than owned: it lives for the call and the caller owns both
/// halves. A long operation takes one of these instead of a progress sink and a
/// cancellation flag as separate parameters — three things that have to agree
/// travel better as one.
pub struct Job<'a> {
    id: JobId,
    progress: &'a dyn Progress,
    cancel: &'a Cancel,
}

impl<'a> Job<'a> {
    /// A run that reports to `progress` and stops when `cancel` says so.
    pub fn new(progress: &'a dyn Progress, cancel: &'a Cancel) -> Job<'a> {
        Job {
            id: JobId::next(),
            progress,
            cancel,
        }
    }

    /// A run under an id the caller already has.
    ///
    /// For a caller that handed the id out before the work started — a window
    /// that has to be able to cancel a job it has not yet heard from.
    pub fn with_id(id: JobId, progress: &'a dyn Progress, cancel: &'a Cancel) -> Job<'a> {
        Job {
            id,
            progress,
            cancel,
        }
    }

    /// Which run this is.
    pub fn id(&self) -> JobId {
        self.id
    }

    /// Say what is happening.
    pub fn report(&self, event: Event<'_>) {
        self.progress.report(event);
    }

    /// The progress sink, for a call that still takes one.
    pub fn progress(&self) -> &'a dyn Progress {
        self.progress
    }

    /// Whether this run has been asked to stop.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// A run nobody is watching and nobody will stop.
    ///
    /// For tests, for examples, and for a caller that genuinely has neither — a
    /// benchmark measuring the parse does not want a progress line per stage
    /// and has nobody to cancel on its behalf.
    ///
    /// `'static` on purpose: it borrows a flag that lives for the process, so
    /// it can be written inline at a call site instead of forcing every test to
    /// keep a binding alive beside the call it is making. The flag is shared by
    /// every unattended job and is never set — there is no handle to set it
    /// with, which is what "unattended" means.
    pub fn unattended() -> Job<'static> {
        static NEVER: std::sync::OnceLock<Cancel> = std::sync::OnceLock::new();
        Job {
            id: JobId::next(),
            progress: &crate::progress::Silent,
            cancel: NEVER.get_or_init(Cancel::new),
        }
    }

    /// Stop here if the run has been cancelled.
    ///
    /// The one line a loop adds. Placed at a boundary where stopping leaves
    /// something coherent behind — between documents, between entries, between
    /// chunks — never in the middle of writing one.
    pub fn check(&self, phase: Phase) -> Result<()> {
        if self.cancel.is_cancelled() {
            return Err(ArunaError::Cancelled { phase });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::{Progress, Silent};

    #[test]
    fn a_fresh_run_is_not_cancelled_and_a_cancelled_one_stays_cancelled() {
        let cancel = Cancel::new();
        let job = Job::new(&Silent, &cancel);
        assert!(!job.is_cancelled());
        assert!(job.check(Phase::Parsing).is_ok());

        cancel.cancel();
        assert!(job.is_cancelled());
        assert!(matches!(
            job.check(Phase::Parsing),
            Err(ArunaError::Cancelled {
                phase: Phase::Parsing
            })
        ));
        // Idempotent: asking twice is asking once.
        cancel.cancel();
        assert!(job.is_cancelled());
    }

    /// The flag is shared, not copied — which is the whole point of the handle.
    #[test]
    fn a_clone_of_a_handle_stops_the_same_run() {
        let cancel = Cancel::new();
        let held_elsewhere = cancel.clone();
        let job = Job::new(&Silent, &cancel);

        held_elsewhere.cancel();
        assert!(
            job.is_cancelled(),
            "the other handle did not reach this run"
        );
    }

    /// Cancelled from another thread, which is where the request really comes
    /// from: a window's event loop is never the thread doing the work.
    #[test]
    fn a_run_is_stopped_from_another_thread() {
        let cancel = Cancel::new();
        let from_the_window = cancel.clone();
        let stopper = std::thread::spawn(move || from_the_window.cancel());
        stopper.join().expect("no panic");
        assert!(Job::new(&Silent, &cancel).is_cancelled());
    }

    /// Two runs are told apart, which a window needs to attribute progress.
    #[test]
    fn every_job_has_an_id_of_its_own() {
        let cancel = Cancel::new();
        let first = Job::new(&Silent, &cancel).id();
        let second = Job::new(&Silent, &cancel).id();
        assert_ne!(first, second);
        assert!(second.get() > first.get(), "ids move forward");
    }

    /// An id handed out before the work starts is the id the work reports
    /// under — a window has to be able to cancel a job it has not heard from.
    #[test]
    fn a_job_can_be_given_an_id_that_was_handed_out_first() {
        let cancel = Cancel::new();
        let id = JobId::next();
        assert_eq!(Job::with_id(id, &Silent, &cancel).id(), id);
    }

    /// The phase names cross a process boundary, so they are written out rather
    /// than derived from Rust type names a rename could change.
    #[test]
    fn every_phase_has_a_stable_name() {
        let phases = [
            (Phase::Obtaining, "obtaining"),
            (Phase::Parsing, "parsing"),
            (Phase::Exporting, "exporting"),
            (Phase::Validating, "validating"),
            (Phase::Publishing, "publishing"),
        ];
        for (phase, code) in phases {
            assert_eq!(phase.code(), code);
            assert_eq!(phase.to_string(), code);
        }
        // Distinct, so an error cannot name two stages at once.
        let names: std::collections::BTreeSet<&str> =
            phases.iter().map(|(p, _)| p.code()).collect();
        assert_eq!(names.len(), phases.len());
    }

    /// A job is carried to the thread the work runs on, and the handle stays
    /// with the caller — so both halves have to cross a thread boundary.
    ///
    /// Stated as a compile-time assertion rather than left to be discovered:
    /// the day something in here stops being `Send` it stops being possible to
    /// build the corpus off the main thread, and the error would appear at the
    /// call site in the desktop crate rather than here.
    #[test]
    fn a_job_and_its_handle_cross_thread_boundaries() {
        fn shared<T: Send + Sync>() {}
        shared::<Cancel>();
        shared::<Job<'static>>();
        shared::<&'static dyn Progress>();
    }

    /// The sink is handed back as it was handed in.
    ///
    /// `progress()` is what a caller uses to pass the job's sink to something
    /// that still takes a `&dyn Progress` of its own. Nothing in this crate
    /// does yet — the accessor is here for the window — so this is what says it
    /// works: reporting through the job and through the sink it returns has to
    /// reach the same place.
    #[test]
    fn the_sink_a_job_carries_is_the_sink_it_was_given() {
        #[derive(Default)]
        struct Counting(std::sync::atomic::AtomicUsize);
        impl Progress for Counting {
            fn report(&self, _event: Event<'_>) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let sink = Counting::default();
        let cancel = Cancel::new();
        let job = Job::new(&sink, &cancel);

        job.report(Event::ParsingArchive);
        job.progress().report(Event::CheckingPackage);

        assert_eq!(
            sink.0.load(Ordering::Relaxed),
            2,
            "one of the two reports went somewhere else"
        );
    }

    #[test]
    fn an_unattended_run_reports_nowhere_and_is_never_stopped() {
        let job = Job::unattended();
        job.report(Event::ParsingArchive);
        assert!(!job.is_cancelled());
        assert!(job.check(Phase::Exporting).is_ok());
        // Still its own run: two unattended jobs are two jobs.
        assert_ne!(Job::unattended().id(), Job::unattended().id());
    }
}
