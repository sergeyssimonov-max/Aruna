//! The document model: what one manuscript says, as XML defines what it says.
//!
//! **What the model is.** The XML Information Set of the document, as a
//! conforming processor reports it — not the bytes, and not anything read into
//! them. Three consequences, each decided rather than assumed, because the
//! corpus exercises every one of them:
//!
//! - line ends are normalised (`\r\n` and `\r` read as `\n`) — 6 111 documents
//!   of TLHdig Beta 0.3 carry a carriage return;
//! - attribute values are normalised (a tab, carriage return or line feed in a
//!   value reads as a space) — 11 documents;
//! - the five predefined entities and character references are replaced by the
//!   characters they stand for, and adjacent character data is one text node.
//!
//! That is not a value supplied for the author. It is what the XML specification
//! says a reader of these bytes has read, and it is what the independent second
//! opinion named in `docs/PDF-ACCEPTANCE.md` §1–2, `xsltproc`, reports — so the
//! model is checked against it node for node (`tests/document_model.rs`).
//! Counted on 2026-09-13.
//!
//! **What the model is not.** It holds no field that is not in the document.
//! There is no siglum, no CTH number, no editor and no year here: the inventory
//! fields of [`crate::parse`] are derived — from windows of text, from folder
//! names, with `—` standing in for what is missing — and a derived value that
//! looked like a read one is exactly what this model must not carry. A later
//! stage derives them *from* the model and says so. What is absent is absent: an
//! attribute the tag does not carry is not in [`Element::attributes`], a
//! declaration the document does not open with is `None`, an unprefixed name has
//! no namespace.
//!
//! One thing the model does add, and it is named: [`Name::namespace`]. It is the
//! URI a prefix is bound to by a declaration *in the same document*, which is
//! what *Namespaces in XML* says the name means; `docs/XML-CONTRACT.md` §4 asks
//! for qualified names to be resolved before layout. The declarations it comes
//! from stay in the model as written, in [`Element::namespaces`].
//!
//! What is not kept, because XML does not count it as content: whether an empty
//! element was written `<a/>` or `<a></a>`, the quote character around a value,
//! whitespace between attributes, and whitespace outside the root element.
//!
//! **Order is document order, by construction.** [`Document::nodes`] is one
//! vector, filled in the order the parser meets the nodes; attributes and
//! namespace declarations are vectors filled in the order they stand in the tag.
//! No map, no sort, no insertion order other than reading order. A subtree is a
//! contiguous run of that vector — [`Node::end`] says where it stops — so the
//! structure needs no pointers and the model has no recursive destructor for a
//! deep document to overflow.
//!
//! **Held once.** The model borrows from the document's text wherever the text
//! is already what the model has to say, and allocates only where the XML
//! specification changes it (a normalised line end, a resolved reference). The
//! reader is a pull parser over that same text; no second copy is made.
//!
//! **Which documents it refuses.** Everything `quick-xml` refuses — the 206
//! documents named in the manifest, with the reason [`xml_wellformed::classify`]
//! gives, not a second classification — and the seventeen it accepts that XML or
//! *Namespaces in XML* forbids ([`xml_wellformed::beyond_the_parser`]).
//! `docs/XML-CONTRACT.md` §5 sends those seventeen to the same place as the 206:
//! refused and reported. Beyond those, it refuses what this project has no
//! policy for yet ([`Undecided`]) instead of choosing one in passing.
//!
//! **This module knows nothing about the output.** No layout, no page, no markup
//! language of a rendering: the boundary is specification §3.8 and
//! `docs/PDF-ACCEPTANCE.md`, "The boundary to keep", and
//! `the_model_names_no_output_format` below holds it.

use std::borrow::Cow;

use quick_xml::escape::{resolve_predefined_entity, EscapeError};
use quick_xml::events::attributes::Attribute as RawAttribute;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::{PrefixDeclaration, QName, ResolveResult};
use quick_xml::{NsReader, XmlVersion};

use crate::xml_wellformed::{self, Beyond, Finding};

/// The namespace the `xml` prefix is bound to without being declared.
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

/// One document, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document<'s> {
    declaration: Option<Declaration<'s>>,
    nodes: Vec<Node<'s>>,
    root: usize,
}

/// The XML declaration, as written. Only what it carries: `encoding` and
/// `standalone` are `None` when the declaration does not name them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration<'s> {
    pub version: Cow<'s, str>,
    pub encoding: Option<Cow<'s, str>>,
    pub standalone: Option<Cow<'s, str>>,
}

/// One node, and where its subtree ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node<'s> {
    /// The element this node is inside, or `None` at the top of the document —
    /// the root element, and any comment or instruction before or after it.
    pub parent: Option<usize>,
    /// One past the last node of this node's subtree. A node without children
    /// ends right after itself.
    pub end: usize,
    pub kind: Kind<'s>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind<'s> {
    Element(Element<'s>),
    /// Character data, with every adjacent piece joined — text, references and
    /// CDATA sections alike, as the Information Set has one text node for them.
    Text(Cow<'s, str>),
    Comment(Cow<'s, str>),
    /// A processing instruction. `data` is everything after the target and the
    /// whitespace that separates them, and is empty when there is nothing.
    Instruction {
        target: Cow<'s, str>,
        data: Cow<'s, str>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element<'s> {
    pub name: Name<'s>,
    /// The `xmlns` and `xmlns:…` attributes of this tag, in the order written.
    /// Kept apart from [`Element::attributes`] because XML does not count them
    /// as attributes: they bind prefixes, and a reader asking for an element's
    /// attributes does not get them.
    pub namespaces: Vec<Binding<'s>>,
    /// Every other attribute, in the order written.
    pub attributes: Vec<Attribute<'s>>,
}

/// A qualified name as written, and the namespace it is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name<'s> {
    pub prefix: Option<Cow<'s, str>>,
    pub local: Cow<'s, str>,
    /// `None` when the name is not in a namespace: an unprefixed attribute, or
    /// an unprefixed element with no default namespace in scope.
    pub namespace: Option<Cow<'s, str>>,
}

/// `xmlns="uri"` (no prefix) or `xmlns:prefix="uri"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding<'s> {
    pub prefix: Option<Cow<'s, str>>,
    pub uri: Cow<'s, str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute<'s> {
    pub name: Name<'s>,
    pub value: Cow<'s, str>,
}

/// Why a document has no model.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// The bytes are not UTF-8. No document of this corpus declares anything
    /// else, and reading other bytes as UTF-8 would invent characters.
    #[error("not UTF-8 at line {line}, column {column}")]
    NotUtf8 { line: usize, column: usize },
    /// `quick-xml` refused it — one of the documents the manifest names, with
    /// the classifier's reason.
    ///
    /// Line and column are counted in the bytes handed to [`Document::read`].
    /// Read from the archive, that is one line off the manifest for a document
    /// that gets a declaration written in front of it — 23 494 of them — since
    /// the manifest counts in the package copy.
    #[error("unread: {} at line {}, column {}", .0.reason.key(), .0.line, .0.column)]
    Unread(Finding),
    /// Accepted by the parser and forbidden by XML or by *Namespaces in XML*.
    #[error("beyond this parser: {} at line {}, column {}", .0.limit.key(), .0.line, .0.column)]
    BeyondTheParser(Beyond),
    /// The parser refused and the classifier saw nothing wrong. Never expected:
    /// the two share one reader configuration, and this variant exists so that
    /// a disagreement is reported rather than folded into the other two.
    #[error(
        "the parser refused at line {line}, column {column} and the classifier did not: {message}"
    )]
    Unexplained {
        line: usize,
        column: usize,
        message: String,
    },
    /// A prefix used and never declared.
    #[error("prefix `{prefix}` is not declared, at line {line}, column {column}")]
    UndeclaredPrefix {
        prefix: String,
        line: usize,
        column: usize,
    },
    /// Something this project has not decided what to do with.
    #[error("{} at line {line}, column {column}: no policy has been decided", .what.key())]
    Undecided {
        what: Undecided,
        line: usize,
        column: usize,
    },
    /// Text, or a second element, outside the root element.
    #[error("content outside the root element at line {line}, column {column}")]
    OutsideRoot { line: usize, column: usize },
    #[error("no root element")]
    NoRoot,
}

/// The constructs `docs/XML-CONTRACT.md` §5 leaves without a policy. None of
/// them occurs in TLHdig Beta 0.3; a document that brings one is refused and
/// named until the decision is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Undecided {
    /// An entity reference other than the five predefined ones and character
    /// references — contract §5, question 2.
    EntityReference,
    /// A `<!DOCTYPE …>`. Nothing is fetched, ever; and what an internal subset
    /// would declare is the same undecided question as above.
    DocumentType,
    /// An XML version other than 1.0. Version 1.1 reads line ends differently.
    Version,
    /// A declared encoding other than UTF-8.
    Encoding,
}

impl Undecided {
    pub fn key(self) -> &'static str {
        match self {
            Undecided::EntityReference => "entity-reference",
            Undecided::DocumentType => "document-type-declaration",
            Undecided::Version => "xml-version",
            Undecided::Encoding => "encoding",
        }
    }
}

impl<'s> Document<'s> {
    /// Read one document from its bytes.
    pub fn read(bytes: &'s [u8]) -> Result<Self, Refusal> {
        let source = std::str::from_utf8(bytes).map_err(|err| {
            let (line, column) = xml_wellformed::position(bytes, err.valid_up_to());
            Refusal::NotUtf8 { line, column }
        })?;
        let document = match Builder::new(source).run() {
            Ok(document) => document,
            Err(Stop::Refused(refusal)) => return Err(refusal),
            Err(Stop::Parser { at, message }) => {
                return Err(match xml_wellformed::classify(bytes) {
                    Some(finding) => Refusal::Unread(finding),
                    None => {
                        let (line, column) = xml_wellformed::position(bytes, at);
                        Refusal::Unexplained {
                            line,
                            column,
                            message,
                        }
                    }
                })
            }
        };
        // Asked only of a document the parser accepted, as the manifest asks
        // it: of a refused one the scan would describe wreckage.
        if let Some(beyond) = xml_wellformed::beyond_the_parser(bytes) {
            return Err(Refusal::BeyondTheParser(beyond));
        }
        Ok(document)
    }

    /// The XML declaration, if the document opens with one.
    pub fn declaration(&self) -> Option<&Declaration<'s>> {
        self.declaration.as_ref()
    }

    /// Every node, in document order.
    pub fn nodes(&self) -> &[Node<'s>] {
        &self.nodes
    }

    /// The index of the root element in [`Document::nodes`].
    pub fn root(&self) -> usize {
        self.root
    }

    /// The children of `parent`, in document order; `None` asks for the top of
    /// the document.
    pub fn children(&self, parent: Option<usize>) -> Children<'_, 's> {
        let (next, stop) = match parent {
            Some(at) => (at + 1, self.nodes.get(at).map_or(at + 1, |node| node.end)),
            None => (0, self.nodes.len()),
        };
        Children {
            nodes: &self.nodes,
            next,
            stop,
        }
    }
}

/// See [`Document::children`].
pub struct Children<'d, 's> {
    nodes: &'d [Node<'s>],
    next: usize,
    stop: usize,
}

impl Iterator for Children<'_, '_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        if self.next >= self.stop {
            return None;
        }
        let at = self.next;
        self.next = self.nodes.get(at).map_or(self.stop, |node| node.end);
        Some(at)
    }
}

/// Why building stopped.
enum Stop {
    /// `quick-xml` refused, at this byte; the classifier is asked why.
    Parser {
        at: usize,
        message: String,
    },
    Refused(Refusal),
}

/// The URI of a namespace declaration in scope, and at which depth it was made.
///
/// Only the URI: which prefix it is bound to is `quick-xml`'s resolver's
/// question, and this list exists so the model can borrow the URI from where
/// the document wrote it rather than copy the resolver's.
struct Scoped<'s> {
    depth: usize,
    uri: Cow<'s, str>,
}

struct Builder<'s> {
    source: &'s str,
    reader: NsReader<&'s [u8]>,
    declaration: Option<Declaration<'s>>,
    nodes: Vec<Node<'s>>,
    open: Vec<usize>,
    scopes: Vec<Scoped<'s>>,
    root: Option<usize>,
    /// Character data not yet a node, and the byte it began at.
    pending: Option<(Cow<'s, str>, usize)>,
}

impl<'s> Builder<'s> {
    fn new(source: &'s str) -> Self {
        let mut reader = NsReader::from_str(source);
        xml_wellformed::strict(reader.config_mut());
        Builder {
            source,
            reader,
            declaration: None,
            nodes: Vec::new(),
            open: Vec::new(),
            scopes: Vec::new(),
            root: None,
            pending: None,
        }
    }

    fn run(mut self) -> Result<Document<'s>, Stop> {
        loop {
            let at = self.reader.buffer_position() as usize;
            let (resolved, event) = match self.reader.read_resolved_event() {
                Ok((resolved, event)) => (Resolved::from(resolved), event),
                Err(err) => {
                    return Err(Stop::Parser {
                        at: self.reader.error_position() as usize,
                        message: err.to_string(),
                    })
                }
            };
            match event {
                Event::Start(tag) => self.element(&tag, resolved, at, true)?,
                Event::Empty(tag) => self.element(&tag, resolved, at, false)?,
                Event::End(_) => {
                    self.flush()?;
                    let Some(element) = self.open.pop() else {
                        return Err(Stop::Parser {
                            at,
                            message: "end tag with nothing open".into(),
                        });
                    };
                    let end = self.nodes.len();
                    if let Some(node) = self.nodes.get_mut(element) {
                        node.end = end;
                    }
                    self.leave_scope();
                }
                Event::Text(text) => self.append(text.xml10_content(), at),
                Event::CData(data) => self.append(data.xml10_content(), at),
                Event::GeneralRef(reference) => {
                    if reference.is_char_ref() {
                        match reference.resolve_char_ref() {
                            Ok(Some(ch)) => self.append(Cow::Owned(ch.to_string()), at),
                            Ok(None) | Err(_) => {
                                return Err(Stop::Parser {
                                    at,
                                    message: format!(
                                        "invalid character reference `{}`",
                                        &*reference
                                    ),
                                })
                            }
                        }
                    } else if let Some(ch) = resolve_predefined_entity(&reference) {
                        self.append(Cow::Borrowed(ch), at);
                    } else {
                        return Err(self.undecided(Undecided::EntityReference, at));
                    }
                }
                Event::Comment(comment) => {
                    self.flush()?;
                    let text = comment.xml10_content();
                    self.leaf(Kind::Comment(text));
                }
                Event::PI(instruction) => {
                    self.flush()?;
                    let target = borrowed(self.source, instruction.target());
                    let data = instruction
                        .content()
                        .trim_start_matches([' ', '\t', '\r', '\n']);
                    let data = normalise_line_ends(borrowed(self.source, data));
                    self.leaf(Kind::Instruction { target, data });
                }
                Event::Decl(declaration) => {
                    let version = declaration
                        .version()
                        .map_err(|err| Stop::Parser {
                            at,
                            message: err.to_string(),
                        })?
                        .into_owned();
                    if version != "1.0" {
                        return Err(self.undecided(Undecided::Version, at));
                    }
                    let encoding = match declaration.encoding() {
                        None => None,
                        Some(Ok(encoding)) => Some(encoding.into_owned()),
                        Some(Err(err)) => {
                            return Err(Stop::Parser {
                                at,
                                message: err.to_string(),
                            })
                        }
                    };
                    if encoding
                        .as_deref()
                        .is_some_and(|name| !name.eq_ignore_ascii_case("utf-8"))
                    {
                        return Err(self.undecided(Undecided::Encoding, at));
                    }
                    let standalone = match declaration.standalone() {
                        None => None,
                        Some(Ok(standalone)) => Some(standalone.into_owned()),
                        Some(Err(err)) => {
                            return Err(Stop::Parser {
                                at,
                                message: err.to_string(),
                            })
                        }
                    };
                    self.declaration = Some(Declaration {
                        version: Cow::Owned(version),
                        encoding: encoding.map(Cow::Owned),
                        standalone: standalone.map(Cow::Owned),
                    });
                }
                Event::DocType(_) => return Err(self.undecided(Undecided::DocumentType, at)),
                Event::Eof => {
                    self.flush()?;
                    if !self.open.is_empty() {
                        // `quick-xml` is silent on an input that ends with
                        // elements open; the classifier is not.
                        return Err(Stop::Parser {
                            at: self.source.len(),
                            message: "input ended with elements open".into(),
                        });
                    }
                    let Some(root) = self.root else {
                        return Err(Stop::Refused(Refusal::NoRoot));
                    };
                    return Ok(Document {
                        declaration: self.declaration,
                        nodes: self.nodes,
                        root,
                    });
                }
            }
        }
    }

    fn element(
        &mut self,
        tag: &BytesStart<'s>,
        resolved: Resolved,
        at: usize,
        has_content: bool,
    ) -> Result<(), Stop> {
        self.flush()?;
        if self.open.is_empty() && self.root.is_some() {
            return Err(self.outside_root(at));
        }
        let depth = self.open.len();

        // Attributes first, because the tag's own declarations are in scope for
        // its own name.
        let mut namespaces = Vec::new();
        let mut plain: Vec<(QName<'_>, Cow<'s, str>)> = Vec::new();
        for attribute in tag.attributes() {
            let attribute: RawAttribute<'_> = attribute.map_err(|err| Stop::Parser {
                at,
                message: err.to_string(),
            })?;
            let key = attribute.key;
            let value = match attribute.normalized_value(XmlVersion::Implicit1_0) {
                Ok(value) => kept(self.source, value),
                Err(quick_xml::Error::Escape(EscapeError::UnrecognizedEntity(..))) => {
                    return Err(self.undecided(Undecided::EntityReference, at))
                }
                Err(err) => {
                    return Err(Stop::Parser {
                        at,
                        message: err.to_string(),
                    })
                }
            };
            match key.as_namespace_binding() {
                Some(PrefixDeclaration::Default) => namespaces.push(Binding {
                    prefix: None,
                    uri: value,
                }),
                Some(PrefixDeclaration::Named(prefix)) => namespaces.push(Binding {
                    prefix: Some(borrowed(self.source, prefix)),
                    uri: value,
                }),
                None => plain.push((key, value)),
            }
        }
        for binding in &namespaces {
            self.scopes.push(Scoped {
                depth,
                uri: binding.uri.clone(),
            });
        }

        let name = self.name(tag.name(), resolved, at)?;
        let mut attributes = Vec::with_capacity(plain.len());
        for (key, value) in plain {
            let resolved = Resolved::from(self.reader.resolver().resolve_attribute(key).0);
            let name = self.name(key, resolved, at)?;
            attributes.push(Attribute { name, value });
        }

        let index = self.nodes.len();
        self.nodes.push(Node {
            parent: self.open.last().copied(),
            end: index + 1,
            kind: Kind::Element(Element {
                name,
                namespaces,
                attributes,
            }),
        });
        if self.open.is_empty() {
            self.root = Some(index);
        }
        if has_content {
            self.open.push(index);
        } else {
            self.leave_scope();
        }
        Ok(())
    }

    fn name(&self, qname: QName<'_>, resolved: Resolved, at: usize) -> Result<Name<'s>, Stop> {
        let (local, prefix) = qname.decompose();
        let prefix = prefix.map(|prefix| borrowed(self.source, prefix.into_inner()));
        let namespace = match resolved {
            Resolved::Unbound => None,
            Resolved::Bound(uri) => Some(self.in_scope(&uri)),
            Resolved::Unknown(prefix) => {
                let (line, column) = xml_wellformed::position(self.source.as_bytes(), at);
                return Err(Stop::Refused(Refusal::UndeclaredPrefix {
                    prefix,
                    line,
                    column,
                }));
            }
        };
        Ok(Name {
            prefix,
            local: borrowed(self.source, local.into_inner()),
            namespace,
        })
    }

    /// The declared URI equal to `uri`, borrowed from where it was declared.
    fn in_scope(&self, uri: &str) -> Cow<'s, str> {
        if let Some(scoped) = self.scopes.iter().rev().find(|scoped| scoped.uri == uri) {
            return scoped.uri.clone();
        }
        if uri == XML_NAMESPACE {
            return Cow::Borrowed(XML_NAMESPACE);
        }
        Cow::Owned(uri.to_owned())
    }

    /// Close the scope of the element just finished.
    fn leave_scope(&mut self) {
        let depth = self.open.len();
        while self
            .scopes
            .last()
            .is_some_and(|scoped| scoped.depth >= depth)
        {
            self.scopes.pop();
        }
    }

    fn append(&mut self, piece: Cow<'s, str>, at: usize) {
        match &mut self.pending {
            None => self.pending = Some((piece, at)),
            Some((text, _)) => text.to_mut().push_str(&piece),
        }
    }

    /// Turn pending character data into a node, or refuse it outside the root.
    fn flush(&mut self) -> Result<(), Stop> {
        let Some((text, at)) = self.pending.take() else {
            return Ok(());
        };
        if self.open.is_empty() {
            if text
                .chars()
                .all(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'))
            {
                return Ok(());
            }
            return Err(self.outside_root(at));
        }
        self.leaf(Kind::Text(text));
        Ok(())
    }

    fn leaf(&mut self, kind: Kind<'s>) {
        let index = self.nodes.len();
        self.nodes.push(Node {
            parent: self.open.last().copied(),
            end: index + 1,
            kind,
        });
    }

    fn undecided(&self, what: Undecided, at: usize) -> Stop {
        let (line, column) = xml_wellformed::position(self.source.as_bytes(), at);
        Stop::Refused(Refusal::Undecided { what, line, column })
    }

    fn outside_root(&self, at: usize) -> Stop {
        let (line, column) = xml_wellformed::position(self.source.as_bytes(), at);
        Stop::Refused(Refusal::OutsideRoot { line, column })
    }
}

/// [`ResolveResult`] without the borrow of the reader, so the reader can be
/// asked again before the answer is used.
enum Resolved {
    Unbound,
    Bound(String),
    Unknown(String),
}

impl From<ResolveResult<'_>> for Resolved {
    fn from(resolved: ResolveResult<'_>) -> Self {
        match resolved {
            ResolveResult::Unbound => Resolved::Unbound,
            ResolveResult::Bound(namespace) => Resolved::Bound(namespace.0.to_owned()),
            ResolveResult::Unknown(prefix) => Resolved::Unknown(prefix),
        }
    }
}

/// `part` as a slice of `source` when it is one, so the model borrows instead
/// of copying; a copy otherwise.
///
/// The reader hands out slices of the text it was given, but typed with the
/// lifetime of the event rather than of the text. The address says which they
/// are. No `unsafe`: an address is compared, never dereferenced, and the slice
/// returned is taken from `source` by index.
fn borrowed<'s>(source: &'s str, part: &str) -> Cow<'s, str> {
    let start = (part.as_ptr() as usize).wrapping_sub(source.as_ptr() as usize);
    match start
        .checked_add(part.len())
        .and_then(|end| source.get(start..end))
    {
        Some(slice) if std::ptr::eq(slice.as_ptr(), part.as_ptr()) => Cow::Borrowed(slice),
        _ => Cow::Owned(part.to_owned()),
    }
}

fn kept<'s>(source: &'s str, value: Cow<'_, str>) -> Cow<'s, str> {
    match value {
        Cow::Borrowed(part) => borrowed(source, part),
        Cow::Owned(value) => Cow::Owned(value),
    }
}

/// XML 1.0 line-end normalisation for the one kind of event `quick-xml` does
/// not normalise itself.
fn normalise_line_ends(text: Cow<'_, str>) -> Cow<'_, str> {
    if !text.contains('\r') {
        return text;
    }
    Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml_wellformed::{Limit, Reason};

    fn read(xml: &str) -> Document<'_> {
        Document::read(xml.as_bytes()).unwrap_or_else(|why| panic!("refused {xml}: {why}"))
    }

    fn refusal(xml: &str) -> Refusal {
        match Document::read(xml.as_bytes()) {
            Ok(_) => panic!("accepted {xml}"),
            Err(why) => why,
        }
    }

    fn element<'d, 's>(document: &'d Document<'s>, at: usize) -> &'d Element<'s> {
        match &document.nodes()[at].kind {
            Kind::Element(element) => element,
            other => panic!("node {at} is {other:?}"),
        }
    }

    /// The boundary, held at the source: nothing in the model's code names an
    /// output format, a renderer, or the modules that produce one.
    ///
    /// Comment lines are skipped — the module header names PDF to say it is not
    /// here — and so is this test module, which has to name what it forbids.
    #[test]
    fn the_model_names_no_output_format() {
        let source = include_str!("document.rs");
        let found = output_formats_named_in(source);
        assert!(
            found.is_empty(),
            "the model names an output format: {found:#?}"
        );
    }

    /// The words the boundary forbids, as `document.rs:<line>: <word>`, for the
    /// code above the tests in `source`.
    fn output_formats_named_in(source: &str) -> Vec<String> {
        let code = source.split("#[cfg(test)]").next().unwrap_or(source);
        let forbidden = [
            "pdf",
            "html",
            "typst",
            "presentation",
            "style",
            "layout",
            "export",
        ];
        let mut found = Vec::new();
        for (number, line) in code.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            let lower = trimmed.to_ascii_lowercase();
            for word in forbidden {
                if lower.contains(word) {
                    found.push(format!("document.rs:{}: {word}: {line}", number + 1));
                }
            }
        }
        found
    }

    /// **The boundary guard has teeth.** A source that passes it because the
    /// guard reads nothing would look exactly like a model that keeps the
    /// boundary; so the guard is run over the real source with one line of code
    /// planted above the tests, and must name it. A comment naming the same
    /// module must still pass, as the header's does.
    #[test]
    fn the_output_format_guard_names_a_planted_line() {
        let source = include_str!("document.rs");
        let at = source.find("#[cfg(test)]").expect("the tests");
        let planted = format!(
            "{}use crate::presentation::CorpusPresentation;\n{}",
            &source[..at],
            &source[at..]
        );
        let found = output_formats_named_in(&planted);
        assert_eq!(found.len(), 1, "the planted line: {found:#?}");
        assert!(found[0].contains("presentation"), "{found:#?}");

        let commented = format!(
            "{}// crate::presentation is where a page is made, not here\n{}",
            &source[..at],
            &source[at..]
        );
        assert!(output_formats_named_in(&commented).is_empty());
    }

    /// Siblings stay in the order written, and so do attributes — on a shape
    /// where order is the content: two lines of one tablet.
    #[test]
    fn order_is_the_order_of_the_document() {
        let document = read(
            r#"<text><lb lnr="2" lg="Hit"/><w trans="nu">nu</w><lb lnr="1" lg="Hit"/><w trans="ta">ta</w></text>"#,
        );
        let root = document.root();
        let children: Vec<usize> = document.children(Some(root)).collect();
        let shape: Vec<String> = children
            .iter()
            .map(|&at| {
                let element = element(&document, at);
                let attributes: Vec<String> = element
                    .attributes
                    .iter()
                    .map(|attribute| format!("{}={}", attribute.name.local, attribute.value))
                    .collect();
                format!("{} {}", element.name.local, attributes.join(" "))
            })
            .collect();
        assert_eq!(
            shape,
            [
                "lb lnr=2 lg=Hit",
                "w trans=nu",
                "lb lnr=1 lg=Hit",
                "w trans=ta"
            ]
        );
        // And the vector itself is document order: the text of the first `w`
        // comes before the second `lb`.
        assert_eq!(document.nodes().len(), 7);
        assert_eq!(document.nodes()[root].end, 7);
        assert!(matches!(&document.nodes()[3].kind, Kind::Text(text) if text == "nu"));
    }

    /// What the document does not carry, the model does not carry.
    #[test]
    fn absence_is_absence() {
        let document = read(r#"<a><w>nu</w></a>"#);
        assert_eq!(document.declaration(), None);
        let w = element(&document, 1);
        assert!(w.attributes.is_empty());
        assert!(w.namespaces.is_empty());
        assert_eq!(w.name.prefix, None);
        assert_eq!(w.name.namespace, None);

        let declared = read(r#"<?xml version="1.0"?><a/>"#);
        assert_eq!(
            declared.declaration(),
            Some(&Declaration {
                version: Cow::Borrowed("1.0"),
                encoding: None,
                standalone: None,
            })
        );
    }

    /// The three things the XML specification changes on reading, and nothing
    /// else: line ends, whitespace in values, references.
    #[test]
    fn the_model_is_what_xml_says_was_read() {
        let document = read("<a b=\"x\ty\r\nz\" c=\"&lt;&#039;\">one\r\ntwo &amp; three</a>");
        let a = element(&document, 0);
        assert_eq!(a.attributes[0].value, "x y z");
        assert_eq!(a.attributes[1].value, "<'");
        assert!(
            matches!(&document.nodes()[1].kind, Kind::Text(text) if text == "one\ntwo & three")
        );
        assert_eq!(
            document.nodes().len(),
            2,
            "adjacent character data is one node"
        );
    }

    /// Held once: text the specification does not change is borrowed from the
    /// document, not copied.
    #[test]
    fn unchanged_text_is_borrowed_not_copied() {
        let xml = r#"<a trans="nu"><w>ta</w></a>"#;
        let document = read(xml);
        let a = element(&document, 0);
        assert!(matches!(a.name.local, Cow::Borrowed(_)));
        assert!(matches!(a.attributes[0].value, Cow::Borrowed(_)));
        assert!(matches!(
            &document.nodes()[2].kind,
            Kind::Text(Cow::Borrowed(_))
        ));
    }

    /// Заимствование доказывается адресом, а не именем варианта.
    ///
    /// `Cow::Borrowed` сам по себе говорит только, что копии не делали здесь;
    /// он был бы тем же и для строки, взятой откуда угодно еще. Здесь
    /// проверяется то, ради чего модель писалась: текст лежит внутри самого
    /// документа, то есть документ в памяти один. Отрицательный контроль рядом —
    /// копия той же строки в диапазон не попадает, иначе проверка сходилась бы
    /// у чего угодно.
    #[test]
    fn borrowed_text_points_into_the_document_itself() {
        let xml = String::from(r#"<a trans="nu"><w>ta</w></a>"#);
        let document = read(&xml);
        let from = xml.as_bytes().as_ptr() as usize;
        let range = from..from + xml.len();

        let Kind::Text(Cow::Borrowed(text)) = &document.nodes()[2].kind else {
            panic!("текст не заимствован");
        };
        assert!(
            range.contains(&(text.as_ptr() as usize)),
            "текст лежит не в документе, значит документ скопирован"
        );

        let Cow::Borrowed(value) = &element(&document, 0).attributes[0].value else {
            panic!("значение атрибута не заимствовано");
        };
        assert!(
            range.contains(&(value.as_ptr() as usize)),
            "значение атрибута лежит не в документе"
        );

        let copy = text.to_string();
        assert!(
            !range.contains(&(copy.as_ptr() as usize)),
            "отрицательный контроль не работает: копия попала в диапазон"
        );
    }

    #[test]
    fn names_are_resolved_by_the_declarations_in_the_document() {
        let document = read(
            r#"<AOxml xmlns:AO="http://hethiter.net/ns/AO/1.0" xml:space="preserve"><AO:TxtPubl n="1">KBo 1.1</AO:TxtPubl></AOxml>"#,
        );
        let root = element(&document, 0);
        assert_eq!(root.namespaces.len(), 1);
        assert!(root.attributes.iter().all(|a| a.name.local != "AO"));
        assert_eq!(
            root.attributes[0].name.namespace.as_deref(),
            Some(XML_NAMESPACE)
        );
        let publication = element(&document, 1);
        assert_eq!(publication.name.prefix.as_deref(), Some("AO"));
        assert_eq!(
            publication.name.namespace.as_deref(),
            Some("http://hethiter.net/ns/AO/1.0")
        );
        assert_eq!(publication.attributes[0].name.namespace, None);
    }

    #[test]
    fn comments_and_instructions_are_nodes_in_place() {
        let document = read("<?xml-stylesheet href=\"HPMxml.css\"?><a><!-- x --><?p  data?></a>");
        let kinds: Vec<&Kind<'_>> = document.nodes().iter().map(|node| &node.kind).collect();
        assert!(
            matches!(kinds[0], Kind::Instruction { target, data } if target == "xml-stylesheet" && data == "href=\"HPMxml.css\"")
        );
        assert!(matches!(kinds[2], Kind::Comment(text) if text == " x "));
        assert!(
            matches!(kinds[3], Kind::Instruction { target, data } if target == "p" && data == "data")
        );
        assert_eq!(document.nodes()[0].parent, None);
        assert_eq!(document.root(), 1);
    }

    /// What the parser refuses is refused with the classifier's reason.
    #[test]
    fn an_unread_document_carries_the_classifiers_reason() {
        assert!(matches!(
            refusal("<a><t><w>nu</t></a>"),
            Refusal::Unread(Finding {
                reason: Reason::ElementNeverClosed,
                ..
            })
        ));
        assert!(matches!(refusal("<a><w>nu</w>"), Refusal::Unread(_)));
        assert!(matches!(refusal(r#"<a c="1" c="2"/>"#), Refusal::Unread(_)));
        assert!(matches!(
            refusal("<a><!-- a -- b --></a>"),
            Refusal::Unread(_)
        ));
    }

    /// The seventeen: accepted by the parser, refused by the model.
    #[test]
    fn what_the_parser_misses_is_refused_all_the_same() {
        assert!(matches!(
            refusal(r#"<AOxml xmlns:AO="u"><AO:-LineNrExpl/></AOxml>"#),
            Refusal::BeyondTheParser(Beyond {
                limit: Limit::ColonWithoutLocalName,
                ..
            })
        ));
        assert!(matches!(
            refusal(r#"<a><w trans="a<b">nu</w></a>"#),
            Refusal::BeyondTheParser(Beyond {
                limit: Limit::RawLessThanInAttributeValue,
                ..
            })
        ));
    }

    /// No policy, no guess — and nothing fetched on the way to refusing.
    #[test]
    fn what_has_no_policy_is_refused() {
        let undecided = |xml: &str| match refusal(xml) {
            Refusal::Undecided { what, .. } => what,
            other => panic!("{xml}: {other:?}"),
        };
        assert_eq!(
            undecided(r#"<!DOCTYPE a [<!ENTITY x SYSTEM "file:///etc/passwd">]><a>&x;</a>"#),
            Undecided::DocumentType
        );
        assert_eq!(undecided("<a>&x;</a>"), Undecided::EntityReference);
        assert_eq!(undecided(r#"<a b="&x;"/>"#), Undecided::EntityReference);
        assert_eq!(
            undecided(r#"<?xml version="1.1"?><a/>"#),
            Undecided::Version
        );
        assert_eq!(
            undecided(r#"<?xml version="1.0" encoding="ISO-8859-1"?><a/>"#),
            Undecided::Encoding
        );
    }

    #[test]
    fn the_root_is_one_element_and_nothing_else_is_outside_it() {
        assert!(matches!(refusal("<a/><b/>"), Refusal::OutsideRoot { .. }));
        assert!(matches!(refusal("<a/>text"), Refusal::OutsideRoot { .. }));
        assert!(matches!(refusal("<!-- only -->"), Refusal::NoRoot));
        assert!(matches!(
            refusal("<p:a/>"),
            Refusal::UndeclaredPrefix { .. }
        ));
        let spaced = read("\n<a/>\n\n");
        assert_eq!(spaced.nodes().len(), 1);
        assert!(matches!(
            Document::read(b"<a>\xff</a>"),
            Err(Refusal::NotUtf8 { line: 1, column: 4 })
        ));
    }
}
