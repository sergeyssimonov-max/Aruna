//! Shared instruments for the integration tests that need to read what the
//! program wrote rather than take its word for it.
//!
//! Nothing here is a fixture factory for its own sake: each piece exists
//! because a test below has to inspect an artefact the crate produces in a
//! format the crate does not read back. The JSON reader is the main one — the
//! catalog is hand-written by `catalog::render` and hand-read by a Node script
//! in another language, so the seam between them has no Rust reader at all,
//! and a test that checked the document by substring would pass on a document
//! no parser accepts.
//!
//! Compiled into more than one test binary, so not every item is used by every
//! one of them.
#![allow(dead_code)]

use std::fmt;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// The archive, obtained without asking Zenodo anything
// ---------------------------------------------------------------------------

/// The release lookup these tests hand to
/// [`aruna::obtain_archive_advised_by`], in place of the one that reaches
/// zenodo.org.
///
/// It answers what a current, unmodified record answers: the record this build
/// is pinned to, published with the digest this build expects. `zenodo::advice`
/// finds nothing to say about that, so a test sees the event stream of an
/// up-to-date run rather than one being told the corpus has moved — which is
/// what these tests were seeing before, whenever Zenodo was reachable enough to
/// say it.
pub fn current_record(record_id: u64) -> aruna::error::Result<aruna::zenodo::Release> {
    Ok(aruna::zenodo::Release {
        record_id,
        file: "TLHbasisONLINE25_1_ZENODO_Beta_03.zip".to_string(),
        md5: Some(aruna::download::ZENODO_ZIP_MD5.to_string()),
        published: None,
    })
}

/// [`aruna::obtain_archive`] with Zenodo left out of it.
///
/// The plain function asks the API which edition of the corpus is current
/// before every download. That is right for a run and wrong for a test: the
/// origin here is a local server, and the question would go to zenodo.org all
/// the same — a live request from a suite that states it makes none, invisible
/// in the local server's own request count, and ten seconds of nothing on a day
/// the API is slow.
pub fn obtain_archive(
    url: &str,
    md5: &str,
    job: &aruna::job::Job<'_>,
) -> aruna::error::Result<aruna::cache::Archive> {
    aruna::obtain_archive_advised_by(url, md5, job, current_record)
}

// ---------------------------------------------------------------------------
// A strict reader for the one JSON document this crate writes
// ---------------------------------------------------------------------------

/// A JSON value, in the shapes this crate's documents are allowed to contain.
///
/// No floats. That is not a shortcut: nothing this program writes has a
/// fractional number in it — the catalog holds counts and pool indices, the
/// manifest holds counts and code points — so one appearing is a change to a
/// format worth failing on rather than a value to round.
///
/// Booleans and nulls are read but are not welcome everywhere. The manifest
/// uses `false`; the catalog uses neither, and
/// `the_catalog_holds_no_booleans_or_nulls` in `catalog_contract.rs` is what
/// says so — the shape a document is allowed to have belongs to the test for
/// that document, not to the reader, which would otherwise be a different
/// reader for every file.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Str(String),
    Int(i64),
    Bool(bool),
    Null,
    Arr(Vec<Json>),
    /// Pairs rather than a map: duplicate keys are a defect this reader must be
    /// able to see, and a map would silently keep the last one.
    Obj(Vec<(String, Json)>),
}

/// Where the document stopped making sense, and why.
#[derive(Debug, PartialEq)]
pub struct JsonError {
    pub at: usize,
    pub what: String,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "byte {}: {}", self.at, self.what)
    }
}

impl Json {
    /// Parse a whole document. Trailing bytes are an error rather than ignored.
    pub fn parse(text: &str) -> Result<Json, JsonError> {
        let mut p = Parser {
            bytes: text.as_bytes(),
            at: 0,
        };
        let value = p.value()?;
        p.spaces();
        if p.at != p.bytes.len() {
            return Err(p.err("trailing bytes after the document"));
        }
        Ok(value)
    }

    /// The value under `key`, if this is an object that has one.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The keys, in the order the document wrote them.
    pub fn keys(&self) -> Vec<&str> {
        match self {
            Json::Obj(pairs) => pairs.iter().map(|(k, _)| k.as_str()).collect(),
            _ => Vec::new(),
        }
    }

    pub fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Json::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Every value in the document, this one included, in reading order.
    ///
    /// For assertions about a whole document rather than about one field —
    /// "nothing anywhere in here is a boolean" is a sentence about the format,
    /// and checking it by hand at every key is how a new key escapes it.
    pub fn walk(&self) -> Vec<&Json> {
        let mut out = vec![self];
        match self {
            Json::Arr(items) => out.extend(items.iter().flat_map(Json::walk)),
            Json::Obj(pairs) => out.extend(pairs.iter().flat_map(|(_, v)| v.walk())),
            _ => {}
        }
        out
    }

    /// A short name for the shape, for assertion messages.
    pub fn kind(&self) -> &'static str {
        match self {
            Json::Str(_) => "string",
            Json::Int(_) => "integer",
            Json::Bool(_) => "boolean",
            Json::Null => "null",
            Json::Arr(_) => "array",
            Json::Obj(_) => "object",
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Parser<'a> {
    fn err(&self, what: &str) -> JsonError {
        JsonError {
            at: self.at,
            what: what.to_string(),
        }
    }

    fn spaces(&mut self) {
        while matches!(self.bytes.get(self.at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn eat(&mut self, byte: u8) -> Result<(), JsonError> {
        if self.peek() == Some(byte) {
            self.at += 1;
            return Ok(());
        }
        Err(self.err(&format!("expected {:?}", byte as char)))
    }

    fn value(&mut self) -> Result<Json, JsonError> {
        self.spaces();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b'-' | b'0'..=b'9') => self.integer(),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(_) => Err(self.err("not the start of a value this format allows")),
            None => Err(self.err("the document ends where a value should start")),
        }
    }

    fn object(&mut self) -> Result<Json, JsonError> {
        self.eat(b'{')?;
        let mut pairs = Vec::new();
        self.spaces();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Obj(pairs));
        }
        loop {
            self.spaces();
            let key = self.string()?;
            self.spaces();
            self.eat(b':')?;
            let value = self.value()?;
            pairs.push((key, value));
            self.spaces();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Json::Obj(pairs));
                }
                _ => return Err(self.err("expected ',' or '}' after a member")),
            }
        }
    }

    fn array(&mut self) -> Result<Json, JsonError> {
        self.eat(b'[')?;
        let mut items = Vec::new();
        self.spaces();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.value()?);
            self.spaces();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(self.err("expected ',' or ']' after an element")),
            }
        }
    }

    /// A JSON string, with the escapes the catalog's writer can produce.
    fn string(&mut self) -> Result<String, JsonError> {
        self.eat(b'"')?;
        let mut out = String::new();
        loop {
            let Some(byte) = self.peek() else {
                return Err(self.err("the document ends inside a string"));
            };
            self.at += 1;
            match byte {
                b'"' => return Ok(out),
                b'\\' => {
                    let Some(esc) = self.peek() else {
                        return Err(self.err("the document ends inside an escape"));
                    };
                    self.at += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.unicode_escape()?),
                        _ => return Err(self.err("unknown escape")),
                    }
                }
                // A raw control character inside a string is what the writer's
                // escaping exists to prevent, so seeing one here is a finding
                // rather than something to accept quietly.
                0x00..=0x1f => return Err(self.err("raw control character inside a string")),
                _ => {
                    // Continuation bytes of one UTF-8 sequence travel together;
                    // the input is a `&str`, so the boundaries are known good.
                    let start = self.at - 1;
                    while self.peek().is_some_and(|b| b & 0xc0 == 0x80) {
                        self.at += 1;
                    }
                    out.push_str(
                        std::str::from_utf8(&self.bytes[start..self.at])
                            .map_err(|_| self.err("invalid UTF-8 inside a string"))?,
                    );
                }
            }
        }
    }

    fn unicode_escape(&mut self) -> Result<char, JsonError> {
        let hex = self
            .bytes
            .get(self.at..self.at + 4)
            .ok_or_else(|| self.err("truncated \\u escape"))?;
        let text = std::str::from_utf8(hex).map_err(|_| self.err("\\u escape is not ASCII"))?;
        let code = u32::from_str_radix(text, 16).map_err(|_| self.err("\\u escape is not hex"))?;
        self.at += 4;
        // No surrogate pairing: the writer escapes only control characters, so
        // a surrogate here means the format changed and the test should say so.
        char::from_u32(code).ok_or_else(|| self.err("\\u escape is not a character"))
    }

    /// One of the three bare words JSON has, and nothing that merely starts
    /// like one.
    fn literal(&mut self, word: &str, value: Json) -> Result<Json, JsonError> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            return Ok(value);
        }
        Err(self.err("not a value this format allows"))
    }

    fn integer(&mut self) -> Result<Json, JsonError> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.at += 1;
        }
        if self.at == start {
            return Err(self.err("expected a number"));
        }
        // A fraction or an exponent is a value this format does not use; the
        // reader refuses rather than rounding it into an integer.
        if matches!(self.peek(), Some(b'.' | b'e' | b'E')) {
            return Err(self.err("fractional or exponential number"));
        }
        std::str::from_utf8(&self.bytes[start..self.at])
            .ok()
            .and_then(|t| t.parse::<i64>().ok())
            .map(Json::Int)
            .ok_or_else(|| self.err("number does not fit an i64"))
    }
}

// ---------------------------------------------------------------------------
// Archives
// ---------------------------------------------------------------------------

/// A manuscript in the shape the corpus really has one.
pub fn manuscript(siglum: &str, editor: &str, date: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><AOxml xml:space="preserve"><AOHeader><docID>{siglum}</docID><meta><uebern editor="{editor}" date="{date}"/></meta></AOHeader><body><text><l lg="Hit"/>text</text></body></AOxml>"#
    )
}

/// A ZIP holding `entries`, written to `path`.
pub fn archive(path: &Path, entries: &[(&str, String)]) -> PathBuf {
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;

    let file = std::fs::File::create(path).expect("create archive");
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, body) in entries {
        zip.start_file(*name, options).expect("start entry");
        zip.write_all(body.as_bytes()).expect("write entry");
    }
    zip.finish().expect("finish archive");
    path.to_path_buf()
}

/// An archive with the awkward shapes the corpus has: one group filed under two
/// folders and not adjacent to itself, a repeated siglum, a siglum with a
/// slash, and metadata that repeats so the catalog's pool has something to
/// deduplicate.
pub fn mixed_archive(dir: &Path) -> PathBuf {
    archive(
        &dir.join("corpus.zip"),
        &[
            (
                "root/CTH 5_XML_HFR/KBo 1.1.xml",
                manuscript("KBo 1.1", "FB", "2017-03-28"),
            ),
            (
                "root/CTH 9_XML_HFR/KUB 2.1.xml",
                manuscript("KUB 2.1", "GM", "2019-01-02"),
            ),
            (
                "root/CTH 5_XML_TLH/KBo 1.1.xml",
                manuscript("KBo 1.1", "FB", "2017-03-28"),
            ),
            (
                "root/CTH 5_XML_HFR/544-f.xml",
                manuscript("544/f", "FB", "2017-03-28"),
            ),
            (
                "root/CTH 9_XML_TLH/KUB 2.2.xml",
                manuscript("KUB 2.2", "GM", "2019-01-02"),
            ),
            // Debris of each kind the gates turn away, so a run over this
            // archive exercises both halves of the skipped-entry report:
            // `style.css` is not counted at all (it never claimed to be a
            // manuscript), `__MACOSX` is refused by its path, and `notes.xml`
            // is refused by what is inside it.
            ("root/style.css", "body{}".to_string()),
            (
                "root/__MACOSX/CTH 5_XML_HFR/._KBo 1.1.xml",
                "\u{0}\u{5}".to_string(),
            ),
            (
                "root/CTH 5_XML_HFR/notes.xml",
                "<html><body>not a manuscript</body></html>".to_string(),
            ),
        ],
    )
}

// ---------------------------------------------------------------------------
// A local origin that can answer more than one client at a time
// ---------------------------------------------------------------------------

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// How long a gated origin will wait for the clients it was told to expect.
///
/// A deadline rather than a synchronisation device: when every expected client
/// does arrive — which is the case the gate exists for — it is never reached,
/// and the clients are released the instant the last one connects. It is here
/// so a test that produces fewer clients than it meant to fails on its own
/// assertion instead of hanging.
const GATE_DEADLINE: Duration = Duration::from_secs(10);

/// A server that hands out one body and counts what it was asked for.
///
/// `cache_lifecycle.rs` has one of these already; this one differs in the
/// property the tests here are about — it answers each connection on its own
/// thread, so two clients that arrive together are actually served together.
/// A sequential accept loop would serialise exactly the overlap under test and
/// the assertions would pass without ever having produced it.
///
/// It also records how many connections were open at once, which is the only
/// way to tell "both ran and one waited" from "both ran at the same time".
pub struct Origin {
    port: u16,
    hits: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    live: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    listening: Option<std::thread::JoinHandle<()>>,
}

impl Origin {
    /// Serve `body` to everyone, with a matching `Content-Length`.
    pub fn serving(body: Vec<u8>) -> Self {
        Self::start(body, 0)
    }

    /// As [`Origin::serving`], but answer nobody until `gate` clients have
    /// connected.
    ///
    /// Overlap that is left to the scheduler is not overlap a test can assert
    /// on: half a megabyte over loopback finishes faster than the next thread
    /// starts, so six genuinely concurrent runs are served one after another
    /// and a peak of one proves nothing either way. Holding the first `gate`
    /// connections open until all of them exist makes the overlap a fact of the
    /// test rather than a hope about the machine.
    pub fn gated(body: Vec<u8>, gate: usize) -> Self {
        Self::start(body, gate)
    }

    fn start(body: Vec<u8>, gate: usize) -> Self {
        // Port 0: the operating system picks a free one, so tests running side
        // by side cannot collide over a number.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let hits = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let live = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));

        let (h, p, l, s) = (
            Arc::clone(&hits),
            Arc::clone(&peak),
            Arc::clone(&live),
            Arc::clone(&stop),
        );
        let body = Arc::new(body);
        // (how many have arrived, released) — the gate below waits on it.
        let arrivals = Arc::new((Mutex::new(0usize), Condvar::new()));
        let listening = std::thread::spawn(move || {
            let mut workers = Vec::new();
            for stream in listener.incoming() {
                if s.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else { break };
                h.fetch_add(1, Ordering::SeqCst);
                let now = l.fetch_add(1, Ordering::SeqCst) + 1;
                p.fetch_max(now, Ordering::SeqCst);
                let body = Arc::clone(&body);
                let l = Arc::clone(&l);
                let arrivals = Arc::clone(&arrivals);
                workers.push(std::thread::spawn(move || {
                    // Read the request line so the client is not writing into a
                    // socket nobody is reading.
                    let mut scratch = [0u8; 1024];
                    let _ = stream.read(&mut scratch);
                    if gate > 1 {
                        let (count, ready) = &*arrivals;
                        let mut here = count.lock().expect("not poisoned");
                        *here += 1;
                        if *here >= gate {
                            ready.notify_all();
                        } else {
                            let _ = ready
                                .wait_timeout_while(here, GATE_DEADLINE, |n| *n < gate)
                                .expect("not poisoned");
                        }
                    }
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(head.as_bytes());
                    let _ = stream.write_all(&body);
                    let _ = stream.flush();
                    l.fetch_sub(1, Ordering::SeqCst);
                }));
            }
            // Joined rather than detached: a test that ends with worker threads
            // still writing is a test that leaks one.
            for worker in workers {
                let _ = worker.join();
            }
        });

        Origin {
            port,
            hits,
            peak,
            live,
            stop,
            listening: Some(listening),
        }
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/TLHbasis.zip", self.port)
    }

    /// How many requests have arrived.
    pub fn requests(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }

    /// The most connections that were ever open at the same time.
    pub fn concurrent_peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }

    /// How many are open now — zero once every client has been served.
    pub fn live_connections(&self) -> usize {
        self.live.load(Ordering::SeqCst)
    }
}

impl Drop for Origin {
    /// Stop accepting and join, so no listening socket and no worker thread
    /// outlives the test that started them.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // The accept loop is blocked in `accept`; one connection wakes it so it
        // can see the flag. Failure is fine — it means it is already gone.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(handle) = self.listening.take() {
            let _ = handle.join();
        }
    }
}

/// How many file descriptors this process holds open.
///
/// `/dev/fd` lists them on macOS and Linux alike. Reading the directory opens
/// one itself, which is why the number is only ever compared with another
/// reading taken the same way.
pub fn open_descriptors() -> usize {
    std::fs::read_dir("/dev/fd")
        .map(|entries| entries.count())
        .unwrap_or(0)
}
