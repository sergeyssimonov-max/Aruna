// The second, independent reader of PDF-ACCEPTANCE §1 on the gate: PDFKit,
// Apple's implementation – not Typst's, not krilla's – which ships with macOS
// and through which Preview copies text. Nothing is installed, no window
// opens. Brought over from the fourth trial and cut to what the gate reads.
//
// For each PDF one line: whether PDFKit opens it, its page count, the count
// of every private-use code point in the text PDFKit extracts (the code
// points of the labels) and how many label letters – `U+` followed by a hex
// digit – that text holds, which must be none.
//
//   swiftc -O -framework PDFKit cli/examples/pdf_gate/pdfkit.swift -o <bin>
//   <bin> <file.pdf>...

import Foundation
import PDFKit

for path in CommandLine.arguments.dropFirst() {
    guard let doc = PDFDocument(url: URL(fileURLWithPath: path)) else {
        print("UNREADABLE\t\(path)")
        continue
    }
    let text = doc.string ?? ""
    var labels: [UInt32: Int] = [:]
    for scalar in text.unicodeScalars where scalar.value >= 0xE000 && scalar.value <= 0xF8FF || scalar.value >= 0xF0000 {
        labels[scalar.value, default: 0] += 1
    }
    var letters = 0
    let chars = Array(text.unicodeScalars)
    if chars.count > 2 {
        for i in 0..<(chars.count - 2) where chars[i] == "U" && chars[i + 1] == "+" && CharacterSet(charactersIn: "0123456789ABCDEF").contains(chars[i + 2]) {
            letters += 1
        }
    }
    let spelled = labels.keys.sorted().map { String(format: "%X:%d", $0, labels[$0]!) }.joined(separator: ",")
    print("OPENED\t\(path)\t\(doc.pageCount)\t\(spelled)\t\(letters)")
}
