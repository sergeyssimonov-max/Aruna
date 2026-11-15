// Run the release reader's transport checks in the ordinary test suite. The
// module is pure and needs neither PDFKit nor the real corpus on CI.
#[path = "../examples/pdf_gate/pdfkit_output.rs"]
mod pdfkit_output;

#[path = "../examples/pdf_gate/canonical_output.rs"]
mod canonical_output;
