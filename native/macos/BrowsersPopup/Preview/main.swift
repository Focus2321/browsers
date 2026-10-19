// Preview harness for the native popup: shows it with sample browsers, without the Rust app.
// Build and run with ./run.sh, see there for the options.
//
// Prints "WINDOW <number>" once the panel is up (for `screencapture -l`), "EVENT <json>" for every
// event the popup sends, and "CAPTURE <x>,<y>,<w>,<h>" (top-left screen coordinates, for
// `screencapture -R`) once the options menu is open with --menu.
import AppKit

setvbuf(stdout, nil, _IOLBF, 0)

// MARK: arguments

var appearance = "system"
var url = "https://github.com/slint-ui/slint/pull/123"
var seconds = 4.0
var keyTokens: [String] = []
var openMenu = false
var many = false
var noBrowsers = false
var hideHotkeys = false
var sourceApp = "Slack"
var clickRow: Int?
var contextRow: Int?
var sendUpdate = false
var reshow = false
var backdrop = false
var keysAfterUpdate: [String] = []
var menuKeys: [String] = []

var args = CommandLine.arguments.dropFirst().makeIterator()
while let arg = args.next() {
    switch arg {
    case "--dark": appearance = "dark"
    case "--light": appearance = "light"
    // the filter, ⇧ and the selection go through the same synthetic keys as --keys, so they
    // exercise the real keyboard handling
    case "--filter": keyTokens += (args.next() ?? "").map { String($0) }
    case "--private": keyTokens.append("Shift")
    case "--select": keyTokens += Array(repeating: "Down", count: Int(args.next() ?? "") ?? 0)
    case "--keys": keyTokens += (args.next() ?? "").split(separator: ",").map(String.init)
    case "--url": url = args.next() ?? ""
    case "--source": sourceApp = args.next() ?? ""
    case "--seconds": seconds = Double(args.next() ?? "") ?? seconds
    case "--menu": openMenu = true
    case "--click": clickRow = Int(args.next() ?? "")
    case "--context": contextRow = Int(args.next() ?? "")
    // after the keys, replaces the content with the last 3 browsers left out
    case "--update": sendUpdate = true
    // after the keys, hides and shows the popup again, like the next link would
    case "--reshow": reshow = true
    // a wallpaper-like window behind the popup, so screenshots don't pick up the real screen
    case "--backdrop": backdrop = true
    // keys for the open (context or options) menu, queued so its tracking loop picks them up
    case "--menu-keys": menuKeys += (args.next() ?? "").split(separator: ",").map(String.init)
    case "--then": keysAfterUpdate += (args.next() ?? "").split(separator: ",").map(String.init)
    case "--many": many = true
    case "--none": noBrowsers = true
    case "--no-hotkeys": hideHotkeys = true
    default:
        FileHandle.standardError.write(Data("unknown option \(arg)\n".utf8))
        exit(2)
    }
}

// MARK: sample icons

let iconDir = URL(fileURLWithPath: "/tmp/popup-icons")
try? FileManager.default.createDirectory(at: iconDir, withIntermediateDirectories: true)

func writePNG(_ image: NSImage, pixels: Int, to name: String) -> String {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
        bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = NSSize(width: pixels, height: pixels)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    image.draw(in: NSRect(x: 0, y: 0, width: pixels, height: pixels))
    NSGraphicsContext.restoreGraphicsState()
    let path = iconDir.appendingPathComponent(name).path
    try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
    return path
}

// app icon PNG, like the ones the Rust side caches; empty when the app isn't installed
func appIcon(_ bundleID: String, _ name: String) -> String {
    guard let app = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleID) else { return "" }
    return writePNG(NSWorkspace.shared.icon(forFile: app.path), pixels: 64, to: name)
}

// a stand-in profile picture
func profilePicture() -> String {
    let image = NSImage(size: NSSize(width: 64, height: 64), flipped: false) { rect in
        NSGradient(colors: [.systemTeal, .systemIndigo])!.draw(in: rect, angle: -60)
        let symbol = NSImage(systemSymbolName: "person.fill", accessibilityDescription: nil)!
            .withSymbolConfiguration(.init(pointSize: 34, weight: .regular))!
        let tinted = NSImage(size: symbol.size, flipped: false) { r in
            symbol.draw(in: r)
            NSColor.white.withAlphaComponent(0.9).set()
            r.fill(using: .sourceAtop)
            return true
        }
        tinted.draw(in: NSRect(x: (64 - symbol.size.width) / 2, y: 4, width: symbol.size.width, height: symbol.size.height))
        return true
    }
    return writePNG(image, pixels: 64, to: "profile.png")
}

let chrome = appIcon("com.google.Chrome", "chrome.png")
let safari = appIcon("com.apple.Safari", "safari.png")
let brave = appIcon("com.brave.Browser", "brave.png")
let firefox = appIcon("org.mozilla.firefox", "firefox.png")
let zen = appIcon("app.zen-browser.zen", "zen.png")
let picture = profilePicture()

func browser(
    _ index: Int, _ name: String, profile: String = "", icon: String,
    profiles: Bool = false, incognito: Bool = true, picture: String = "", color: String = ""
) -> [String: Any] {
    let appID = "/Applications/\(name).app/Contents/MacOS/\(name)"
    return [
        "profile_index": index,
        "unique_id": profiles ? "\(appID)#\(profile)" : appID,
        "unique_app_id": appID,
        "browser_name": name,
        "profile_name": profile,
        "full_name": profiles ? "\(name) (\(profile))" : name,
        "supports_profiles": profiles,
        "supports_incognito": incognito,
        "icon_path": icon,
        "profile_icon_path": picture,
        "profile_initial": profile.first.map { String($0).uppercased() } ?? "",
        "avatar_color": color,
    ]
}

var browsers: [[String: Any]] = [
    browser(0, "Chrome", profile: "Tallgrass", icon: chrome, profiles: true, color: "#7c5ce6"),
    browser(1, "Chrome", profile: "TF", icon: chrome, profiles: true, color: "#3e9d68"),
    browser(2, "Chrome", profile: "Your Chrome", icon: chrome, profiles: true, picture: picture),
    browser(3, "Safari", icon: safari, incognito: false),
    browser(4, "Brave", icon: brave),
    browser(5, "Firefox", icon: firefox),
    browser(6, "Zen", profile: "Default (release)", icon: zen, profiles: true, color: "#2f7fd8"),
]
if many {
    browsers += browsers.map { b in
        var b = b
        b["profile_index"] = (b["profile_index"] as! Int) + 7
        b["unique_id"] = "\(b["unique_id"]!)-2"
        return b
    }
}

if noBrowsers { browsers = [] }

let parsed = URL(string: url)
var path = parsed?.path ?? ""
if path == "/" { path = "" }
if let query = parsed?.query { path += "?" + query }
let state: [String: Any] = [
    "url": url,
    "url_domain": parsed?.host ?? "",
    "url_path": parsed?.host == nil ? url : path,
    "source_app_name": sourceApp,
    "show_hotkeys": !hideHotkeys,
    "show_set_as_default": true,
    "appearance": appearance,
    "browsers": browsers,
    "restorable": [["unique_id": "ff-work", "full_name": "Firefox (Work)"]],
]
func json(_ object: [String: Any]) -> String {
    String(data: try! JSONSerialization.data(withJSONObject: object), encoding: .utf8)!
}
let stateJSON = json(state)
var smallerState = state
smallerState["browsers"] = Array(browsers.dropLast(3))

// MARK: synthetic keys

// a key event straight into the panel - real global key injection gets swallowed by event taps
func keyEvent(_ type: NSEvent.EventType, _ chars: String, keyCode: UInt16, flags: NSEvent.ModifierFlags, window: NSWindow) -> NSEvent {
    NSEvent.keyEvent(
        with: type, location: .zero, modifierFlags: flags, timestamp: ProcessInfo.processInfo.systemUptime,
        windowNumber: window.windowNumber, context: nil, characters: chars,
        charactersIgnoringModifiers: chars, isARepeat: false, keyCode: keyCode)!
}

var heldFlags: NSEvent.ModifierFlags = []
let digitKeyCodes: [Character: UInt16] = ["1": 18, "2": 19, "3": 20, "4": 21, "5": 23, "6": 22, "7": 26, "8": 28, "9": 25]
let shiftedDigits: [Character: String] = ["1": "!", "2": "@", "3": "#", "4": "$", "5": "%", "6": "^", "7": "&", "8": "*", "9": "("]

func press(_ token: String, in panel: NSWindow) {
    print("KEY \(token)")
    let named: [String: (String, UInt16)] = [
        "Return": ("\r", 36), "Enter": ("\u{3}", 76), "Esc": ("\u{1b}", 53), "Space": (" ", 49),
        "Backspace": ("\u{7f}", 51), "Up": ("\u{f700}", 126), "Down": ("\u{f701}", 125),
    ]
    switch token {
    case "Shift", "-Shift":
        heldFlags = token == "Shift" ? [.shift] : []
        panel.sendEvent(keyEvent(.flagsChanged, "", keyCode: 56, flags: heldFlags, window: panel))
    case let t where t.hasPrefix("Cmd+"):
        // "comma", since a bare "," separates the --keys tokens
        let key = String(t.dropFirst(4))
        let chars = key == "comma" ? "," : key
        _ = panel.performKeyEquivalent(with: keyEvent(.keyDown, chars, keyCode: 0, flags: heldFlags.union(.command), window: panel))
    default:
        if let (chars, code) = named[token] {
            panel.sendEvent(keyEvent(.keyDown, chars, keyCode: code, flags: heldFlags, window: panel))
        } else if let c = token.first, token.count == 1, let code = digitKeyCodes[c] {
            // what a US keyboard types for the digit row, ⇧ included
            let chars = heldFlags.contains(.shift) ? shiftedDigits[c]! : token
            panel.sendEvent(keyEvent(.keyDown, chars, keyCode: code, flags: heldFlags, window: panel))
        } else {
            panel.sendEvent(keyEvent(.keyDown, token, keyCode: 0, flags: heldFlags, window: panel))
        }
    }
}

// MARK: mouse

// a synthetic click at a point in window coordinates (bottom-left origin)
func click(_ point: NSPoint, in panel: NSWindow, right: Bool = false) {
    let types: [NSEvent.EventType] = right ? [.rightMouseDown, .rightMouseUp] : [.leftMouseDown, .leftMouseUp]
    for (i, type) in types.enumerated() {
        let event = NSEvent.mouseEvent(
            with: type, location: point, modifierFlags: heldFlags, timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: panel.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!
        // the mouse-up is queued, so it's also handled from inside a tracking loop the down starts
        if i == 0 { panel.sendEvent(event) } else { NSApp.postEvent(event, atStart: false) }
    }
}

// middle of the given row, as long as the list isn't scrolled
func rowCenter(_ row: Int, in panel: NSWindow) -> NSPoint {
    NSPoint(x: 120, y: panel.frame.height - (50 + 1 + 5 + 40 * CGFloat(row) + 20))
}

func postMenuKeys(_ tokens: [String], in panel: NSWindow) {
    let named: [String: (String, UInt16)] = [
        "Return": ("\r", 36), "Up": ("\u{f700}", 126), "Down": ("\u{f701}", 125), "Right": ("\u{f703}", 124),
    ]
    for token in tokens {
        guard let (chars, code) = named[token] else { continue }
        for type in [NSEvent.EventType.keyDown, .keyUp] {
            NSApp.postEvent(keyEvent(type, chars, keyCode: code, flags: [], window: panel), atStart: false)
        }
    }
}

// wallpaper-like gradient window behind the panel (and where its menus open), returned as the
// region to capture in top-left screen coordinates
func showBackdrop(behind panel: NSWindow, dark: Bool) -> CGRect {
    let p = panel.frame
    let frame = NSRect(x: p.minX - 32, y: p.minY - 230, width: p.width + 300, height: p.height + 262)
    let window = NSWindow(contentRect: frame, styleMask: [.borderless], backing: .buffered, defer: false)
    let colors: [NSColor] = dark
        ? [NSColor(red: 0.09, green: 0.12, blue: 0.27, alpha: 1), NSColor(red: 0.33, green: 0.16, blue: 0.40, alpha: 1)]
        : [NSColor(red: 0.62, green: 0.78, blue: 0.95, alpha: 1), NSColor(red: 0.96, green: 0.80, blue: 0.84, alpha: 1)]
    window.contentView = GradientView(colors: colors)
    window.isReleasedWhenClosed = false
    window.orderFrontRegardless()
    let screenHeight = NSScreen.screens.first?.frame.height ?? 0
    return CGRect(x: frame.minX, y: screenHeight - frame.maxY, width: frame.width, height: frame.height)
}

final class GradientView: NSView {
    let colors: [NSColor]
    init(colors: [NSColor]) { self.colors = colors; super.init(frame: .zero) }
    required init?(coder: NSCoder) { nil }
    override func draw(_ rect: NSRect) { NSGradient(colors: colors)!.draw(in: bounds, angle: -55) }
}

func printFrame(_ label: String, _ panel: NSWindow) {
    let f = panel.frame
    print("FRAME \(label) x=\(Int(f.minX)) top=\(Int(f.maxY)) w=\(Int(f.width)) h=\(Int(f.height))")
}

func reportCaptureRect() {
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
    let mine = windows.filter { ($0[kCGWindowOwnerPID as String] as? Int32) == getpid() }
    let rects = mine.compactMap { w -> CGRect? in
        guard let bounds = w[kCGWindowBounds as String] as? NSDictionary else { return nil }
        return CGRect(dictionaryRepresentation: bounds)
    }
    guard let first = rects.first else { return }
    let union = rects.dropFirst().reduce(first) { $0.union($1) }.insetBy(dx: -24, dy: -24)
    print("CAPTURE \(Int(union.minX)),\(Int(union.minY)),\(Int(union.width)),\(Int(union.height))")
}

// MARK: run

let app = NSApplication.shared
app.setActivationPolicy(.accessory)

browsers_popup_init { json in
    guard let json else { return }
    print("EVENT \(String(cString: json))")
}

func after(_ delay: TimeInterval, _ block: @escaping @MainActor () -> Void) {
    // .common so it also fires while the options menu is tracking
    let timer = Timer(timeInterval: delay, repeats: false) { _ in MainActor.assumeIsolated { block() } }
    RunLoop.main.add(timer, forMode: .common)
}

after(0.1) {
    app.activate(ignoringOtherApps: true)
    stateJSON.withCString { browsers_popup_show($0) }
    let panel = PopupController.shared.window
    if backdrop {
        let region = showBackdrop(behind: panel, dark: appearance != "light")
        panel.orderFrontRegardless()
        print("BACKDROP \(Int(region.minX)),\(Int(region.minY)),\(Int(region.width)),\(Int(region.height))")
    }
    print("WINDOW \(panel.windowNumber)")
    print("VISIBLE \(browsers_popup_is_visible())")

    var delay = 0.4
    for token in keyTokens {
        after(delay) { press(token, in: panel) }
        delay += 0.12
    }
    if sendUpdate || reshow {
        after(delay + 0.1) {
            printFrame("before-update", panel)
            if reshow {
                browsers_popup_hide()
                print("VISIBLE \(browsers_popup_is_visible())")
                stateJSON.withCString { browsers_popup_show($0) }
            } else {
                json(smallerState).withCString { browsers_popup_update($0) }
            }
            printFrame("after-update", panel)
        }
        delay += 0.3
        for token in keysAfterUpdate {
            after(delay) { press(token, in: panel) }
            delay += 0.12
        }
    }
    if let clickRow {
        after(delay + 0.2) { click(rowCenter(clickRow, in: panel), in: panel) }
    }
    if let contextRow {
        after(delay + 0.2) { click(rowCenter(contextRow, in: panel), in: panel, right: true) }
        after(delay + 0.8) { reportCaptureRect() }
    }
    if openMenu {
        after(delay + 0.2) { click(NSPoint(x: panel.frame.width - 20, y: 15), in: panel) }
        after(delay + 0.8) { reportCaptureRect() }
    }
    if !menuKeys.isEmpty {
        after(delay + 1.0) { postMenuKeys(menuKeys, in: panel) }
    }
}

after(seconds) {
    browsers_popup_hide()
    print("VISIBLE \(browsers_popup_is_visible())")
    exit(0)
}

app.run()
