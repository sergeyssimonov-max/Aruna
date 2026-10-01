//! The world Typst compiles in: two files, `/main.typ` – the template – and
//! `/doc.json` – the document as data. Nothing else exists, and every path
//! Typst asks for is logged: the log is what shows that no document opened a
//! way to a bibliography style (the guard of the `quick-xml` exception,
//! `.cargo/audit.toml`).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

use super::fonts::Fonts;

/// The two paths that exist.
pub(crate) const MAIN: &str = "/main.typ";
pub(crate) const DATA: &str = "/doc.json";

pub(crate) struct PdfWorld<'f> {
    library: LazyHash<Library>,
    fonts: &'f Fonts,
    main_id: FileId,
    data_id: FileId,
    main: Source,
    data: Bytes,
    /// Every path Typst asked for, served or not.
    asked: Mutex<BTreeSet<String>>,
}

fn file_id(p: &'static str) -> Option<FileId> {
    Some(RootedPath::new(VirtualRoot::Project, VirtualPath::new(p).ok()?).intern())
}

impl<'f> PdfWorld<'f> {
    pub fn new(fonts: &'f Fonts, template: &str, json: String) -> Option<Self> {
        let main_id = file_id(MAIN)?;
        Some(PdfWorld {
            library: LazyHash::new(Library::default()),
            fonts,
            main_id,
            data_id: file_id(DATA)?,
            main: Source::new(main_id, template.to_string()),
            data: Bytes::new(json.into_bytes()),
            asked: Mutex::new(BTreeSet::new()),
        })
    }

    /// The log of paths. A poisoned log is still the log: a path is one
    /// insertion, done or not.
    fn log(&self) -> std::sync::MutexGuard<'_, BTreeSet<String>> {
        self.asked.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn ask(&self, id: FileId) {
        self.log()
            .insert(id.vpath().get_without_slash().to_string());
    }

    /// The paths asked for that are neither of the two: empty, always.
    pub(crate) fn strays(&self) -> Vec<String> {
        let known = [&MAIN[1..], &DATA[1..]];
        self.log()
            .iter()
            .filter(|p| !known.contains(&p.as_str()))
            .cloned()
            .collect()
    }

    /// Every path asked for.
    pub(crate) fn asked(&self) -> BTreeSet<String> {
        self.log().clone()
    }
}

impl World for PdfWorld<'_> {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &self.fonts.book
    }
    fn main(&self) -> FileId {
        self.main_id
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        self.ask(id);
        if id == self.main_id {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotFound(PathBuf::from(
                "only the template exists",
            )))
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.ask(id);
        if id == self.data_id {
            Ok(self.data.clone())
        } else {
            Err(FileError::NotFound(PathBuf::from("only doc.json exists")))
        }
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.fonts.get(index).cloned()
    }
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}
