//! English diagnostic templates shared privately by the library and CLI.
//!
//! `export::validate` formats these by key; the console supplies the Russian
//! wording. The remaining producers still write their own English sentences,
//! checked against this catalogue by the console's source guard.
//! No key is exposed in `ArunaError`, IPC or output files.
//!
//! Order is significant: the console chooses the first matching template.
//! `LinkToUnownedFile` must precede `LinkToOtherDocument`.

macro_rules! with_diagnostics {
    ($consumer:ident) => {
        $consumer! {
            SourceEncoding => "the source declares encoding=\"{}\" and the canonical declaration says UTF-8; the bytes would be kept and their meaning changed",
            SourceVersion => "the source declares version=\"{}\" and the canonical declaration says 1.0; the bytes would be kept and their meaning changed",
            DroppedInstruction => "instruction <?{}…?> was dropped and is not on the permit list",
            ContentDiffers => "content differs at byte {} (source {} bytes, output {} bytes)\n      source: …{}…\n      output: …{}…",
            MissingDeclaration => "the output does not begin with the canonical declaration",
            AddedInstruction => "an instruction appears in the output that was not in the source",
            InventoryOutsideRows => "the inventory links something outside its rows: {}",
            InventoryDocumentCount => "the inventory lists {} documents, and {} were placed",
            MissingInventoryLink => "{} has no link in the inventory",
            MultipleInventoryLinks => "{} has {} links in the inventory, and a document has one",
            LinkOutsidePackage => "the link of {} is not a relative path inside the package: {}",
            LinkNotDocument => "the link of {} is not to a document: {}",
            LinkToRefusedPdf => "{} links a PDF, and its PDF was refused: {}",
            LinkToXmlWithPdf => "{} links its XML, and its PDF was built: {}",
            LinkToUnownedFile => "{} links a file of no document: {}",
            LinkToOtherDocument => "{} links a file of {}: {}",
            LinkMissingFile => "the link of {} points at nothing: {}",
            PackageMissingFile => "the package is missing {}",
            OrphanFile => "orphan file in the package: {}",
            MissingDocument => "expected document missing: {}",
            UnopenableDocument => "expected document does not open by its name: {}",
            DocumentUnicodeTwins => "two documents differ only in Unicode form and the disk holds one file for both: {} and {}",
            DiskUnicodeTwins => "two files on the disk differ only in Unicode form: {} and {}",
            MissingGroupFolder => "group without a folder: {}",
            UnreadableManifest => "the manifest cannot be read: {}",
            PackageFileTooLarge => "{} is larger than the {} byte limit for this file, so it is not one this program wrote",
            MissingManifestEntry => "the manifest has no {} entry for {}",
            ExtraManifestEntry => "the manifest has a {} entry for {}, which is not in the package",
            ManifestNameCount => "the manifest lists {} distinct {} names for {} documents",
            MissingPdfsSection => "the manifest has no pdfs section",
            PdfRefusalCount => "the manifest gives {} reasons for a refused PDF, and {} documents have none",
            PackageTooDeep => "{} is nested deeper than a package can be",
            UnreadableDirectory => "cannot read {}",
            PackageSymlink => "the package holds a symbolic link, which an export never writes: {}",
            UnexpectedPackageFile => "unexpected file in the package: {}",
            DestinationSymlink => "it is a symbolic link, and whatever it points at is not this exporter's to replace",
            DestinationFile => "it is a file, not a package",
            DestinationMissingInventory => "there is no {} in it",
            DestinationForeignFile => "it contains {}, which this exporter did not put there",
            PublishedPackageDiffers => "the published package differs from the one that was built and checked",
            InvalidPublishLock => "something that is not a lock file stands where the publish lock belongs",
            PublishHolder => "pid {}, since {}",
            ForeignPublishLock => "a lock file this program did not write",
            UnnamedRun => "an unnamed run",
            FontUnassignedCuneiform => "U+100000, which nothing on a stock machine draws",
            FontCorpusCuneiform => "the 376 cuneiform signs of the corpus",
            FontEditorialMarks => "six editorial marks",
            FontMain => "the main face of the PDF",
            FontItalic => "the main face, italic",
            FontBold => "the main face, bold",
            FontHebrewPunctuation => "U+05C3",
            FontTerms => "the terms the font beside it is distributed under",
            FontDirectoryUnnamed => "the PDF, and no directory of fonts was named (ARUNA_FONTS in the console)",
            MissingPdf => "PDF built but not on disk: {}",
            OrphanPdf => "PDF on disk that no document built: {}",
            PdfUnicodeTwins => "two PDFs differ only in Unicode form and the disk holds one file for both: {} and {}",
            PdfWorldAsked => "Typst asked for {}, and only the template and the data exist",
            PdfUnplacedLabels => "{} labels in the text, {} label images in the file",
            PdfUnplacedSpans => "{} label spans not found exactly once in their stream",
            PdfClusterOverlap => "page {}: two repeated clusters overlap at MCID {}",
            PdfCreditMissing => "the credit was predicted and UllikummiA is not embedded",
            PdfCreditUnexpected => "UllikummiA is embedded and the credit was not predicted",
            PdfLabelGlyph => "the main face cannot draw a label: {}",
            PdfStackFace => "the font stack has no regular face of {}",
            PdfWorld => "the world of the template could not be built",
            PdfReadBack => "the PDF this program wrote could not be read back: {}",
            PdfRefused => "Typst refused the document: {}",
        }
    };
}
pub(crate) use with_diagnostics;

// `format!` needs a literal, so expand a key to one at compile time rather
// than adding a runtime formatter. Rust still checks each call's arguments.
macro_rules! define_templates {
    ($($key:ident => $english:literal,)*) => {
        macro_rules! text {
            $(($key) => { $english };)*
        }
        pub(crate) use text;
    };
}
self::with_diagnostics!(define_templates);
