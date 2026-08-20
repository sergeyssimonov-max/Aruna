//! Scratch: hammer the pure halves of the export pipeline with generated input.
//!
//! Three invariants the package rests on, checked against input nobody wrote by
//! hand: normalising never distorts, the manifest is always valid JSON, and the
//! inventory never lets a field escape into markup.
use aruna::export::manifest::{render_manifest, FontContract};
use aruna::export::{
    inventory::{render_group_index, render_inventory},
    normalize_into, place, verify, Fragment, Placed,
};
use aruna::parse::ManuscriptRecord;
use std::collections::BTreeMap;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }
}

const NASTY: [&str; 24] = [
    "",
    " ",
    "<",
    ">",
    "&",
    "\"",
    "'",
    "</script>",
    "<!--",
    "]]>",
    "\\",
    "\n",
    "\t",
    "\u{0}",
    "\u{7f}",
    "\u{feff}",
    "é",
    "𒀀",
    "n\u{0301}",
    "%2F",
    "..",
    ".",
    "/",
    "\u{2028}",
];

fn text(rng: &mut Rng) -> String {
    let mut s = String::new();
    for _ in 0..rng.below(6) {
        s.push_str(rng.pick(&NASTY));
    }
    s
}

fn record(rng: &mut Rng, i: usize) -> ManuscriptRecord {
    let cth = format!("CTH {}", rng.below(4));
    ManuscriptRecord {
        title: text(rng),
        sigla: format!("{}{}", text(rng), i % 3), // collisions on purpose
        cth_num: cth.trim_start_matches("CTH ").parse().unwrap_or(u32::MAX),
        cth: Some(cth),
        authorship: text(rng),
        year: text(rng),
        lang: text(rng),
        inv: text(rng),
        corpus: text(rng),
    }
}

/// A document with a prologue that is plausible, hostile, or both.
fn document(rng: &mut Rng) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    if rng.below(3) == 0 {
        out.extend_from_slice("\u{feff}".as_bytes());
    }
    for _ in 0..rng.below(4) {
        out.extend_from_slice(
            rng.pick(&[
                "<?xml version=\"1.0\"?>",
                "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>",
                "<?xml-stylesheet type=\"text/css\" href=\"HPMxml.css\"?>",
                "<?xml-stylesheet href=\"other.css\"?>",
                "<?php evil ?>",
                "<?x?>",
            ])
            .as_bytes(),
        );
        for _ in 0..rng.below(3) {
            out.extend_from_slice(rng.pick(&["\n", " ", "\r\n", "\t"]).as_bytes());
        }
    }
    out.extend_from_slice(b"<AOxml>");
    for _ in 0..rng.below(8) {
        out.extend_from_slice(text(rng).as_bytes());
    }
    out.extend_from_slice(b"</AOxml>");
    out
}

fn main() {
    const SEED: u64 = 0x9E3779B97F4A7C15;
    println!("seed: {SEED:#018x}  (fixed, so a failure here reproduces)");
    let mut rng = Rng(SEED);
    let mut normalised = Vec::new();
    let (mut distorted, mut json_bad, mut html_bad) = (0usize, 0usize, 0usize);
    let mut refused = 0usize;

    // 1. Normalising never distorts, whatever the prologue looks like.
    for _ in 0..200_000 {
        let source = document(&mut rng);
        normalised.clear();
        normalize_into(&source, &mut normalised);
        // A document that declares an encoding other than UTF-8 must be
        // refused: keeping its bytes under a UTF-8 declaration would change
        // what they mean, and the byte comparison would not see it. That is the
        // correct outcome, so the oracle has to know the rule rather than count
        // the refusal as a fault.
        let text = String::from_utf8_lossy(&source);
        let declares_other = text
            .split("?>")
            .take_while(|piece| piece.contains("<?"))
            .any(|pi| pi.contains("encoding=") && !pi.contains("UTF-8"));
        match (verify::compare(&source, &normalised), declares_other) {
            (Ok(_), false) => {}
            (Err(_), true) => refused += 1,
            (Ok(_), true) => {
                distorted += 1;
                if distorted <= 3 {
                    eprintln!("ALLOWED THROUGH: {text:?}");
                }
            }
            (Err(why), false) => {
                distorted += 1;
                if distorted <= 3 {
                    eprintln!("DISTORTED: {why}\n  source: {text:?}");
                }
            }
        }
    }

    // 2 and 3. The two documents, over records built to break them.
    for round in 0..2_000 {
        let n = 1 + rng.below(12);
        let records: Vec<ManuscriptRecord> = (0..n).map(|i| record(&mut rng, i)).collect();
        let fragments: Vec<Fragment> = records
            .iter()
            .enumerate()
            .map(|(i, r)| Fragment {
                record: r.clone(),
                source: format!("{}/x{}.xml", text(&mut rng), i),
            })
            .collect();
        let mut sorted = fragments.clone();
        aruna::order::sort_by_display_order(&mut sorted, |f: &Fragment| &f.record);
        let placed: Vec<Placed> = match place(&sorted) {
            Ok(p) => p,
            Err(_) => continue, // a refused collision is a correct outcome
        };
        let recs: Vec<ManuscriptRecord> = sorted.iter().map(|f| f.record.clone()).collect();

        let mut applied = BTreeMap::new();
        applied.insert(text(&mut rng), 1usize);
        let mut fonts = FontContract::default();
        fonts.observe(&text(&mut rng));
        let json = render_manifest(&recs, &placed, &text(&mut rng), "abc", &applied, &fonts);
        if let Err(why) = check_json(&json) {
            json_bad += 1;
            if json_bad <= 3 {
                eprintln!("BAD JSON (round {round}): {why}");
            }
        }

        for html in [
            render_inventory(&recs, &placed, &text(&mut rng)),
            render_group_index("CTH 0", &recs[..1], &placed[..1]),
        ] {
            if let Err(why) = check_html(&html) {
                html_bad += 1;
                if html_bad <= 3 {
                    eprintln!("BAD HTML (round {round}): {why}");
                }
            }
        }
    }

    println!("distorted: {distorted} | correctly refused: {refused} | bad json: {json_bad} | bad html: {html_bad}");
    if distorted + json_bad + html_bad > 0 {
        std::process::exit(1);
    }
    println!("--- ok ---");
}

/// A strict-enough JSON check without a parser dependency: structure, quoting,
/// and that no control character rode in raw.
fn check_json(s: &str) -> Result<(), String> {
    let b = s.as_bytes();
    let (mut depth, mut in_str, mut esc) = (0i32, false, false);
    for (i, &c) in b.iter().enumerate() {
        if in_str {
            if esc {
                if !matches!(
                    c,
                    b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' | b'u'
                ) {
                    return Err(format!("bad escape \\{} at {i}", c as char));
                }
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            } else if c < 0x20 {
                return Err(format!("raw control U+{c:04X} inside a string at {i}"));
            }
        } else {
            match c {
                b'"' => in_str = true,
                b'{' | b'[' => depth += 1,
                b'}' | b']' => {
                    depth -= 1;
                    if depth < 0 {
                        return Err(format!("closed too much at {i}"));
                    }
                }
                _ => {}
            }
        }
    }
    if in_str {
        return Err("unterminated string".into());
    }
    if depth != 0 {
        return Err(format!("{depth} unclosed"));
    }
    Ok(())
}

/// No field may leave its element: a raw `<` outside a tag, or a `"` inside an
/// attribute, is a field that escaped.
///
/// `<script>` and `<style>` hold raw text by definition — `a < b.length` in the
/// inventory's own JavaScript is not markup — so their contents are skipped up
/// to the matching close tag, which is exactly the rule a browser applies.
fn check_html(s: &str) -> Result<(), String> {
    let b = s.as_bytes();
    let (mut i, mut in_tag, mut quote) = (0usize, false, 0u8);
    let mut tag_start = 0usize;
    while i < b.len() {
        let c = b[i];
        if in_tag {
            if quote != 0 {
                if c == quote {
                    quote = 0;
                } else if c == b'<' || c == b'>' {
                    return Err(format!("raw {} inside an attribute at {i}", c as char));
                }
            } else if c == b'"' || c == b'\'' {
                quote = c;
            } else if c == b'>' {
                in_tag = false;
                let name = s[tag_start + 1..i]
                    .split([' ', '\t', '\n'])
                    .next()
                    .unwrap_or("");
                if name.eq_ignore_ascii_case("script") || name.eq_ignore_ascii_case("style") {
                    let close = format!("</{}", name.to_ascii_lowercase());
                    match s[i..].to_ascii_lowercase().find(&close) {
                        Some(off) => {
                            i += off;
                            continue;
                        }
                        None => return Err(format!("<{name}> is never closed")),
                    }
                }
            } else if c == b'<' {
                return Err(format!("nested < at {i}"));
            }
        } else if c == b'<' {
            in_tag = true;
            tag_start = i;
        }
        i += 1;
    }
    if in_tag {
        return Err("unterminated tag".into());
    }
    Ok(())
}
