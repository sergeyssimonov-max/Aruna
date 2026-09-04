//! The one place that decides what CSS a generated document carries.
//!
//! Every page this program writes is self-contained: its styles are inside it,
//! in a `<style>` element, and the package holds no `.css` file at all. That is
//! a property of the *output*, not of the source — nothing else in the crate is
//! allowed to write a `<style>`.
//!
//! ```text
//!   frontend/src/inventory/canonical.css ─┬─► inventory_css() ─► TLHdig_Beta_0.3.html
//!   frontend/src/inventory/screen.css     ┤
//!   frontend/src/inventory/print.css     ─┘
//! ```
//!
//! Three sections in that order, always: what the document is, what this page
//! is, and how it prints. Print comes last because it is the only section
//! written to override — the page's own screen rules are what it has to undo —
//! and a rule that has to win is easier to read at the end than dressed in
//! specificity in the middle. **The order is the whole of the cascade here, and
//! it is decided in [`join`] and nowhere else**, which is why the sections stay
//! three files rather than being bundled into one.
//!
//! **The sources are not here.** They live in `frontend/src/inventory/`, and
//! Vite builds each section into `generated/` — lowered to the Safari 16 floor
//! the desktop window targets, with the maintainer's comments dropped on the
//! way. `cargo build` never runs any of that: the built sections are committed,
//! and `frontend/tests/inventory-artifact.test.ts` fails if they are not what
//! the sources now produce. See `docs/FRONTEND-CONTRACT.md`, *The target state*.
//!
//! **There used to be a second page.** Each CTH folder opened with an
//! `index.html` assembled the same way, from `style_group.css`; that is why the
//! shared section exists at all — the two documents had drifted, one turning in
//! dark mode while the other stayed light. The pages were given up on
//! 2026-08-23 and the group stylesheet with them. The split is kept because the
//! shared/print seam is what a future PDF joins, and because collapsing it now
//! would bury the reason the tokens live where they do.

/// Tokens and the base document: what both pages are before either adds
/// anything.
///
/// The three sections are private, and the module is the boundary: the only
/// thing outside it that has any business knowing about CSS is a rendered
/// document, which gets it through [`inventory_css`]. This one was public with
/// the reason "so a test can assert that a rendered document really contains
/// it" — the tests that do so are in this file, where privacy is no obstacle,
/// and nothing outside the crate ever named it.
const SHARED: &str = include_str!("generated/canonical.css");

/// How either document prints. Shared, and emitted last; see the module note.
const PRINT: &str = include_str!("generated/print.css");

/// The inventory's own section: the legend, the toolbar, the table, the groups.
const INVENTORY: &str = include_str!("generated/screen.css");

/// The shared section exactly as a document carries it.
///
/// The trim is the whole difference from [`SHARED`]: the sections are joined
/// with exactly one newline between them, and a built file that ends in a blank
/// line would put two there.
///
/// Private and built only for tests since 2026-09-03, for the reason the module
/// note already gives about [`SHARED`]: the only callers are the assertions
/// below, `join` reads the constants directly, and nothing outside the crate
/// ever named it. `cfg(test)` rather than a bare `fn`, because a private
/// function no release build calls is dead code, and silencing that would say
/// less than this line does.
#[cfg(test)]
fn shared_section() -> String {
    SHARED.trim_matches('\n').to_string()
}

/// The print section exactly as a document carries it. Private and test-only
/// for the same reason as [`shared_section`].
#[cfg(test)]
fn print_section() -> String {
    PRINT.trim_matches('\n').to_string()
}

/// The stylesheet for the inventory: shared, then its own, then print.
pub fn inventory_css() -> String {
    join(INVENTORY)
}

/// The three sections in the one order they are ever emitted in.
///
/// Order is the whole of the cascade here, and it is decided in this function
/// and nowhere else — which is the point of having the function at all.
///
/// The comments were dropped on the way out by [`strip_comments`] until
/// 2026-08-23; Lightning CSS does it now, in the build, and it parses the
/// stylesheet rather than scanning it for `/*`. Nothing else is minified: the
/// rules stay on their own lines and indented, because someone reading the
/// source of a document should be able to read it.
fn join(page: &str) -> String {
    let mut css = String::with_capacity(SHARED.len() + page.len() + PRINT.len() + 2);
    for section in [SHARED, page, PRINT] {
        css.push_str(section.trim_matches('\n'));
        css.push('\n');
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stylesheet carries the shared section, whole and unaltered, and the
    /// print section last.
    ///
    /// It held two documents to one design until the CTH pages were given up on
    /// 2026-08-23. It still fixes the order, which is the whole of the cascade
    /// here, and it is what a second document would be joined to.
    #[test]
    fn every_page_is_built_from_the_one_shared_section() {
        let css = inventory_css();
        assert!(
            css.starts_with(&shared_section()),
            "the stylesheet does not open with the shared section, whole"
        );
        assert!(
            css.trim_end().ends_with(&print_section()),
            "the print section is not last, so the page's own rules could win over it"
        );
    }

    /// Nothing but the print section speaks about paper.
    ///
    /// A `@media print` block inside a page's own section would be a second
    /// place deciding how the corpus prints, which is the arrangement this
    /// module replaced.
    #[test]
    fn only_the_print_section_speaks_about_paper() {
        assert!(PRINT.contains("@media print"));
        for (name, section) in [("shared", SHARED), ("inventory", INVENTORY)] {
            assert!(
                !section.contains("@media print") && !section.contains("@page"),
                "the {name} section carries print rules of its own"
            );
        }
    }

    /// The tokens are declared once, in the shared section, and nowhere else.
    ///
    /// A page section that opened its own `:root` would be redefining the design
    /// for itself, which is the drift this module exists to prevent.
    #[test]
    fn only_the_shared_section_declares_the_design_tokens() {
        assert!(SHARED.contains(":root {"));
        assert!(
            !INVENTORY.contains(":root"),
            "the inventory's section redeclares the tokens"
        );
    }

    /// Screen and print are separated logically inside one stylesheet, which is
    /// what makes the documents self-contained without a second file.
    #[test]
    fn both_stylesheets_carry_screen_and_print_rules() {
        let css = inventory_css();
        assert!(css.contains("@media print"), "no print rules");
        assert!(css.contains("@page"), "no page box");
        assert!(
            css.contains("display: table-header-group"),
            "table headings would not repeat across printed pages"
        );
    }

    /// The blanket rule the print section is explicitly not allowed to use: it
    /// makes a renderer push any large block whole onto an empty page.
    #[test]
    fn nothing_avoids_breaking_inside_everything() {
        for css in [SHARED, INVENTORY, PRINT] {
            assert!(
                !css.contains("* { break-inside: avoid"),
                "a blanket break-inside rule would strand large blocks on empty pages"
            );
        }
    }

    /// The comments explain the source and do not travel with the output.
    #[test]
    fn the_maintainers_commentary_stays_in_the_source() {
        let css = inventory_css();
        assert!(
            !css.contains("/*") && !css.contains("*/"),
            "a comment reached the document"
        );
        // And the rules themselves are untouched: still one per line, still
        // indented, still readable by whoever opens the source of a page.
        assert!(inventory_css().contains("\n  color: var(--fg-muted);\n"));
    }

    // Two tests stood here until 2026-08-23, when the comments stopped being
    // stripped in this file: one held that `content: "/*"` is a declaration
    // and not the start of a comment, the other that an unterminated comment
    // takes the rest of the file and not more. Both were about
    // `strip_comments`, a scanner this module no longer has — Lightning CSS
    // parses the stylesheet in the build instead, and the first of those cases
    // is one it cannot get wrong. What survives them is the test above: no
    // comment, from any source, reaches a document.

    /// The stack names the faces the corpus needs, not only the ones a user
    /// interface needs.
    ///
    /// Measured rather than assumed, by `examples/font_coverage`: the plain UI
    /// stack draws 259 of the corpus's 648 code points and the other 389 came
    /// out of macOS's silent substitution — 19 021 documents of cuneiform among
    /// them. Fallback is a property of one operating system, differs between
    /// machines, and a PDF engine need not do it at all.
    ///
    /// This is a source-level assertion because that is where the failure is
    /// visible. A missing family does not break a page, a build or a test: the
    /// document renders, on this machine, through a font nobody chose.
    #[test]
    fn the_font_stack_names_what_the_corpus_needs() {
        let stack = SHARED
            .split_once("--font-sans:")
            .and_then(|(_, rest)| rest.split_once(';'))
            .map(|(value, _)| value.to_string())
            .expect("the shared section declares --font-sans");

        // Every face `docs/FONTS.md` specifies, and the reason each is there
        // is written out both here and in the stylesheet itself.
        for face in [
            "Noto Sans Cuneiform",
            "UllikummiA",
            "STIX Two Math",
            "Arial",
        ] {
            assert!(
                stack.contains(face),
                "the stack no longer names {face}, so those characters would be \
                 drawn by whatever the operating system picks — or not at all. \
                 What each face covers is in docs/FONTS.md."
            );
        }

        // A generic stays last, so a machine without those faces degrades to
        // fallback rather than refusing to render.
        assert!(stack.trim_end().ends_with("sans-serif"));

        // And the one face that must never be named. `Hiragino Sans GB` covers
        // U+E83A only in the sense that its private-use area holds a Chinese
        // glyph at that number; naming it would make an unrelated sign the
        // official rendering of a TLHdig character.
        assert!(
            !stack.contains("Hiragino"),
            "a font was named that draws a private-use code point as something \
             it does not mean"
        );
    }

    /// Compact, and measured rather than assumed.
    ///
    /// The number is here so that a stylesheet which quietly became a framework
    /// is noticed while it is still one edit old. It mattered more when a copy
    /// went into all 663 CTH pages; the inventory carries one copy.
    #[test]
    fn the_repeated_stylesheet_stays_small() {
        let inventory = inventory_css().len();
        assert!(
            inventory < 24 * 1024,
            "the inventory's stylesheet is {inventory} bytes"
        );
    }
}
