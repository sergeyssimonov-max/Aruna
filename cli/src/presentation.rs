//! What a reader is shown, decided once and rendered many times.
//!
//! Between the records the archive yields and the documents that ship there is
//! a layer of small decisions: which of a manuscript's two names to print,
//! where a link points from the page it is written on, which facts a row
//! carries when some of them are missing. None of that is a property of a
//! manuscript, and none of it is a property of HTML — it is what this project
//! shows about the corpus, and it was being decided independently in each
//! renderer.
//!
//! That is how the two documents disagreed. The inventory prints the siglum
//! inside a group and the full title outside one; the group page — which the
//! CTH folders opened with until 2026-08-23 — printed `place.label`, which is
//! nearly but not exactly the same rule. Each was right for its own page and
//! neither could be checked against the other, because there was nothing for
//! them to be checked against.
//!
//! **The group pages are gone**, by decision rather than by simplification: the
//! inventory now links each manuscript straight at its XML file and a CTH label
//! is plain text. So there is one document doing the linking, and one href per
//! manuscript — but the layer stays, because the naming rule it settled is still
//! shared with the manifest and with whatever renders next.
//!
//! ```text
//!   [ManuscriptRecord] + [Placed]
//!               │
//!               ▼
//!        CorpusPresentation ─┬─► the inventory
//!                            ├─► a future PDF
//!                            └─► a future DTO ──► Tauri ──► TypeScript
//! ```
//!
//! **Borrowed, not owned.** The corpus is 23 936 records and every string in
//! them is already in memory; a presentation that cloned them would allocate
//! tens of megabytes to say the same thing twice. The hrefs are the exception —
//! they do not exist until they are computed — and they are computed once here
//! rather than once per renderer.
//!
//! **Not serialisable, on purpose.** A DTO for Tauri is an owning type built
//! from this one by copying, and it belongs in its own module where `serde` can
//! be derived without spreading inward — see `docs/FRONTEND-CONTRACT.md` §2.1.
//! Lifetimes are right for HTML and for PDF, which run in the same pass over
//! the same data; they are wrong for a value that has to outlive the call and
//! cross a process boundary, and pretending otherwise here would put `serde` in
//! the parser.
//!
//! **What it does not decide.** Escaping, which is the renderer's and differs
//! by context — text, attribute and URL are three questions. Styling. Counts
//! that are a `len()`. This layer answers what to show, not how to write it
//! down.

use crate::export::naming::href;
use crate::export::Placed;
use crate::parse::{group_label, group_runs, ManuscriptRecord, MISSING};

/// The spellings the corpus uses for one and the same editor.
///
/// TLHdig records who made an edition in whatever form the file's author typed:
/// initials in most documents, a full name in the newer ones. A reader looking
/// for a surname would otherwise miss every row that carries only initials —
/// `schwemer` finds 84 manuscripts and not the 7 that say `DS`.
///
/// **This is a fact about the corpus, so it lives here.** It was written in the
/// client script that searches the inventory, and in the website's copy beside
/// it while the website existed; an agreement test held the pair together, and
/// when the site went in 2.1.0 the pair became one copy in a `.ts` file with
/// nothing holding it to anything. Which spellings name the same scholar is not
/// a decision about how a table behaves, and the crate already knows the
/// spelling — it renders the cell. So the crate says it, once, and a renderer
/// carries it into the document for the search to find.
///
/// Written in the corpus's own casing. What a spelling changes is which rows a
/// query reaches, never what a row displays: the cell keeps what the document
/// wrote.
///
/// **How complete this is, measured rather than assumed.** The corpus's editor
/// column holds 46 distinct values across 23 936 manuscripts (2026-09-06,
/// counted off a built package). The three groups below reach 121 of them:
/// `DS` 4, `ds` 3 and `Daniel Schwemer` 84; `FF` 9 and `Francesco Fuscagni` 18;
/// `Andrey Shatskov` 28 and `Andrei Shatskov` 3 — one name, transliterated two
/// ways, which a search for either spelling would otherwise miss.
///
/// What is left open, and deliberately not guessed: several sets of initials
/// could belong to full names that are also in the column — `JB` beside
/// `James Burgin`, `TS` beside `TurnaSomel`. Which initials stand for whom is a
/// statement about people, not about strings, and this file has no way to check
/// one. A wrong pair here would quietly merge two scholars, which is worse than
/// a search that misses rows, so an addition wants someone who knows the field.
pub const EDITOR_ALIASES: [&[&str]; 3] = [
    &["DS", "Daniel Schwemer"],
    &["FF", "Francesco Fuscagni"],
    &["Andrey Shatskov", "Andrei Shatskov"],
];

/// The other ways this corpus spells the same editor, empty when there are none.
///
/// Matching ignores case and surrounding space, because the archive's spelling
/// is what a document happened to type. The spelling given is never returned:
/// the row already carries it.
pub fn other_spellings(editor: &str) -> Vec<&'static str> {
    let editor = editor.trim();
    if editor.is_empty() {
        return Vec::new();
    }
    EDITOR_ALIASES
        .iter()
        .find(|group| group.iter().any(|one| one.eq_ignore_ascii_case(editor)))
        .map(|group| {
            group
                .iter()
                .copied()
                .filter(|one| !one.eq_ignore_ascii_case(editor))
                .collect()
        })
        .unwrap_or_default()
}

/// The corpus as a document shows it: groups, in the order they are listed.
///
/// Built once per run and read by every renderer. The order is the records'
/// own — [`crate::order::sort_records`] put them in it and both documents list
/// manuscripts that way, so a presentation that re-sorted would be a third
/// opinion about the corpus.
#[derive(Debug)]
pub struct CorpusPresentation<'a> {
    /// Where the corpus came from, as the attribution line prints it.
    pub source: &'a str,
    /// The CTH groups, in listing order.
    pub groups: Vec<GroupPresentation<'a>>,
}

/// One CTH group, and the manuscripts filed under it.
#[derive(Debug)]
pub struct GroupPresentation<'a> {
    /// The label as a reader sees it: `CTH 5`, or the dash for no group.
    pub label: &'a str,
    pub fragments: Vec<FragmentPresentation<'a>>,
}

/// One manuscript, as a line in a listing.
#[derive(Debug)]
pub struct FragmentPresentation<'a> {
    /// The name to print.
    ///
    /// Inside a group that is the siglum — the heading has already said the CTH
    /// and repeating it on every row is noise. Outside one it is the full
    /// title, which is the only thing that identifies the manuscript. This
    /// branch used to live in the inventory's row writer and, in a slightly
    /// different form, in the group page's; one of them was going to drift.
    pub display_name: &'a str,
    /// This manuscript's file, relative to the package root:
    /// `./CTH%205/KBo%201.1.xml`.
    ///
    /// `None` when the corpus is being shown without a folder behind it — the
    /// CLI's standalone inventory has no files to link to, and a link that
    /// resolved to nothing would be worse than plain text.
    ///
    /// One href, not two: until 2026-08-23 there was a second, relative to the
    /// group's own folder, for the page that folder opened with. There are no
    /// group pages any more, so the inventory is the only document that links
    /// and the package root is the only place a link is written from.
    pub href: Option<String>,
    /// The record itself, for the fields a renderer lays out in columns.
    pub record: &'a ManuscriptRecord,
}

impl<'a> FragmentPresentation<'a> {
    /// The facts worth printing beside the name, in display order.
    ///
    /// Only what this manuscript actually carries: a record with no editor gets
    /// no editor rather than a dash standing in for one. The fields are read
    /// out of an archive and some of them are genuinely absent.
    pub fn facts(&self) -> Vec<&'a str> {
        [
            self.record.lang.as_str(),
            self.record.corpus.as_str(),
            self.record.authorship.as_str(),
            self.record.year.as_str(),
        ]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect()
    }
}

impl<'a> CorpusPresentation<'a> {
    /// The corpus as a package shows it: every group has a page, every
    /// manuscript a file.
    ///
    /// `placed` is parallel to `records` — the exporter builds it that way and
    /// `crate::export::group_slices` relies on the same invariant.
    pub fn linked(records: &'a [ManuscriptRecord], placed: &'a [Placed], source: &'a str) -> Self {
        Self::build(records, Some(placed), source)
    }

    /// The corpus as the standalone inventory shows it: no folder, no links.
    pub fn plain(records: &'a [ManuscriptRecord], source: &'a str) -> Self {
        Self::build(records, None, source)
    }

    fn build(
        records: &'a [ManuscriptRecord],
        placed: Option<&'a [Placed]>,
        source: &'a str,
    ) -> Self {
        let mut groups = Vec::new();
        let mut from = 0usize;

        for run in group_runs(records) {
            let label = group_label(&run[0]);
            let slice = placed.map(|all| &all[from..from + run.len()]);
            from += run.len();

            let fragments = run
                .iter()
                .enumerate()
                .map(|(i, record)| {
                    let place = slice.map(|s| &s[i]);
                    FragmentPresentation {
                        display_name: display_name(record, place),
                        href: place.map(|p| href(&p.relative)),
                        record,
                    }
                })
                .collect();

            groups.push(GroupPresentation { label, fragments });
        }

        CorpusPresentation { source, groups }
    }

    /// How many manuscripts the corpus holds.
    pub fn manuscripts(&self) -> usize {
        self.groups.iter().map(|g| g.fragments.len()).sum()
    }
}

/// The name a manuscript is listed under.
///
/// Inside a CTH group the siglum identifies it and the group heading has
/// already said the rest; with no group, or with no siglum, the full title is
/// all there is. When the manuscript has been placed, the placement's label is
/// preferred — it is the siglum as a reader writes it even where the file name
/// had to escape it.
fn display_name<'a>(record: &'a ManuscriptRecord, place: Option<&'a Placed>) -> &'a str {
    if record.cth.is_some() && record.sigla != MISSING {
        return place.map_or(record.sigla.as_str(), |p| p.label.as_str());
    }
    record.title.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::place;
    use crate::export::tests_support::fragment;

    /// Records and their placements, from the same fixtures the exporter uses.
    fn corpus() -> (Vec<ManuscriptRecord>, Vec<Placed>) {
        let fragments = [
            fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/a.xml"),
            fragment("544/f", "CTH 5", "root/CTH 5_XML_HFR/b.xml"),
            fragment("KUB 2.1", "CTH 9", "root/CTH 9_XML_TLH/c.xml"),
        ];
        let placed = place(&fragments).expect("placed");
        let records = fragments.iter().map(|f| f.record.clone()).collect();
        (records, placed)
    }

    #[test]
    fn the_groups_are_the_record_runs_in_record_order() {
        let (records, placed) = corpus();
        let corpus = CorpusPresentation::linked(&records, &placed, "test");

        let labels: Vec<&str> = corpus.groups.iter().map(|g| g.label).collect();
        assert_eq!(labels, ["CTH 5", "CTH 9"]);
        assert_eq!(corpus.groups[0].fragments.len(), 2);
        assert_eq!(corpus.manuscripts(), records.len());
    }

    /// A link names the manuscript's own file, from the package root.
    ///
    /// It carries the group's folder because that is where the file is, not
    /// because the folder is a destination: nothing links to a folder any more.
    #[test]
    fn a_link_names_the_manuscript_s_file_from_the_package_root() {
        let (records, placed) = corpus();
        let corpus = CorpusPresentation::linked(&records, &placed, "test");
        let href = corpus.groups[0].fragments[0]
            .href
            .as_deref()
            .expect("a linked fragment");

        assert_eq!(href, "./CTH%205/KBo%201.1.xml");
        assert!(
            href.ends_with(".xml"),
            "a link points at the fragment itself, never at a page about it"
        );
    }

    /// A separator inside a siglum is escaped in the file name and escaped
    /// again in the link that names it — which is one encoding of a name that
    /// already contains a percent sign, not two of the same thing.
    #[test]
    fn a_siglum_holding_a_separator_survives_both_hrefs() {
        let (records, placed) = corpus();
        let corpus = CorpusPresentation::linked(&records, &placed, "test");
        let slash = corpus.groups[0]
            .fragments
            .iter()
            .find(|f| f.display_name == "544/f")
            .expect("the siglum with a slash");
        let href = slash.href.as_deref().expect("linked");

        assert_eq!(href, "./CTH%205/544%252Ff.xml");
        // And the name a reader sees is not the escaped one.
        assert_eq!(slash.display_name, "544/f");
    }

    /// Inside a group the siglum names a manuscript; the group heading has
    /// already said the CTH.
    #[test]
    fn a_grouped_manuscript_is_named_by_its_siglum() {
        let (records, placed) = corpus();
        let corpus = CorpusPresentation::linked(&records, &placed, "test");
        assert_eq!(corpus.groups[0].fragments[0].display_name, "KBo 1.1");
        assert!(
            !corpus.groups[0].fragments[0].display_name.contains("CTH"),
            "the row repeats what its heading already says"
        );
    }

    /// Without a group there is no heading to carry the CTH, so the full title
    /// is what identifies the manuscript.
    #[test]
    fn an_ungrouped_manuscript_is_named_by_its_title() {
        let mut records = corpus().0;
        for record in &mut records {
            record.cth = None;
        }
        let corpus = CorpusPresentation::plain(&records, "test");
        assert_eq!(corpus.groups.len(), 1, "one run, under the dash");
        assert_eq!(corpus.groups[0].label, MISSING);
        assert_eq!(corpus.groups[0].fragments[0].display_name, records[0].title);
    }

    /// A corpus with no folder behind it carries no links.
    ///
    /// The CLI's standalone inventory is one file in `~/Downloads`; a link to
    /// `./CTH 5/KBo 1.1.xml` from there resolves to nothing.
    #[test]
    fn an_unlinked_corpus_offers_no_hrefs_at_all() {
        let records = corpus().0;
        let corpus = CorpusPresentation::plain(&records, "test");
        for group in &corpus.groups {
            assert!(group.fragments.iter().all(|f| f.href.is_none()));
        }
    }

    /// Only the facts a manuscript carries.
    #[test]
    fn the_facts_are_the_ones_the_record_holds() {
        let (mut records, placed) = corpus();
        records[0].authorship.clear();
        records[0].year = "   ".to_string();
        let corpus = CorpusPresentation::linked(&records, &placed, "test");

        let facts = corpus.groups[0].fragments[0].facts();
        assert!(
            !facts.iter().any(|f| f.trim().is_empty()),
            "a blank stood in for a fact that is not there: {facts:?}"
        );
        assert_eq!(
            facts,
            [records[0].lang.as_str(), records[0].corpus.as_str()]
        );
    }

    /// Nothing is copied.
    ///
    /// Not a style preference: the corpus is 23 936 records and every string in
    /// them is already in memory. A presentation that cloned them would
    /// allocate tens of megabytes to say the same thing a second time, and this
    /// layer is built once per run over the whole corpus.
    #[test]
    fn the_presentation_borrows_the_records_it_describes() {
        let (records, placed) = corpus();
        let corpus = CorpusPresentation::linked(&records, &placed, "test");

        assert!(std::ptr::eq(
            corpus.groups[0].fragments[0].record,
            &records[0]
        ));
        assert!(std::ptr::eq(
            corpus.groups[0].fragments[0].display_name.as_ptr(),
            placed[0].label.as_ptr()
        ));
    }

    /// The renderers no longer decide anything about the corpus.
    ///
    /// The point of this module, stated as a property of the source rather than
    /// of one document's output. Each of these was a decision a renderer used
    /// to make on its own, and each is the kind that fails quietly: two pages
    /// that name a manuscript differently, or link it to two different places,
    /// both render.
    ///
    /// Reading the source is a blunt instrument and it is the right one here —
    /// the alternative is asserting on rendered HTML, which cannot tell a
    /// decision that moved from a decision that was duplicated. `#[cfg(test)]`
    /// blocks are excluded: a test may compose whatever it needs.
    #[test]
    fn the_renderers_take_their_decisions_from_here() {
        let renderers = [("html.rs", include_str!("html.rs"))];

        for (name, source) in renderers {
            let code = source
                .split("#[cfg(test)]")
                .next()
                .expect("a module has a body");

            for (decision, marker) in [
                ("which name to show", "cth.is_some()"),
                ("which name to show", "!= MISSING"),
                ("where a link points", "href(&"),
                ("how the records are grouped", "group_runs("),
            ] {
                assert!(
                    !code.contains(marker),
                    "{name} decides {decision} itself (`{marker}`); \
                     that belongs to the presentation both documents share"
                );
            }
        }
    }

    /// **A spelling is matched however the document typed it.**
    ///
    /// The archive writes `DS` in most files and `Daniel Schwemer` in the newer
    /// ones, and either of them has to reach the other. Case and surrounding
    /// space are the document's business, not the list's.
    #[test]
    fn an_editor_is_found_by_any_spelling_the_corpus_uses() {
        assert_eq!(other_spellings("DS"), ["Daniel Schwemer"]);
        assert_eq!(other_spellings("ds"), ["Daniel Schwemer"]);
        assert_eq!(other_spellings(" Daniel Schwemer "), ["DS"]);
        assert_eq!(other_spellings("FF"), ["Francesco Fuscagni"]);
    }

    /// **A pair may be two spellings of one name, not initials and a name.**
    ///
    /// `Andrey Shatskov` is written 28 times and `Andrei Shatskov` three; a
    /// search for either missed the other until 2026-09-06. Nothing about the
    /// list says a group has to pair initials with a name, and this group is
    /// what stops a reader from assuming it does.
    #[test]
    fn a_name_transliterated_two_ways_is_one_editor() {
        assert_eq!(other_spellings("Andrey Shatskov"), ["Andrei Shatskov"]);
        assert_eq!(other_spellings("andrei shatskov"), ["Andrey Shatskov"]);
    }

    /// An editor the corpus spells one way only, and no editor at all, add
    /// nothing: the list says which names are one person, not that every name
    /// has a second form.
    #[test]
    fn an_editor_with_one_spelling_gets_nothing_added() {
        assert!(other_spellings("AA").is_empty());
        assert!(other_spellings("").is_empty());
        assert!(other_spellings("   ").is_empty());
    }

    /// No spelling belongs to two people: a name that reached two groups would
    /// make the answer depend on which was written first.
    #[test]
    fn no_spelling_names_two_editors() {
        let mut seen: Vec<String> = Vec::new();
        for group in EDITOR_ALIASES {
            for one in group {
                let one = one.to_ascii_lowercase();
                assert!(!seen.contains(&one), "the spelling {one} is in two groups");
                seen.push(one);
            }
        }
    }
}
