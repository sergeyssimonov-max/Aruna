//! Which of the corpus's characters the installed fonts can actually draw.
//!
//! ```text
//! cargo run --release --example font_coverage -- fixtures/…zip [report.json]
//! ```
//!
//! The question this answers cannot be answered by looking at a page. A missing
//! glyph on macOS is not a blank: the system silently substitutes from whatever
//! font it can find, so a document that renders perfectly on this machine may
//! render as empty boxes on the next one — and a private-use code point can be
//! drawn by an unrelated font that happens to have something in that slot, which
//! looks like a correct sign and is not.
//!
//! So this reads the `cmap` table of every font file the system offers and asks,
//! for each of the 648 code points the corpus really uses, which fonts contain
//! it. Three answers matter, in order of how much trouble they cause:
//!
//! * **covered by nothing** — a PDF will have a hole there, whatever engine
//!   renders it;
//! * **covered only by a user-installed font** — works here, tofu on any other
//!   machine, and cannot be relied on unless the font is licensed and bundled;
//! * **covered outside the declared stack** — works today by fallback, and
//!   fallback is a property of the operating system rather than of this project.
//!
//! No font parsing crate. The `cmap` subtable formats this needs are two, both
//! are simple arrays, and the whole reader below is under two hundred lines —
//! against a dependency that would have to be audited, licensed and kept.
//! Everything it reads is bounds-checked; a malformed or truncated font is
//! skipped rather than trusted.
//!
//! Corpus-driven by construction: there is no list of "the Hittitological
//! characters" anywhere in this file. The code points come out of the archive.

use aruna::parse::{
    is_manuscript_xml, looks_like_manuscript, truncate_on_char_boundary, HEADER_READ_LIMIT,
};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Where macOS keeps fonts, in the order that matters for the verdict.
///
/// The first two are the system's and are on every Mac of this version. The
/// last two are what somebody installed, and a corpus that depends on them
/// depends on that person's machine.
const SYSTEM_DIRS: [&str; 2] = [
    "/System/Library/Fonts",
    "/System/Library/Fonts/Supplemental",
];
const USER_DIRS: [&str; 2] = ["/Library/Fonts", "~/Library/Fonts"];

/// The stack as the documents declare it, read out of the stylesheet.
///
/// Not a copy. A list written here would be a second statement of the same
/// decision, and the two would drift — which is exactly the failure this whole
/// example exists to detect, so making it in the detector would be absurd.
/// The built canonical section is compiled in, so this cannot be run against a
/// stylesheet other than the one that ships.
const STYLESHEET: &str = include_str!("../src/generated/canonical.css");

/// The families named in `--font-sans`, in order.
fn declared_families() -> Vec<String> {
    let Some(rest) = STYLESHEET.split_once("--font-sans:") else {
        eprintln!("the canonical section no longer declares --font-sans");
        std::process::exit(1);
    };
    let Some((value, _)) = rest.1.split_once(';') else {
        eprintln!("--font-sans is not terminated");
        std::process::exit(1);
    };
    value
        .split(',')
        .map(|family| family.trim().trim_matches('"').trim().to_lowercase())
        // `sans-serif` and its kin name no file; they are the last resort and
        // are exactly what "covered by fallback" means.
        .filter(|family| {
            !family.is_empty()
                && !matches!(
                    family.as_str(),
                    "sans-serif" | "serif" | "monospace" | "system-ui"
                )
        })
        .collect()
}

/// The file names the generic macOS families resolve to.
///
/// `system-ui` and `-apple-system` are not families a directory contains: they
/// are instructions to use the platform's interface font, which on macOS 13 is
/// San Francisco, in the files below. Named here because the mapping is a fact
/// about the operating system rather than about this project.
const PLATFORM_UI_FILES: [&str; 5] = ["sfns", "sfnsdisplay", "sfnstext", "helvetica", "geneva"];

fn main() {
    let mut args = std::env::args_os().skip(1);
    let Some(zip) = args.next() else {
        eprintln!("usage: font_coverage <archive.zip> [report.json]");
        std::process::exit(2);
    };
    let report_path = args.next().map(PathBuf::from);

    eprintln!("reading the corpus…");
    let used = code_points(Path::new(&zip));
    eprintln!("  {} distinct code points", used.len());

    eprintln!("reading fonts…");
    let fonts = installed_fonts();
    eprintln!("  {} font files", fonts.len());

    let families = declared_families();
    eprintln!("declared stack: {}", families.join(", "));

    let coverage = coverage(&used, &fonts, &families);
    report(&used, &coverage);
    let missing = verify_environment(&fonts, &families);
    suggest(&used, &fonts, &coverage, &families);

    // A non-zero exit when a required face is absent, so this can be a gate in
    // a script rather than something a person has to read.
    if missing {
        std::process::exit(1);
    }

    if let Some(path) = report_path {
        write_report(&path, &used, &coverage);
        eprintln!("\nwritten to {}", path.display());
    }
}

// ---------------------------------------------------------------------------
// The corpus
// ---------------------------------------------------------------------------

/// Every distinct code point in the manuscripts of `zip`.
///
/// The whole of each document, not the header window: a character the corpus
/// uses once, in one line of one transliteration, still has to be drawable.
fn code_points(zip: &Path) -> BTreeSet<u32> {
    let file = std::fs::File::open(zip).unwrap_or_else(|e| {
        eprintln!("{}: {e}", zip.display());
        std::process::exit(1);
    });
    let mut archive = zip::ZipArchive::new(std::io::BufReader::with_capacity(1 << 18, file))
        .unwrap_or_else(|e| {
            eprintln!("{}: {e}", zip.display());
            std::process::exit(1);
        });

    let mut used = BTreeSet::new();
    let mut bytes = Vec::new();

    for i in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(i) else {
            continue;
        };
        if !is_manuscript_xml(entry.name()) {
            continue;
        }
        bytes.clear();
        if entry.read_to_end(&mut bytes).is_err() {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        // The same gate the exporter applies, so this counts the documents that
        // ship rather than everything shaped like one.
        if !looks_like_manuscript(truncate_on_char_boundary(&text, HEADER_READ_LIMIT)) {
            continue;
        }
        used.extend(text.chars().map(|c| c as u32));
    }
    used
}

// ---------------------------------------------------------------------------
// The fonts
// ---------------------------------------------------------------------------

/// One font file, and where it came from.
struct Font {
    path: PathBuf,
    system: bool,
    covers: BTreeSet<u32>,
    /// Whether this file draws characters or merely marks them as undrawable.
    ///
    /// A last-resort font maps enormous ranges of Unicode to a single glyph —
    /// a box saying "nothing here can draw this". Counting that as coverage
    /// would turn every audit into 648 of 648 and mean nothing, so it is kept
    /// apart. See [`characters_of`].
    last_resort: bool,
}

impl Font {
    fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Whether this file is one the declared stack resolves to.
    ///
    /// Matched on the file name with its spaces and hyphens removed, because a
    /// family called `Noto Sans Cuneiform` lives in `NotoSansCuneiform-Regular.ttf`
    /// and neither spelling is wrong.
    fn declared(&self, families: &[String]) -> bool {
        let name = self.name().to_lowercase().replace([' ', '-', '_'], "");
        families
            .iter()
            .any(|family| name.starts_with(&family.replace(' ', "")))
            || PLATFORM_UI_FILES.iter().any(|file| name.starts_with(file))
    }
}

/// Every font file the system offers, with the code points each one covers.
fn installed_fonts() -> Vec<Font> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut fonts = Vec::new();

    for (dir, system) in SYSTEM_DIRS
        .iter()
        .map(|d| (d.to_string(), true))
        .chain(USER_DIRS.iter().map(|d| (d.replace('~', &home), false)))
    {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_font = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                matches!(e.to_lowercase().as_str(), "ttf" | "otf" | "ttc" | "otc")
            });
            if !is_font {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let (covers, last_resort) = characters_of(&bytes);
            if !covers.is_empty() {
                fonts.push(Font {
                    path,
                    system,
                    covers,
                    last_resort,
                });
            }
        }
    }
    fonts
}

// ---------------------------------------------------------------------------
// `cmap`, read by hand
// ---------------------------------------------------------------------------

/// Read `n` big-endian bytes at `at`, or `None` past the end.
fn be(bytes: &[u8], at: usize, n: usize) -> Option<u32> {
    let slice = bytes.get(at..at + n)?;
    Some(slice.iter().fold(0u32, |acc, b| (acc << 8) | u32::from(*b)))
}

/// Every character the font at `bytes` can draw, and whether it really draws
/// them.
///
/// Handles a single font and a collection (`ttcf`), and within each the three
/// `cmap` subtable formats that carry Unicode above the legacy encodings:
/// format 4 for the Basic Multilingual Plane, format 12 above it — cuneiform
/// lives there — and format 13.
///
/// Format 13 is the interesting one and was missed at first. Its purpose in the
/// OpenType specification is last-resort fonts: it maps a *range* of code
/// points to a *single* glyph. macOS ships exactly one such font,
/// `LastResort.otf`, whose whole `cmap` is four groups — and one of them sends
/// `U+E000..U+10FFFF`, a million code points including every private use area,
/// to one glyph. That glyph is the box a reader sees when nothing can draw a
/// character.
///
/// So a format 13 subtable is reported, and reported as what it is. Reading it
/// as coverage would have said the corpus is fully covered while five of its
/// signs render as an error box; not reading it at all — which is what happened
/// until 2026-08-22 — hid the one font that explains what a reader actually
/// sees.
fn characters_of(bytes: &[u8]) -> (BTreeSet<u32>, bool) {
    let mut found = BTreeSet::new();
    let mut last_resort = false;

    // A collection names its members' offsets; a single font starts at zero.
    let offsets: Vec<usize> = if bytes.starts_with(b"ttcf") {
        let count = be(bytes, 8, 4).unwrap_or(0) as usize;
        (0..count.min(64))
            .filter_map(|i| be(bytes, 12 + i * 4, 4).map(|o| o as usize))
            .collect()
    } else {
        vec![0]
    };

    for base in offsets {
        let Some(tables) = be(bytes, base + 4, 2) else {
            continue;
        };
        for i in 0..tables as usize {
            let record = base + 12 + i * 16;
            let Some(tag) = bytes.get(record..record + 4) else {
                continue;
            };
            if tag != b"cmap" {
                continue;
            }
            let Some(cmap) = be(bytes, record + 8, 4).map(|o| o as usize) else {
                continue;
            };
            read_cmap(bytes, cmap, &mut found, &mut last_resort);
        }
    }
    (found, last_resort)
}

/// Walk one `cmap` table's subtables.
fn read_cmap(bytes: &[u8], cmap: usize, found: &mut BTreeSet<u32>, last_resort: &mut bool) {
    let Some(count) = be(bytes, cmap + 2, 2) else {
        return;
    };
    for i in 0..count as usize {
        let record = cmap + 4 + i * 8;
        let Some(offset) = be(bytes, record + 4, 4).map(|o| cmap + o as usize) else {
            continue;
        };
        match be(bytes, offset, 2) {
            Some(4) => read_format_4(bytes, offset, found),
            Some(12) => read_format_12(bytes, offset, found),
            Some(13) => {
                *last_resort = true;
                read_format_13(bytes, offset, found);
            }
            // Formats 0, 2, 6 and 14 also exist. Measured across the 366 fonts
            // on this machine, every file carrying one of them also carries a
            // format 4 or 12 subtable, so they add nothing — and formats 0 and
            // 6 cannot express anything format 4 could not. A subtable this
            // does not understand is skipped rather than guessed at.
            _ => {}
        }
    }
}

/// Format 4: segments of the Basic Multilingual Plane.
fn read_format_4(bytes: &[u8], at: usize, found: &mut BTreeSet<u32>) {
    let Some(seg_x2) = be(bytes, at + 6, 2).map(|v| v as usize) else {
        return;
    };
    let segments = seg_x2 / 2;
    let ends = at + 14;
    let starts = ends + seg_x2 + 2;

    for s in 0..segments {
        let (Some(end), Some(start)) = (be(bytes, ends + s * 2, 2), be(bytes, starts + s * 2, 2))
        else {
            continue;
        };
        // The final segment is the 0xFFFF terminator, and a segment that runs
        // backwards is a malformed font.
        if start > end || start == 0xFFFF {
            continue;
        }
        // The glyph mapping is not read: the question is coverage, and a
        // character named by a segment is one the font claims to have.
        for cp in start..=end {
            found.insert(cp);
        }
    }
}

/// Format 12: groups covering the whole of Unicode, including cuneiform.
fn read_format_12(bytes: &[u8], at: usize, found: &mut BTreeSet<u32>) {
    let Some(groups) = be(bytes, at + 12, 4) else {
        return;
    };
    // A malformed length here would otherwise be a very long loop over nothing.
    for g in 0..groups.min(100_000) as usize {
        let record = at + 16 + g * 12;
        let (Some(start), Some(end)) = (be(bytes, record, 4), be(bytes, record + 4, 4)) else {
            continue;
        };
        if start > end || end - start > 0x11_0000 {
            continue;
        }
        for cp in start..=end {
            found.insert(cp);
        }
    }
}

/// Format 13: a range of code points, all drawn by one glyph.
///
/// Read exactly like format 12 — the two have the same layout — and the
/// difference is what the third field means. In format 12 it is the first glyph
/// of a run; in format 13 it is *the* glyph, for every code point in the range.
fn read_format_13(bytes: &[u8], at: usize, found: &mut BTreeSet<u32>) {
    let Some(groups) = be(bytes, at + 12, 4) else {
        return;
    };
    for g in 0..groups.min(100_000) as usize {
        let record = at + 16 + g * 12;
        let (Some(start), Some(end)) = (be(bytes, record, 4), be(bytes, record + 4, 4)) else {
            continue;
        };
        if start > end || end > 0x10_FFFF {
            continue;
        }
        for cp in start..=end {
            found.insert(cp);
        }
    }
}

// ---------------------------------------------------------------------------
// The verdict
// ---------------------------------------------------------------------------

/// Which fonts cover each code point, split by where they came from.
struct Coverage {
    /// Code point → the system fonts that draw it.
    system: BTreeMap<u32, Vec<String>>,
    /// Code point → the user-installed fonts that draw it.
    user: BTreeMap<u32, Vec<String>>,
    /// Code point → the declared stack's fonts that draw it.
    declared: BTreeMap<u32, Vec<String>>,
    /// Code point → the last-resort fonts that mark it as undrawable.
    ///
    /// Not coverage, and kept apart so it can never be mistaken for it: this is
    /// what a reader sees *instead of* the character.
    indicator: BTreeMap<u32, Vec<String>>,
}

fn coverage(used: &BTreeSet<u32>, fonts: &[Font], families: &[String]) -> Coverage {
    let mut result = Coverage {
        system: BTreeMap::new(),
        user: BTreeMap::new(),
        declared: BTreeMap::new(),
        indicator: BTreeMap::new(),
    };
    for font in fonts {
        let name = font.name();
        let declared = font.declared(families);

        // A last-resort font draws nothing; it marks. Recorded, never counted.
        if font.last_resort {
            for cp in used.intersection(&font.covers) {
                result.indicator.entry(*cp).or_default().push(name.clone());
            }
            continue;
        }

        for cp in used.intersection(&font.covers) {
            if font.system {
                result.system.entry(*cp).or_default().push(name.clone());
            } else {
                result.user.entry(*cp).or_default().push(name.clone());
            }
            if declared {
                result.declared.entry(*cp).or_default().push(name.clone());
            }
        }
    }
    result
}

fn report(used: &BTreeSet<u32>, coverage: &Coverage) {
    let above_bmp = used.iter().filter(|cp| **cp > 0xFFFF).count();
    let uncovered: Vec<u32> = used
        .iter()
        .copied()
        .filter(|cp| !coverage.system.contains_key(cp) && !coverage.user.contains_key(cp))
        .collect();
    let user_only: Vec<u32> = used
        .iter()
        .copied()
        .filter(|cp| !coverage.system.contains_key(cp) && coverage.user.contains_key(cp))
        .collect();
    let outside_stack = used.len() - coverage.declared.len();

    println!("\ncorpus");
    println!("  distinct code points          {}", used.len());
    println!("  above the BMP                 {above_bmp}");

    println!("\ncoverage");
    println!(
        "  by the declared font stack    {} of {}",
        coverage.declared.len(),
        used.len()
    );
    println!("  outside it, by system fallback {outside_stack}");
    println!("  only by an installed font     {}", user_only.len());
    println!("  by nothing at all             {}", uncovered.len());

    if !user_only.is_empty() {
        println!("\nonly on this machine — tofu on another:");
        for cp in &user_only {
            let by = coverage
                .user
                .get(cp)
                .map(|v| v.join(", "))
                .unwrap_or_default();
            println!("  U+{cp:04X}  {by}");
        }
    }

    if !uncovered.is_empty() {
        println!("\nno font here draws these:");
        for cp in &uncovered {
            // What the reader sees instead. A last-resort font contributes a
            // box meaning "nothing can draw this", which is worth knowing and
            // must never be read as the character having been drawn.
            let marked = coverage
                .indicator
                .get(cp)
                .map(|by| format!("  → shown as a placeholder by {}", by.join(", ")))
                .unwrap_or_else(|| "  → nothing at all: blank or tofu".to_string());
            println!("  U+{cp:04X}{marked}");
        }
    }

    // The verdict, said once and in the words the specification uses.
    println!();
    if uncovered.is_empty() && user_only.is_empty() {
        println!("critical uncovered glyphs: NONE");
    } else {
        println!(
            "critical uncovered glyphs: {} uncovered, {} depending on an installed font",
            uncovered.len(),
            user_only.len()
        );
    }
}

/// Is every face the stack names actually installed?
///
/// The coverage number above answers "what can this machine draw". This answers
/// the different question somebody setting up another machine has: "have I got
/// what the specification says I need". The two part company exactly when a
/// font is missing, which is the moment it matters.
fn verify_environment(fonts: &[Font], families: &[String]) -> bool {
    println!("\nthe environment (docs/FONTS.md)");
    let mut missing = false;

    for family in families {
        let flat = family.replace(' ', "");
        // `-apple-system`, `BlinkMacSystemFont` and `Segoe UI` name the
        // platform's interface font rather than a file. They are satisfied by
        // the platform, not by a directory, and saying "missing" for them would
        // be false.
        if flat.starts_with('-') || flat == "blinkmacsystemfont" || flat == "segoeui" {
            println!("  {family:24} platform default");
            continue;
        }
        // The shortest matching file name, so `Arial` reports `Arial.ttf`
        // rather than `ArialHB.ttc` — both begin with the family, and the one
        // that is only the family is the one meant.
        let found = fonts
            .iter()
            .filter(|f| {
                f.name()
                    .to_lowercase()
                    .replace([' ', '-', '_'], "")
                    .starts_with(&flat)
            })
            .min_by_key(|f| f.name().len());
        match found {
            Some(font) => println!(
                "  {family:24} {}  {}",
                font.name(),
                if font.system { "system" } else { "installed" }
            ),
            None => {
                missing = true;
                println!("  {family:24} MISSING — see docs/FONTS.md");
            }
        }
    }
    missing
}

/// What the declared stack would have to name to stop relying on fallback.
///
/// Greedy set cover: repeatedly take the font that draws the most of what is
/// still uncovered. Set cover is NP-hard and the greedy answer is not
/// guaranteed minimal — it does not have to be. The question is not "what is
/// the smallest possible list" but "which few fonts would make the stack
/// honest", and for a corpus whose gaps fall into two or three script ranges
/// the greedy answer is the obvious one.
///
/// System fonts are preferred over installed ones at equal coverage: a font
/// under `/System/Library/Fonts` is on every Mac of this version, and one under
/// `~/Library/Fonts` is on this desk.
fn suggest(used: &BTreeSet<u32>, fonts: &[Font], coverage: &Coverage, families: &[String]) {
    let mut missing: BTreeSet<u32> = used
        .iter()
        .copied()
        .filter(|cp| !coverage.declared.contains_key(cp))
        .collect();
    if missing.is_empty() {
        println!("\nthe declared stack already covers the corpus.");
        return;
    }

    println!(
        "\nto cover the {} points the stack does not name:",
        missing.len()
    );
    let mut chosen: Vec<(String, usize, Vec<u32>, bool)> = Vec::new();

    while !missing.is_empty() {
        let best = fonts
            .iter()
            // A last-resort font would "cover" everything and suggest itself
            // for every gap, which is the opposite of useful.
            .filter(|f| !f.declared(families) && !f.last_resort)
            .map(|f| (f, f.covers.intersection(&missing).count()))
            .filter(|(_, n)| *n > 0)
            // Most coverage wins; a system font wins a tie.
            .max_by_key(|(f, n)| (*n, f.system));
        let Some((font, gained)) = best else {
            break;
        };
        let taken: Vec<u32> = font.covers.intersection(&missing).copied().collect();
        for cp in &taken {
            missing.remove(cp);
        }
        chosen.push((font.name(), gained, taken, font.system));
    }

    for (name, gained, points, system) in &chosen {
        println!(
            "  {name:34} +{gained:<4} {}",
            if *system {
                "system"
            } else {
                "INSTALLED HERE ONLY"
            }
        );
        // A font carrying a handful of code points is one whose place in the
        // stack has to be argued a character at a time, so they are named. A
        // font carrying hundreds is a script, and the count says it.
        if *gained <= 8 {
            let list: Vec<String> = points.iter().map(|cp| format!("U+{cp:04X}")).collect();
            println!("  {:34} {}", "", list.join(" "));
        }
    }
    if !missing.is_empty() {
        println!("  {} still uncovered by any font", missing.len());
    }
}

/// The same findings as JSON, for whatever reads them next.
fn write_report(path: &Path, used: &BTreeSet<u32>, coverage: &Coverage) {
    let list = |points: &[u32]| {
        points
            .iter()
            .map(|cp| format!("\"U+{cp:04X}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let uncovered: Vec<u32> = used
        .iter()
        .copied()
        .filter(|cp| !coverage.system.contains_key(cp) && !coverage.user.contains_key(cp))
        .collect();
    let user_only: Vec<u32> = used
        .iter()
        .copied()
        .filter(|cp| !coverage.system.contains_key(cp) && coverage.user.contains_key(cp))
        .collect();

    let json = format!(
        "{{\n  \"code_points\": {},\n  \"covered_by_declared_stack\": {},\n  \
         \"uncovered\": [{}],\n  \"user_font_only\": [{}]\n}}\n",
        used.len(),
        coverage.declared.len(),
        list(&uncovered),
        list(&user_only),
    );
    if let Err(e) = std::fs::write(path, json) {
        eprintln!("{}: {e}", path.display());
    }
}
