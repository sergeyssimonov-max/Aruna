//! Scratch: the XML layer on bytes nobody wrote.
//!
//! The model (`Document::read`), the classifier (`classify`) and the scanner
//! behind the parser (`beyond_the_parser`) all read a corpus this program did
//! not produce, and none of them may panic whatever they are handed. Here they
//! are handed a well-formed manuscript mutated at the byte level – bytes
//! flipped, cut, dropped, doubled and spliced with markup, with bytes that are
//! not UTF-8 and with the byte-order marks of UTF-16 – so that most inputs are
//! nearly a document, which is where a reader's edge cases live.
//!
//! No panic is the invariant, and one more is asserted: nothing the model reads
//! is refused by the classifier or the scanner. The other direction is counted,
//! not asserted. On the corpus the three agree on every document
//! (`document_model.rs`); on nonsense the model refuses more, and for reasons
//! the other two do not ask about – a document with no root or content after
//! it, a prefix never declared, a construct with no policy, and a broken
//! reference in an attribute value, which the model unescapes and the
//! classifier does not (`Refusal::Unexplained`). First run, 2026-09-25, seed
//! below: 2 132 refusals of the last kind, 2 107 of them named by neither of
//! the other two; 6 879 disagreements in all; no panic.
use aruna::document::{Document, Refusal};
use aruna::parse::looks_like_manuscript;
use aruna::xml_wellformed::{beyond_the_parser, classify};
use std::collections::BTreeMap;
use std::panic::catch_unwind;

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

/// A manuscript as the corpus writes one: prolog, stylesheet instruction,
/// namespaced attributes, an entity, a comment, CDATA and cuneiform.
const SEED_DOCUMENT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<?xml-stylesheet href="HPMxml.css"?>
<AOxml xmlns:xi="http://www.w3.org/2001/XInclude" xmlns:AO="urn:aruna:ao">
<AOHeader><docID>KBo 1.1</docID><meta><AO:Manuscripts><AO:TxtPubl>KBo 1.1</AO:TxtPubl><AO:InvNr>Bo 2019/1</AO:InvNr></AO:Manuscripts></meta>
<uebersetzung editor="A. B&amp;C" date="2021"/><!-- a comment --></AOHeader>
<body><div1 type="AOText"><text xml:lang="Hit"><lb lnr="1'"/><w>𒀀-na</w><![CDATA[<raw>]]><w>x&lt;y</w></text></div1></body>
</AOxml>
"#;

const PIECES: [&[u8]; 24] = [
    b"<",
    b">",
    b"/>",
    b"</",
    b"<?",
    b"?>",
    b"<!--",
    b"-->",
    b"<![CDATA[",
    b"]]>",
    b"<!DOCTYPE x [<!ENTITY e \"v\">]>",
    b"&",
    b"&amp;",
    b"&#x0;",
    b"&e;",
    b"=\"",
    b"xmlns:p=\"\"",
    b"p:",
    b"\0",
    b"\xEF\xBB\xBF",
    b"\xFF\xFE",
    b"\xFE\xFF",
    b"\xC3",
    b"\xF0\x92\x80",
];

fn mutate(rng: &mut Rng, bytes: &mut Vec<u8>) {
    for _ in 0..1 + rng.below(4) {
        if bytes.is_empty() {
            bytes.extend_from_slice(PIECES[rng.below(PIECES.len())]);
            continue;
        }
        let at = rng.below(bytes.len());
        match rng.below(6) {
            0 => bytes[at] = rng.next() as u8,
            1 => {
                bytes.remove(at);
            }
            2 => bytes.truncate(at),
            3 => {
                let piece = PIECES[rng.below(PIECES.len())];
                bytes.splice(at..at, piece.iter().copied());
            }
            4 => {
                let end = (at + 1 + rng.below(32)).min(bytes.len());
                let copy = bytes[at..end].to_vec();
                bytes.splice(at..at, copy);
            }
            _ => {
                let end = (at + 1 + rng.below(32)).min(bytes.len());
                bytes.drain(at..end);
            }
        }
    }
}

fn kind(refusal: &Refusal) -> &'static str {
    match refusal {
        Refusal::NotUtf8 { .. } => "not-utf8",
        Refusal::Unread(_) => "unread",
        Refusal::BeyondTheParser(_) => "beyond",
        Refusal::Unexplained { .. } => "unexplained",
        Refusal::UndeclaredPrefix { .. } => "undeclared-prefix",
        Refusal::Undecided { .. } => "undecided",
        Refusal::OutsideRoot { .. } => "outside-root",
        Refusal::NoRoot => "no-root",
    }
}

fn main() {
    const SEED: u64 = 0x005E_ED0F_A70C_5A11;
    const ROUNDS: usize = 200_000;
    println!("seed: {SEED:#018x}  (fixed, so a failure here reproduces)");
    if let Err(refusal) = Document::read(SEED_DOCUMENT.as_bytes()) {
        panic!("the unmutated document is refused, so nothing below is near a document: {refusal}");
    }
    let mut rng = Rng(SEED);
    let mut read = 0usize;
    let mut refused: BTreeMap<&str, usize> = BTreeMap::new();
    let mut disagree: BTreeMap<(&str, bool, bool), usize> = BTreeMap::new();

    for round in 0..ROUNDS {
        let mut bytes = SEED_DOCUMENT.as_bytes().to_vec();
        mutate(&mut rng, &mut bytes);

        let refusal = catch_unwind(|| Document::read(&bytes).err().map(|r| kind(&r)))
            .unwrap_or_else(|_| panic!("round {round}: the model panicked on {bytes:?}"));
        let model = refusal.is_none();
        let classified = catch_unwind(|| classify(&bytes).is_some())
            .unwrap_or_else(|_| panic!("round {round}: the classifier panicked on {bytes:?}"));
        let beyond = catch_unwind(|| beyond_the_parser(&bytes).is_some())
            .unwrap_or_else(|_| panic!("round {round}: the scanner panicked on {bytes:?}"));
        if let Ok(text) = std::str::from_utf8(&bytes) {
            catch_unwind(|| looks_like_manuscript(text))
                .unwrap_or_else(|_| panic!("round {round}: the sniffer panicked on {text:?}"));
        }

        let named = classified || beyond;
        match refusal {
            None => read += 1,
            Some(kind) => *refused.entry(kind).or_default() += 1,
        }
        assert!(
            !(model && named),
            "round {round}: the model read what the classifier or the scanner refuses: {bytes:?}"
        );
        if named == model {
            *disagree
                .entry((refusal.unwrap_or("read"), classified, beyond))
                .or_default() += 1;
        }
    }
    println!("--- ok: {ROUNDS} inputs, {read} read ---");
    println!("refused, by the model's reason: {refused:?}");
    println!("disagreements, (model, classifier, scanner): {disagree:?}");
}
