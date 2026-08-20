//! What the corpus actually contains, structurally, counted rather than assumed.
//!
//! ```text
//! cargo run --release --example corpus_inventory -- fixtures/…zip [report.json]
//! ```
//!
//! This program does not parse XML in the sense the next stage will need. It
//! scans bytes and counts shapes: how deep documents go, what they carry
//! besides elements, which code points appear in them. Its purpose is to say
//! what a real parser and a PDF renderer would have to handle, and to say it
//! from the corpus rather than from expectation.
//!
//! Nothing here resolves anything. No entity is expanded, no DTD is fetched, no
//! XInclude is followed, no path is opened but the archive named on the command
//! line — which is opened for reading and never written. That is a property of
//! the scanner, not a setting on it.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;
use std::path::PathBuf;

/// What one document is made of.
#[derive(Default)]
struct Shape {
    bytes: usize,
    elements: usize,
    attributes: usize,
    max_depth: usize,
    longest_text: usize,
    empty_elements: usize,
    comments: usize,
    processing_instructions: usize,
    cdata: usize,
    entity_refs: BTreeSet<String>,
    namespaces: BTreeSet<String>,
    root: String,
    declared_encoding: Option<String>,
    has_bom: bool,
    has_declaration: bool,
    has_doctype: bool,
    has_xinclude: bool,
    mixed_content: bool,
    unbalanced: Option<String>,
    ids: Vec<String>,
}

/// Read one document's shape without resolving anything in it.
fn shape_of(bytes: &[u8], code_points: &mut HashMap<u32, usize>) -> Shape {
    let mut s = Shape {
        bytes: bytes.len(),
        ..Default::default()
    };

    let body = if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        s.has_bom = true;
        rest
    } else {
        bytes
    };

    // The distinct code points this document uses, counted once per document so
    // the corpus-wide totals answer "how many documents need this glyph".
    let text = String::from_utf8_lossy(body);
    let mut seen_here: BTreeSet<u32> = BTreeSet::new();
    for ch in text.chars() {
        seen_here.insert(ch as u32);
    }
    for cp in &seen_here {
        *code_points.entry(*cp).or_default() += 1;
    }

    let b = body;
    let mut i = 0usize;
    let mut depth = 0usize;
    let mut stack: Vec<Vec<u8>> = Vec::new();
    // Per open element: whether it has seen a child element, and whether it has
    // seen non-blank text. Both true is mixed content.
    let mut had_child: Vec<bool> = Vec::new();
    let mut had_text: Vec<bool> = Vec::new();
    let mut text_run = 0usize;

    while i < b.len() {
        if b[i] != b'<' {
            if !b[i].is_ascii_whitespace() {
                text_run += 1;
                if let Some(last) = had_text.last_mut() {
                    *last = true;
                }
            }
            i += 1;
            continue;
        }
        s.longest_text = s.longest_text.max(text_run);
        text_run = 0;

        if b[i..].starts_with(b"<!--") {
            s.comments += 1;
            i += find(&b[i..], b"-->").map_or(b.len() - i, |at| at + 3);
            continue;
        }
        if b[i..].starts_with(b"<![CDATA[") {
            s.cdata += 1;
            if let Some(last) = had_text.last_mut() {
                *last = true;
            }
            i += find(&b[i..], b"]]>").map_or(b.len() - i, |at| at + 3);
            continue;
        }
        if b[i..].starts_with(b"<!DOCTYPE") {
            s.has_doctype = true;
            i += find(&b[i..], b">").map_or(b.len() - i, |at| at + 1);
            continue;
        }
        if b[i..].starts_with(b"<?") {
            let end = find(&b[i..], b"?>").map_or(b.len() - i, |at| at + 2);
            let pi = &b[i..i + end];
            if pi.starts_with(b"<?xml ") || pi == b"<?xml?>" {
                s.has_declaration = true;
                s.declared_encoding = attr(pi, b"encoding");
            } else {
                s.processing_instructions += 1;
            }
            i += end;
            continue;
        }
        if b[i..].starts_with(b"</") {
            let end = find(&b[i..], b">").map_or(b.len() - i, |at| at + 1);
            let name = local(&b[i + 2..i + end.max(3) - 1]);
            match stack.pop() {
                Some(open) if open == name => {}
                Some(open) if s.unbalanced.is_none() => {
                    s.unbalanced = Some(format!(
                        "</{}> closes <{}>",
                        String::from_utf8_lossy(&name),
                        String::from_utf8_lossy(&open)
                    ));
                }
                None if s.unbalanced.is_none() => {
                    s.unbalanced = Some(format!(
                        "</{}> with nothing open",
                        String::from_utf8_lossy(&name)
                    ));
                }
                _ => {}
            }
            had_child.pop();
            had_text.pop();
            depth = depth.saturating_sub(1);
            i += end;
            continue;
        }

        // An opening tag.
        let end = find(&b[i..], b">").map_or(b.len() - i, |at| at + 1);
        let tag = &b[i..i + end];
        let self_closing = tag.ends_with(b"/>");
        let inner = &tag[1..tag.len().saturating_sub(if self_closing { 2 } else { 1 })];
        let name_end = inner
            .iter()
            .position(|c| c.is_ascii_whitespace())
            .unwrap_or(inner.len());
        let qname = &inner[..name_end];
        let attrs = &inner[name_end..];

        s.elements += 1;
        if s.root.is_empty() {
            s.root = String::from_utf8_lossy(qname).into_owned();
        }
        if qname.starts_with(b"xi:include") || contains(attrs, b"XInclude") {
            s.has_xinclude = true;
        }
        for (name, value) in attributes(attrs) {
            s.attributes += 1;
            if name == b"xmlns" || name.starts_with(b"xmlns:") {
                s.namespaces
                    .insert(String::from_utf8_lossy(&value).into_owned());
            }
            if name == b"xml:id" || name == b"id" {
                s.ids.push(String::from_utf8_lossy(&value).into_owned());
            }
        }
        if let Some(last) = had_child.last_mut() {
            *last = true;
        }
        if self_closing {
            s.empty_elements += 1;
        } else {
            stack.push(local(qname));
            had_child.push(false);
            had_text.push(false);
            depth += 1;
            s.max_depth = s.max_depth.max(depth);
        }
        i += end;
    }
    s.longest_text = s.longest_text.max(text_run);
    if !stack.is_empty() && s.unbalanced.is_none() {
        s.unbalanced = Some(format!(
            "{} element(s) never closed, innermost <{}>",
            stack.len(),
            String::from_utf8_lossy(stack.last().expect("non-empty"))
        ));
    }

    // Entity references that are not the five XML defines and not numeric.
    let mut at = 0usize;
    while let Some(amp) = find(&b[at..], b"&") {
        let from = at + amp + 1;
        let end = b[from..]
            .iter()
            .position(|c| *c == b';' || c.is_ascii_whitespace() || *c == b'<')
            .unwrap_or(0);
        if end > 0 && b.get(from + end) == Some(&b';') {
            let name = &b[from..from + end];
            let known = matches!(name, b"amp" | b"lt" | b"gt" | b"quot" | b"apos");
            if !known && !name.starts_with(b"#") {
                s.entity_refs
                    .insert(String::from_utf8_lossy(name).into_owned());
            }
        }
        at = from;
    }

    // Mixed content is decided while walking, but the flags are popped with the
    // element, so it is recorded here from whether any element saw both.
    s.mixed_content = mixed(b);
    s
}

/// A second, cheaper pass for mixed content: an element with both a child
/// element and non-blank text directly inside it.
fn mixed(b: &[u8]) -> bool {
    let mut depth_has_child = Vec::new();
    let mut depth_has_text = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        if b[i] != b'<' {
            if !b[i].is_ascii_whitespace() {
                if let Some(last) = depth_has_text.last_mut() {
                    *last = true;
                }
            }
            i += 1;
            continue;
        }
        if b[i..].starts_with(b"<!--") {
            i += find(&b[i..], b"-->").map_or(b.len() - i, |at| at + 3);
            continue;
        }
        if b[i..].starts_with(b"<![CDATA[") {
            if let Some(last) = depth_has_text.last_mut() {
                *last = true;
            }
            i += find(&b[i..], b"]]>").map_or(b.len() - i, |at| at + 3);
            continue;
        }
        if b[i..].starts_with(b"<?") || b[i..].starts_with(b"<!") {
            i += find(&b[i..], b">").map_or(b.len() - i, |at| at + 1);
            continue;
        }
        let end = find(&b[i..], b">").map_or(b.len() - i, |at| at + 1);
        if b[i..].starts_with(b"</") {
            let child = depth_has_child.pop().unwrap_or(false);
            let text = depth_has_text.pop().unwrap_or(false);
            if child && text {
                return true;
            }
        } else if !b[i..i + end].ends_with(b"/>") {
            if let Some(last) = depth_has_child.last_mut() {
                *last = true;
            }
            depth_has_child.push(false);
            depth_has_text.push(false);
        } else if let Some(last) = depth_has_child.last_mut() {
            *last = true;
        }
        i += end;
    }
    false
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    find(hay, needle).is_some()
}

/// The part of a qualified name after the colon.
fn local(qname: &[u8]) -> Vec<u8> {
    match qname.iter().position(|c| *c == b':') {
        Some(at) => qname[at + 1..].to_vec(),
        None => qname.to_vec(),
    }
}

/// One attribute value out of a tag's attribute text.
fn attr(tag: &[u8], name: &[u8]) -> Option<String> {
    attributes(tag)
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| String::from_utf8_lossy(&v).into_owned())
}

/// Every `name="value"` in a tag's attribute text.
fn attributes(mut rest: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut out = Vec::new();
    while let Some(eq) = rest.iter().position(|c| *c == b'=') {
        let name: Vec<u8> = rest[..eq]
            .iter()
            .copied()
            .filter(|c| !c.is_ascii_whitespace())
            .collect();
        let after = &rest[eq + 1..];
        let Some(open) = after.iter().position(|c| *c == b'"' || *c == b'\'') else {
            break;
        };
        let quote = after[open];
        let Some(close) = after[open + 1..].iter().position(|c| *c == quote) else {
            break;
        };
        out.push((name, after[open + 1..open + 1 + close].to_vec()));
        rest = &after[open + 1 + close + 1..];
    }
    out
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let Some(zip) = args.next().map(PathBuf::from) else {
        eprintln!("usage: corpus_inventory <archive.zip> [report.json]");
        std::process::exit(2);
    };
    let report = args.next().map(PathBuf::from);

    let before = aruna::md5::md5_file(&zip).expect("read archive");
    let file = std::fs::File::open(&zip).expect("open archive");
    let mut archive = zip::ZipArchive::new(std::io::BufReader::with_capacity(1 << 18, file))
        .expect("read archive");

    let mut code_points: HashMap<u32, usize> = HashMap::new();
    let mut sizes: Vec<usize> = Vec::new();
    let mut by_name: HashMap<String, Vec<String>> = HashMap::new();
    let mut by_lower: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut roots: BTreeMap<String, usize> = BTreeMap::new();
    let mut encodings: BTreeMap<String, usize> = BTreeMap::new();
    let mut namespaces: BTreeMap<String, usize> = BTreeMap::new();
    let mut entities: BTreeMap<String, usize> = BTreeMap::new();
    let mut series: BTreeMap<String, usize> = BTreeMap::new();
    let mut ids: HashMap<String, usize> = HashMap::new();

    let (mut manuscripts, mut skipped, mut with_bom, mut with_decl) =
        (0usize, 0usize, 0usize, 0usize);
    let (mut with_comments, mut with_pis, mut with_cdata, mut with_doctype) =
        (0, 0usize, 0usize, 0usize);
    let (mut with_xinclude, mut with_mixed, mut with_ns, mut with_entities) =
        (0usize, 0usize, 0usize, 0usize);
    let mut unbalanced: Vec<(String, String)> = Vec::new();
    let mut unbalanced_total = 0usize;
    let mut deepest = (0usize, String::new());
    let mut widest = (0usize, String::new());
    let mut longest_text = (0usize, String::new());
    let mut total_elements = 0usize;
    let mut total_attributes = 0usize;
    let mut bytes = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).expect("entry");
        let name = entry.name().to_string();
        if !aruna::parse::is_manuscript_xml(&name) {
            if name.to_ascii_lowercase().ends_with(".xml") {
                skipped += 1;
            }
            continue;
        }
        bytes.clear();
        entry.read_to_end(&mut bytes).expect("read entry");
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(16 * 1024)]);
        if !aruna::parse::looks_like_manuscript(&head) {
            skipped += 1;
            continue;
        }
        manuscripts += 1;

        let s = shape_of(&bytes, &mut code_points);
        sizes.push(s.bytes);
        total_elements += s.elements;
        total_attributes += s.attributes;
        if s.has_bom {
            with_bom += 1;
        }
        if s.has_declaration {
            with_decl += 1;
        }
        if s.comments > 0 {
            with_comments += 1;
        }
        if s.processing_instructions > 0 {
            with_pis += 1;
        }
        if s.cdata > 0 {
            with_cdata += 1;
        }
        if s.has_doctype {
            with_doctype += 1;
        }
        if s.has_xinclude {
            with_xinclude += 1;
        }
        if s.mixed_content {
            with_mixed += 1;
        }
        if !s.namespaces.is_empty() {
            with_ns += 1;
        }
        if !s.entity_refs.is_empty() {
            with_entities += 1;
        }
        if s.max_depth > deepest.0 {
            deepest = (s.max_depth, name.clone());
        }
        if s.elements > widest.0 {
            widest = (s.elements, name.clone());
        }
        if s.longest_text > longest_text.0 {
            longest_text = (s.longest_text, name.clone());
        }
        if let Some(why) = &s.unbalanced {
            unbalanced_total += 1;
            // Only the first few are worth printing; the count is the number
            // that matters and is kept separately, because reporting the length
            // of a truncated list as a total is how a cap becomes a lie.
            if unbalanced.len() < 5 {
                unbalanced.push((name.clone(), why.clone()));
            }
        }
        *roots.entry(s.root.clone()).or_default() += 1;
        *encodings
            .entry(
                s.declared_encoding
                    .clone()
                    .unwrap_or_else(|| "<none>".into()),
            )
            .or_default() += 1;
        for ns in &s.namespaces {
            *namespaces.entry(ns.clone()).or_default() += 1;
        }
        for e in &s.entity_refs {
            *entities.entry(e.clone()).or_default() += 1;
        }
        for id in &s.ids {
            *ids.entry(id.clone()).or_default() += 1;
        }

        let base = name.rsplit('/').next().unwrap_or(&name).to_string();
        by_name.entry(base.clone()).or_default().push(name.clone());
        by_lower
            .entry(base.to_lowercase())
            .or_default()
            .insert(base.clone());
        let siglum_series: String = base
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        *series
            .entry(if siglum_series.is_empty() {
                "<none>".into()
            } else {
                siglum_series
            })
            .or_default() += 1;
    }

    sizes.sort_unstable();
    let total: usize = sizes.iter().sum();
    let at = |q: f64| sizes[((sizes.len() as f64 - 1.0) * q) as usize];

    println!("archive:              {}", zip.display());
    println!("  digest (md5):       {before}");
    println!();
    println!("documents:            {manuscripts}");
    println!("  .xml the gates refused: {skipped}");
    println!(
        "  total bytes:        {total} ({:.1} MB)",
        total as f64 / 1e6
    );
    println!(
        "  size min/p50/p95/max: {} / {} / {} / {}",
        sizes.first().copied().unwrap_or(0),
        at(0.50),
        at(0.95),
        sizes.last().copied().unwrap_or(0)
    );
    println!("  elements:           {total_elements}");
    println!("  attributes:         {total_attributes}");
    println!();
    println!("structure, by number of documents carrying it:");
    println!("  BOM:                {with_bom}");
    println!("  XML declaration:    {with_decl}");
    println!("  DOCTYPE / DTD:      {with_doctype}");
    println!("  processing instr.:  {with_pis}");
    println!("  comments:           {with_comments}");
    println!("  CDATA:              {with_cdata}");
    println!("  namespaces:         {with_ns}");
    println!("  entity references:  {with_entities}");
    println!("  XInclude:           {with_xinclude}");
    println!("  mixed content:      {with_mixed}");
    println!();
    println!("extremes:");
    println!("  deepest:            {} levels — {}", deepest.0, deepest.1);
    println!("  most elements:      {} — {}", widest.0, widest.1);
    println!(
        "  longest text run:   {} bytes — {}",
        longest_text.0, longest_text.1
    );
    println!();
    println!("root elements:        {roots:?}");
    println!("declared encodings:   {encodings:?}");
    println!("namespaces:           {namespaces:?}");
    if !entities.is_empty() {
        println!("entity references:    {entities:?}");
    }
    println!();
    let dup_names: Vec<_> = by_name.iter().filter(|(_, v)| v.len() > 1).collect();
    let case_collisions: Vec<_> = by_lower.iter().filter(|(_, v)| v.len() > 1).collect();
    let dup_ids = ids.values().filter(|c| **c > 1).count();
    println!("collisions:");
    println!("  same file name in different folders: {}", dup_names.len());
    println!(
        "  names differing only by case:        {}",
        case_collisions.len()
    );
    println!("  ids used by more than one element:   {dup_ids}");
    println!(
        "  documents that do not balance:       {unbalanced_total} (first {} shown)",
        unbalanced.len()
    );
    for (name, why) in unbalanced.iter() {
        println!("    {name}: {why}");
    }
    println!();
    println!("series (leading letters of the file name):");
    let mut top: Vec<_> = series.iter().collect();
    top.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    for (s, c) in top.iter().take(12) {
        println!("  {s:<10} {c}");
    }
    println!();
    println!("distinct code points in the corpus: {}", code_points.len());

    let after = aruna::md5::md5_file(&zip).expect("re-read archive");
    println!();
    println!(
        "archive after the scan: {after}   {}",
        if before == after {
            "unchanged"
        } else {
            "CHANGED — the scan wrote to it"
        }
    );

    if let Some(path) = report {
        let mut json = String::from("{\n");
        json.push_str(&format!("  \"documents\": {manuscripts},\n"));
        json.push_str(&format!("  \"bytes\": {total},\n"));
        json.push_str(&format!("  \"elements\": {total_elements},\n"));
        json.push_str(&format!("  \"attributes\": {total_attributes},\n"));
        json.push_str(&format!("  \"max_depth\": {},\n", deepest.0));
        json.push_str("  \"code_points\": {\n");
        let mut cps: Vec<_> = code_points.iter().collect();
        cps.sort();
        for (i, (cp, count)) in cps.iter().enumerate() {
            let comma = if i + 1 == cps.len() { "" } else { "," };
            json.push_str(&format!("    \"U+{cp:04X}\": {count}{comma}\n"));
        }
        json.push_str("  }\n}\n");
        std::fs::write(&path, json).expect("write report");
        println!("full code-point table written to {}", path.display());
    }
    assert_eq!(before, after, "the archive was modified");
}
