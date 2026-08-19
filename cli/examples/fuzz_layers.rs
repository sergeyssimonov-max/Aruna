//! Scratch: the layers below the export, on input nobody wrote.
//!
//! None of these may panic, whatever they are handed. The parser reads a corpus
//! this program did not produce and the scanners read bytes; a panic in either
//! is a crash with no message. (The JSON reader is private and stays that way —
//! it is covered by its own tests rather than by widening the API for this.)
use aruna::catalog;
use aruna::parse::{
    group_label, group_runs, is_manuscript_xml, looks_like_manuscript, parse_cth_num,
    parse_manuscript, truncate_on_char_boundary,
};
use aruna::xml_scan as scan;

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
}

const PIECES: [&str; 34] = [
    "<",
    ">",
    "/",
    "?",
    "!",
    "-",
    "=",
    "\"",
    "'",
    " ",
    "\n",
    "\t",
    "\0",
    "&",
    "<AOxml",
    "<AOHeader",
    "</AOHeader>",
    "<docID>",
    "</docID>",
    "<meta",
    "<uebern",
    "editor=",
    "date=",
    "xml:lang=",
    "CTH",
    "InvNr",
    "<inv>",
    "<l lg=",
    "[",
    "]",
    "{",
    "}",
    "é",
    "𒀀",
];

fn blob(rng: &mut Rng, max: usize) -> String {
    let mut s = String::new();
    for _ in 0..rng.below(max) {
        s.push_str(PIECES[rng.below(PIECES.len())]);
    }
    s
}

fn main() {
    let mut rng = Rng(0xDEADBEEFCAFEBABE);
    let mut records = Vec::new();

    for round in 0..300_000 {
        let path = blob(&mut rng, 8);
        let xml = blob(&mut rng, 40);

        let _ = is_manuscript_xml(&path);
        let _ = looks_like_manuscript(&xml);
        let _ = parse_cth_num(&xml);
        for max in [0usize, 1, 3, 7, 64] {
            let cut = truncate_on_char_boundary(&xml, max);
            assert!(xml.starts_with(cut), "truncate wandered off the front");
            assert!(
                cut.len() <= max,
                "truncate returned {} for {max}",
                cut.len()
            );
        }

        let bytes = xml.as_bytes();
        let _ = scan::find_ci(bytes, b"cth");
        let _ = scan::find_exact(bytes, b"CTH");
        let _ = scan::find_open_tag(bytes, b"docID");
        let mut found = [None; 2];
        scan::find_open_tags(bytes, &[b"InvNr", b"inv"], &mut found);
        for (at, end) in found.into_iter().flatten() {
            assert!(
                at <= end && end <= bytes.len(),
                "tag span outside the input"
            );
        }
        let _ = scan::find_close_tag(bytes, 0, b"docID");
        let _ = scan::tag_text(bytes, b"docID");
        let _ = scan::strip_tags_bytes(bytes);
        let _ = scan::attr_value(bytes, b"editor");
        let _ = scan::find_year(bytes);
        let _ = scan::find_cth_number(bytes);
        scan::for_each_start_tag(bytes, |_, _| true);

        let record = parse_manuscript(&path, &xml);
        let _ = group_label(&record);
        if round % 97 == 0 {
            records.push(record);
        }
    }

    // The catalog, over records the parser produced from nonsense.
    records.sort_by(|a, b| (a.cth_num, &a.sigla).cmp(&(b.cth_num, &b.sigla)));
    let runs = group_runs(&records).count();
    let cat = catalog::render(&records, "fuzz");
    assert!(!cat.json.is_empty());
    println!("--- ok: {} records, {runs} groups ---", records.len());
}
