// Sets the given .app as the default web browser (http and https).
// macOS asks for confirmation in a system dialog.
//
//   swift scripts/set-default-browser.swift "/Applications/Browsers.app"

import AppKit

guard CommandLine.arguments.count == 2 else {
    print("usage: set-default-browser.swift <path to .app>")
    exit(1)
}

let appURL = URL(fileURLWithPath: CommandLine.arguments[1])
var pending = 2
var failed = false

for scheme in ["http", "https"] {
    NSWorkspace.shared.setDefaultApplication(at: appURL, toOpenURLsWithScheme: scheme) { error in
        if let error {
            print("\(scheme): \(error.localizedDescription)")
            failed = true
        }
        pending -= 1
    }
}

while pending > 0 {
    RunLoop.main.run(until: Date().addingTimeInterval(0.1))
}

exit(failed ? 1 : 0)
