// Sets the given .app as the default web browser (http and https).
// macOS asks for confirmation in a system dialog.
//
//   swift scripts/set-default-browser.swift "/Applications/Browsers.app"

import AppKit

guard CommandLine.arguments.count == 2 else {
    print("usage: set-default-browser.swift <path to .app>")
    exit(1)
}

let appURL = URL(fileURLWithPath: CommandLine.arguments[1]).standardizedFileURL

func currentDefault(_ scheme: String) -> URL? {
    NSWorkspace.shared.urlForApplication(toOpen: URL(string: "\(scheme)://example.com")!)?.standardizedFileURL
}

// One scheme at a time: while macOS shows its confirmation dialog for the
// first request, a second concurrent request fails with "couldn't be opened".
// Confirming the dialog usually switches both schemes, so skip any that are
// already set.
for scheme in ["http", "https"] {
    if currentDefault(scheme) == appURL { continue }
    var done = false
    NSWorkspace.shared.setDefaultApplication(at: appURL, toOpenURLsWithScheme: scheme) { error in
        if let error {
            print("\(scheme): \(error.localizedDescription)")
        }
        done = true
    }
    while !done {
        RunLoop.main.run(until: Date().addingTimeInterval(0.1))
    }
}

let notSet = ["http", "https"].filter { currentDefault($0) != appURL }
if notSet.isEmpty {
    print("Default browser: \(appURL.path)")
    exit(0)
}
print("Not set for: \(notSet.joined(separator: ", ")). Pick it in System Settings > Desktop & Dock > Default web browser.")
exit(1)
