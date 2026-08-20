//! Scratch: hammer the pure naming functions with hostile input.
use aruna::export::naming::{href, output_path, path_component, pdf_path, percent_decode, resolve};

fn main() {
    let seeds: Vec<String> = vec![
        "".into(),
        ".".into(),
        "..".into(),
        "...".into(),
        "   ".into(),
        " . . ".into(),
        "%".into(),
        "%2".into(),
        "%2F".into(),
        "%aé".into(),
        "%éa".into(),
        "a%é".into(),
        "%%".into(),
        "%FF".into(),
        "%00".into(),
        "%2e%2e".into(),
        "./..".into(),
        "KBo 3.22".into(),
        "Bo 2023/23".into(),
        "544/f".into(),
        "93/w+".into(),
        "a\u{7f}b".into(),
        "a\u{1}b".into(),
        "𒀀".into(),
        "é".into(),
        "n\u{0301}".into(),
        "CON".into(),
        "a:b".into(),
        "a\\b".into(),
        "a/b/c".into(),
        "\u{feff}x".into(),
        "x ".into(),
        "x.".into(),
        "x. .".into(),
        "-".into(),
        "~".into(),
        "_".into(),
    ];
    println!("inputs: every seed above, and every pair of them — fixed, not random");
    let mut inputs = seeds.clone();
    for a in &seeds {
        for b in &seeds {
            inputs.push(format!("{a}{b}"));
        }
    }

    for s in &inputs {
        let c = path_component(s);
        assert!(!c.is_empty(), "path_component({s:?}) is empty");
        assert!(
            !c.contains('/'),
            "path_component({s:?}) = {c:?} keeps a slash"
        );
        assert!(
            !c.starts_with('.'),
            "path_component({s:?}) = {c:?} is hidden"
        );
        assert!(c != "." && c != "..", "path_component({s:?}) = {c:?}");

        let p = output_path(s, s);
        assert_eq!(p.components().count(), 2, "output_path({s:?}) = {p:?}");
        assert!(pdf_path(&p).extension().unwrap() == "pdf");

        let h = href(&p);
        assert!(h.is_ascii(), "href({p:?}) = {h:?} is not ascii");
        let back = resolve(&h).unwrap_or_else(|| panic!("resolve({h:?}) refused its own href"));
        assert_eq!(back, p, "round trip broke for {s:?}");

        // percent_decode must survive anything, not just what we generated.
        let _ = percent_decode(s);
        let _ = resolve(s);
        let _ = resolve(&format!("./{s}"));
    }
    // Deliberately hostile hrefs.
    for bad in [
        "./..",
        "./../x",
        "/etc/passwd",
        "http://x/y",
        "./",
        "..",
        "./%2e%2e/x",
        "./a//b",
    ] {
        if let Some(p) = resolve(bad) {
            assert!(
                !p.to_string_lossy().contains(".."),
                "resolve({bad:?}) escaped: {p:?}"
            );
            assert!(p.is_relative(), "resolve({bad:?}) is absolute: {p:?}");
        }
    }
    println!("--- ok, {} inputs ---", inputs.len());
}
