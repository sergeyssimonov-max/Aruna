// The WebKit half of `bench-search.ts`: loads the prepared page in the system
// WKWebView — the engine Safari runs on this machine — prints each message
// the page posts back as a line of JSON on stdout, and exits on the report
// that closes the run.
//
//     swift scripts/bench-search.swift <package-dir>/bench-search.html
//
// No window is created and the process takes no place in the Dock. A view
// that is not on screen may not draw frames; the page reports that itself.

import AppKit
import WebKit

final class Runner: NSObject, WKScriptMessageHandler {
    func userContentController(
        _ controller: WKUserContentController, didReceive message: WKScriptMessage
    ) {
        guard let body = message.body as? String else { return }
        print(body)
        fflush(stdout)
        if body.hasPrefix("{\"done\":true") { exit(0) }
    }
}

guard CommandLine.arguments.count == 2 else {
    FileHandle.standardError.write("usage: swift bench-search.swift <page.html>\n".data(using: .utf8)!)
    exit(2)
}
let page = URL(fileURLWithPath: CommandLine.arguments[1])

let app = NSApplication.shared
app.setActivationPolicy(.prohibited)

let runner = Runner()
let configuration = WKWebViewConfiguration()
configuration.userContentController.add(runner, name: "bench")
let view = WKWebView(frame: NSRect(x: 0, y: 0, width: 1280, height: 900), configuration: configuration)
view.loadFileURL(page, allowingReadAccessTo: page.deletingLastPathComponent())

DispatchQueue.main.asyncAfter(deadline: .now() + 1800) {
    FileHandle.standardError.write("webkit: no result in thirty minutes\n".data(using: .utf8)!)
    exit(1)
}
app.run()
