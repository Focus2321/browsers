// Preview harness for the native Settings and About windows: shows them with sample data, without
// the Rust app. Build and run with ./run.sh, see there for the options.
//
// A small stand-in for the Rust side answers the events the way the app does (hide a row, collapse
// a group, edit a rule, ...) with a fresh state through browsers_settings_update, synchronously
// from inside the callback like Rust may.
//
// Prints "WINDOW <number>" for `screencapture -l`, "ORIGIN <x>,<y>" (the window's top-left corner
// in top-left screen coordinates, for real mouse input), and "EVENT <json>" for every event.
import AppKit

setvbuf(stdout, nil, _IOLBF, 0)

// MARK: arguments

var appearance = "system"
var page = "browsers"
var seconds = 5.0
var showAbout = false
var standardAbout = false
var script: [String] = []
var answer = true
var showHidden = true
var bannerShown = true
var isDefault = false
var testURL = ""
var startWithDraft = false
var shotDir = "/tmp/settings-shots"

var args = CommandLine.arguments.dropFirst().makeIterator()
while let arg = args.next() {
    switch arg {
    case "--dark": appearance = "dark"
    case "--light": appearance = "light"
    case "--page": page = args.next() ?? page
    case "--about": showAbout = true
    // the standard AppKit About panel with the same icon, to compare against
    case "--standard-about": standardAbout = true
    case "--seconds": seconds = Double(args.next() ?? "") ?? seconds
    // steps separated by ";", see run.sh
    case "--script": script += (args.next() ?? "").split(separator: ";").map { $0.trimmingCharacters(in: .whitespaces) }
    // events are only printed, nothing answers them
    case "--no-answer": answer = false
    case "--no-hidden": showHidden = false
    case "--no-banner": bannerShown = false
    case "--default": isDefault = true
    case "--test-url": testURL = args.next() ?? ""
    case "--draft": startWithDraft = true
    case "--shots": shotDir = args.next() ?? shotDir
    default:
        FileHandle.standardError.write(Data("unknown option \(arg)\n".utf8))
        exit(2)
    }
}

// MARK: sample icons

let iconDir = URL(fileURLWithPath: "/tmp/settings-preview-icons")
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
func appIcon(_ bundleID: String, _ name: String, pixels: Int = 64) -> String {
    guard let app = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleID) else { return "" }
    return writePNG(NSWorkspace.shared.icon(forFile: app.path), pixels: pixels, to: name)
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

let icons: [String: String] = [
    "safari": appIcon("com.apple.Safari", "safari.png"),
    "chrome": appIcon("com.google.Chrome", "chrome.png"),
    "brave": appIcon("com.brave.Browser", "brave.png"),
    "firefox": appIcon("org.mozilla.firefox", "firefox.png"),
    "zen": appIcon("app.zen-browser.zen", "zen.png"),
    "slack": appIcon("com.tinyspeck.slackmacgap", "slack.png"),
    "zoom": appIcon("us.zoom.xos", "zoom.png"),
    "browsers": appIcon("software.Browsers", "browsers.png", pixels: 256),
    "picture": profilePicture(),
]

// MARK: the stand-in Rust side

struct Profile {
    var id: String
    var name: String
    var letterColor: String
    var picture = ""
    var hidden = false
}

struct App {
    var id: String
    var name: String
    var icon: String
    var letterColor: String
    var privateOK = true
    var profiles: [Profile]
}

struct Rule {
    var pattern: String
    var edit: String? = nil
    var source = ""
    var target = ""
    var isPrivate = false
}

struct Draft {
    var pattern = ""
    var source = ""
    var target: String? = nil
    var showInvalid = false
}

var apps: [App] = [
    App(id: "/Applications/Safari.app", name: "Safari", icon: icons["safari"]!, letterColor: "#5b8def", privateOK: false,
        profiles: [Profile(id: "/Applications/Safari.app", name: "", letterColor: "#5b8def")]),
    App(id: "/Applications/Google Chrome.app", name: "Chrome", icon: icons["chrome"]!, letterColor: "#e5734a", profiles: [
        Profile(id: "/Applications/Google Chrome.app#Default", name: "Tallgrass", letterColor: "#7c5ce6"),
        Profile(id: "/Applications/Google Chrome.app#Profile 1", name: "TF", letterColor: "#3e9d68", hidden: true),
        Profile(id: "/Applications/Google Chrome.app#Profile 2", name: "Your Chrome", letterColor: "#d0527e", picture: icons["picture"]!),
    ]),
    App(id: "/Applications/Brave Browser.app", name: "Brave", icon: icons["brave"]!, letterColor: "#e0623a",
        profiles: [Profile(id: "/Applications/Brave Browser.app", name: "", letterColor: "#e0623a")]),
    App(id: "/Applications/Firefox.app", name: "Firefox", icon: icons["firefox"]!, letterColor: "#e8853d",
        profiles: [Profile(id: "/Applications/Firefox.app", name: "", letterColor: "#e8853d", hidden: true)]),
    App(id: "/Applications/Zen.app", name: "Zen", icon: icons["zen"]!, letterColor: "#2f7fd8", profiles: [
        Profile(id: "/Applications/Zen.app#Personal", name: "Personal", letterColor: "#2f7fd8"),
        Profile(id: "/Applications/Zen.app#Work", name: "Work", letterColor: "#c2496f"),
    ]),
]
var special: [App] = [
    App(id: "/Applications/Slack.app", name: "Slack", icon: icons["slack"]!, letterColor: "#a66de8", privateOK: false,
        profiles: [Profile(id: "/Applications/Slack.app#T0123", name: "Acme", letterColor: "#a66de8")]),
    App(id: "/Applications/zoom.us.app", name: "zoom.us", icon: icons["zoom"]!, letterColor: "#2d8cff", privateOK: false,
        profiles: [Profile(id: "/Applications/zoom.us.app", name: "", letterColor: "#2d8cff")]),
]
var collapsed: Set<String> = []
var filter = ""
var defaultProfile = "/Applications/Google Chrome.app#Default"
var theme = appearance
var general: [String: Bool] = [
    "native_popup": true, "show_hotkeys": true, "quit_on_lost_focus": false,
    "unwrap_urls": true, "run_in_background": false,
]
var foundText = "Found 9 apps · checked 2 min ago"
var rules: [Rule] = [
    Rule(pattern: "github.com/acme/**", source: "com.tinyspeck.slackmacgap", target: "/Applications/Google Chrome.app#Default"),
    Rule(pattern: "*.notion.so", target: "/Applications/Safari.app"),
    Rule(pattern: "docs.google.com", edit: "docs.google.com/[drive", target: "/Applications/Zen.app#Work", isPrivate: true),
]
var draft: Draft? = startWithDraft ? Draft(pattern: "", source: "com.apple.mail") : nil
var testSource = ""
var fallbackTarget = ""
var fallbackPrivate = false

let sourceOptions: [(String, String)] = [
    ("", "Any app"), ("com.google.Chrome", "Chrome"), ("com.apple.Safari", "Safari"),
    ("com.tinyspeck.slackmacgap", "Slack"), ("us.zoom.xos", "zoom.us"), ("com.apple.mail", "Mail"),
    ("com.apple.MobileSMS", "Messages"),
]

func allProfiles() -> [(App, Profile)] {
    apps.flatMap { app in app.profiles.map { (app, $0) } }
}

func targetLabel(_ id: String) -> (String, String, Bool) {
    if id.isEmpty { return ("Show the picker", "", false) }
    guard let (app, profile) = allProfiles().first(where: { $0.1.id == id }) else { return ("Missing app", "", false) }
    let label = profile.name.isEmpty ? app.name : "\(profile.name) · \(app.name)"
    return (label + (profile.hidden ? " (hidden)" : ""), app.icon, app.privateOK)
}

func sourceLabel(_ id: String) -> String {
    id.isEmpty ? "" : sourceOptions.first { $0.0 == id }?.1 ?? id
}

func patternValid(_ pattern: String) -> Bool {
    !pattern.contains("[") && !pattern.contains(" ")
}

// a rough stand-in for test_opening_rules
func matches(_ rule: Rule, _ url: String) -> Bool {
    guard rule.edit == nil else { return false }
    if !rule.source.isEmpty && rule.source != testSource { return false }
    let core = rule.pattern.replacingOccurrences(of: "*.", with: "").replacingOccurrences(of: "/**", with: "")
    return core.isEmpty || url.contains(core)
}

func rowJSON(_ app: App, _ profile: Profile?, kind: String, draggable: Bool) -> [String: Any] {
    let isHeader = kind == "header"
    let id = isHeader ? "group:\(app.id)" : profile!.id
    let hidden = isHeader ? app.profiles.allSatisfy(\.hidden) : profile!.hidden
    let isDefaultRow = !isHeader && profile!.id == defaultProfile
    var menu: [[String: String]] = []
    if draggable {
        menu = [["action": "move_to_top", "title": "Move to top"], ["action": "move_to_bottom", "title": "Move to bottom"]]
    }
    if !isHeader {
        menu.append(["action": "toggle_default", "title": isDefaultRow ? "Stop opening unmatched links here" : "Open unmatched links here"])
    }
    let hiddenCount = app.profiles.filter(\.hidden).count
    let detail: String
    switch kind {
    case "header": detail = "\(app.profiles.count) profiles" + (hiddenCount > 0 ? " · \(hiddenCount) hidden" : "")
    case "single": detail = profile!.name
    default: detail = ""
    }
    let name = kind == "child" ? profile!.name : app.name
    let icon = kind == "child" ? profile!.picture : app.icon
    return [
        "id": id, "kind": kind, "app_id": app.id, "name": name, "detail": detail, "icon_path": icon,
        "letter": String(name.prefix(1)).uppercased(),
        "letter_color": kind == "child" ? profile!.letterColor : app.letterColor,
        "hidden": hidden, "is_default": isDefaultRow, "collapsed": isHeader && collapsed.contains(app.id) && filter.isEmpty,
        "draggable": draggable, "menu": menu,
    ]
}

func rows(_ list: [App], orderable: Bool) -> [[String: Any]] {
    let draggable = orderable && filter.isEmpty
    var out: [[String: Any]] = []
    for app in list {
        let shown = app.profiles.filter { (showHidden || !$0.hidden) && (filter.isEmpty || "\(app.name) \($0.name)".localizedCaseInsensitiveContains(filter)) }
        if shown.isEmpty { continue }
        if app.profiles.count > 1 {
            out.append(rowJSON(app, nil, kind: "header", draggable: draggable))
            if !collapsed.contains(app.id) || !filter.isEmpty {
                out += shown.map { rowJSON(app, $0, kind: "child", draggable: draggable) }
            }
        } else {
            out.append(rowJSON(app, shown[0], kind: "single", draggable: draggable))
        }
    }
    return out
}

func settingsState() -> [String: Any] {
    let matched = testURL.isEmpty || !testURL.contains(".") ? nil : rules.firstIndex { matches($0, testURL) }
    var test: [String: Any] = ["url": testURL, "source_id": testSource, "source_label": sourceLabel(testSource).isEmpty ? "Any app" : sourceLabel(testSource)]
    if testURL.isEmpty {
        test["result_kind"] = "none"; test["result_text"] = ""
    } else if !testURL.contains(".") {
        test["result_kind"] = "invalid"; test["result_text"] = "That isn't a link yet"
    } else if let matched {
        let (label, _, _) = targetLabel(rules[matched].target)
        test["result_kind"] = "rule"
        test["result_text"] = "Rule \(matched + 1) matches → opens in \(label)" + (rules[matched].isPrivate ? " (private)" : "")
    } else {
        let (label, _, _) = targetLabel(fallbackTarget)
        test["result_kind"] = "fallback"
        test["result_text"] = fallbackTarget.isEmpty ? "No rule matches → shows the picker" : "No rule matches → opens in \(label)"
    }
    let fallbackHit = (test["result_kind"] as? String) == "fallback"
    let (fallbackLabel, fallbackIcon, fallbackPrivateOK) = targetLabel(fallbackTarget)

    var draftJSON: Any = NSNull()
    if let draft {
        let valid = patternValid(draft.pattern)
        let invalid = !valid && draft.showInvalid
        let help: String
        if invalid {
            help = "That pattern isn't valid. Use a domain like notion.so, or * and ** as wildcards."
        } else if draft.pattern.isEmpty {
            help = "Type a domain or a pattern like github.com/acme/**. Saved once it has a pattern and a target."
        } else {
            help = "Matches \(draft.pattern) and every page on it. Saved once it has a pattern and a target."
        }
        draftJSON = [
            "pattern": draft.pattern,
            "pattern_placeholder": draft.source.isEmpty ? "e.g. notion.so" : "any link",
            "pattern_invalid": invalid,
            "source_id": draft.source, "source_label": sourceLabel(draft.source),
            "target_id": draft.target.map { $0 as Any } ?? NSNull(),
            "target_label": draft.target.map { targetLabel($0).0 } ?? "",
            "help": help,
        ] as [String: Any]
    }

    let hiddenCount = (apps + special).flatMap(\.profiles).filter(\.hidden).count
    let ordered = rows(apps, orderable: true)
    let specials = rows(special, orderable: false)
    return [
        "appearance": theme,
        "is_default_browser": isDefault,
        "browsers": [
            "filter": filter, "show_hidden": showHidden, "hidden_count": hiddenCount,
            "reorder_enabled": filter.isEmpty,
            "order_hint": filter.isEmpty ? "Drag to reorder · 1–9 follow this order" : "Clear the filter to reorder",
            "banner": bannerShown ? [
                "title": "2 of these aren't web browsers",
                "message": "Slack and zoom.us can open links, but you probably don't want them in the popup.",
            ] as Any : NSNull(),
            "found_text": foundText,
            "empty_text": filter.isEmpty ? "No apps found. Try Rescan." : "Nothing matches \"\(filter)\"",
            "rows": ordered, "special_rows": specials,
        ] as [String: Any],
        "rules": [
            "rules": rules.enumerated().map { index, rule -> [String: Any] in
                let (label, icon, privateOK) = targetLabel(rule.target)
                return [
                    "pattern": rule.pattern, "pattern_invalid": rule.edit != nil, "pattern_edit": rule.edit ?? "",
                    "source_id": rule.source, "source_label": sourceLabel(rule.source),
                    "target_id": rule.target, "target_label": label, "target_icon_path": icon,
                    "private": rule.isPrivate, "supports_private": privateOK, "matched": index == matched,
                ]
            },
            "source_options": sourceOptions.map { ["id": $0.0, "label": $0.1] },
            "target_options": [["id": "", "label": "Show the picker", "icon_path": ""]] + allProfiles().map { app, profile in
                ["id": profile.id, "label": targetLabel(profile.id).0, "icon_path": app.icon]
            },
            "draft": draftJSON,
            "test": test,
            "fallback": [
                "target_id": fallbackTarget, "target_label": fallbackLabel, "target_icon_path": fallbackIcon,
                "private": fallbackPrivate, "supports_private": fallbackPrivateOK, "matched": fallbackHit,
            ] as [String: Any],
        ] as [String: Any],
        "general": [
            "theme": theme, "native_popup_available": true, "native_popup": general["native_popup"]!,
            "show_hotkeys": general["show_hotkeys"]!,
            "quit_on_lost_focus_visible": !general["run_in_background"]!,
            "quit_on_lost_focus": general["quit_on_lost_focus"]!,
            "unwrap_urls": general["unwrap_urls"]!, "run_in_background": general["run_in_background"]!,
        ] as [String: Any],
        "about": aboutState(withAppearance: false),
    ]
}

func aboutState(withAppearance: Bool) -> [String: Any] {
    let home = "/Users/me"
    var about: [String: Any] = [
        "app_icon_path": icons["browsers"]!,
        "app_name": "Browsers",
        "version_text": "Version 0.6.0",
        "copyright": "© 2022-2024 Browsers.software team.",
        "website_url": "https://browsers.software",
        "directories": [
            ["label": "Config", "path": "\(home)/Library/Application Support/software.Browsers/"],
            ["label": "Cache", "path": "\(home)/Library/Caches/software.Browsers/"],
            ["label": "Logs", "path": "\(home)/Library/Logs/software.Browsers/"],
            ["label": "Resources", "path": "/Applications/Browsers.app/Contents/Resources/"],
        ],
        "made_with_slint": ["label": "Made with Slint", "version_text": "Version 1.18.1", "url": "https://slint.dev"],
    ]
    if withAppearance { about["appearance"] = theme }
    return about
}

func json(_ object: [String: Any]) -> String {
    String(data: try! JSONSerialization.data(withJSONObject: object), encoding: .utf8)!
}

// what the visible rows are, by index, like Rust's Local.rows
func listedRows(_ list: String) -> [[String: Any]] {
    let state = settingsState()["browsers"] as! [String: Any]
    return state[list] as! [[String: Any]]
}

func moveApp(_ appID: String, before target: String?) {
    guard let from = apps.firstIndex(where: { $0.id == appID }) else { return }
    let app = apps.remove(at: from)
    let to = target.flatMap { t in apps.firstIndex { $0.id == t } } ?? apps.count
    apps.insert(app, at: to)
}

func moveProfile(_ id: String, in appID: String, before target: String?) {
    guard let a = apps.firstIndex(where: { $0.id == appID }),
          let from = apps[a].profiles.firstIndex(where: { $0.id == id }) else { return }
    let profile = apps[a].profiles.remove(at: from)
    let to = target.flatMap { t in apps[a].profiles.firstIndex { $0.id == t } } ?? apps[a].profiles.count
    apps[a].profiles.insert(profile, at: to)
}

func handle(_ event: [String: Any]) {
    let type = event["type"] as? String ?? ""
    func int(_ key: String) -> Int { event[key] as? Int ?? -1 }
    func string(_ key: String) -> String { event[key] as? String ?? "" }
    func bool(_ key: String) -> Bool { event[key] as? Bool ?? false }

    switch type {
    case "settings_closed", "about_closed": return
    case "set_default_browser": isDefault = true
    case "browsers_filter_changed": filter = string("text")
    case "set_show_hidden": showHidden = bool("show")
    case "rescan": foundText = "Found 9 apps · checked just now"
    case "hide_non_browsers":
        for i in special.indices { for j in special[i].profiles.indices { special[i].profiles[j].hidden = true } }
        bannerShown = false
    case "dismiss_non_browser_banner": bannerShown = false
    case "toggle_row_visibility":
        let list = string("list")
        let listed = listedRows(list)
        guard listed.indices.contains(int("row")), listed[int("row")]["id"] as? String == string("row_id") else {
            print("STALE \(event)"); return
        }
        let row = listed[int("row")]
        let appID = row["app_id"] as! String
        let isHeader = row["kind"] as? String == "header"
        let hide = !(row["hidden"] as! Bool)
        func apply(_ list: inout [App]) {
            guard let a = list.firstIndex(where: { $0.id == appID }) else { return }
            for j in list[a].profiles.indices where isHeader || list[a].profiles[j].id == row["id"] as! String {
                list[a].profiles[j].hidden = hide
            }
        }
        if list == "rows" { apply(&apps) } else { apply(&special) }
    case "set_group_collapsed":
        if bool("collapsed") { collapsed.insert(string("app_id")) } else { collapsed.remove(string("app_id")) }
    case "row_menu":
        let listed = listedRows(string("list"))
        guard listed.indices.contains(int("row")), listed[int("row")]["id"] as? String == string("row_id") else {
            print("STALE \(event)"); return
        }
        let row = listed[int("row")]
        let appID = row["app_id"] as! String
        switch (string("action"), row["kind"] as! String) {
        case ("toggle_default", _): defaultProfile = defaultProfile == string("row_id") ? "" : string("row_id")
        case ("move_to_top", "child"):
            moveProfile(string("row_id"), in: appID, before: apps.first { $0.id == appID }?.profiles.first?.id)
        case ("move_to_bottom", "child"): moveProfile(string("row_id"), in: appID, before: nil)
        case ("move_to_top", _): moveApp(appID, before: apps.first?.id)
        case ("move_to_bottom", _): moveApp(appID, before: nil)
        default: break
        }
    case "move_browser_row":
        let listed = listedRows("rows")
        guard listed.indices.contains(int("row")), listed[int("row")]["id"] as? String == string("row_id") else {
            print("STALE \(event)"); return
        }
        let row = listed[int("row")]
        let to = int("to")
        let appID = row["app_id"] as! String
        let before = to < listed.count ? listed[to] : nil
        if row["kind"] as? String == "child" {
            let target = before.flatMap { $0["app_id"] as? String == appID && $0["kind"] as? String == "child" ? $0["id"] as? String : nil }
            moveProfile(string("row_id"), in: appID, before: target)
        } else {
            moveApp(appID, before: before?["app_id"] as? String)
        }
    case "add_rule": if draft == nil { draft = Draft() }
    case "rule_pattern_edited":
        guard rules.indices.contains(int("index")) else { return }
        let pattern = string("pattern")
        if patternValid(pattern) { rules[int("index")].pattern = pattern; rules[int("index")].edit = nil } else { rules[int("index")].edit = pattern }
    case "rule_source_picked": if rules.indices.contains(int("index")) { rules[int("index")].source = string("source_id") }
    case "rule_target_picked": if rules.indices.contains(int("index")) { rules[int("index")].target = string("target_id") }
    case "rule_private_set": if rules.indices.contains(int("index")) { rules[int("index")].isPrivate = bool("private") }
    case "rule_duplicate": if rules.indices.contains(int("index")) { rules.insert(rules[int("index")], at: int("index") + 1) }
    case "rule_delete": if rules.indices.contains(int("index")) { rules.remove(at: int("index")) }
    case "move_rule":
        let from = int("from"), to = int("to")
        guard rules.indices.contains(from), (0...rules.count).contains(to) else { return }
        let rule = rules.remove(at: from)
        rules.insert(rule, at: to > from ? to - 1 : to)
    case "draft_pattern_edited": draft?.pattern = string("text")
    case "draft_pattern_committed": draft?.showInvalid = true
    case "draft_source_picked": draft?.source = string("source_id")
    case "draft_target_picked": draft?.target = string("target_id"); draft?.showInvalid = true
    case "draft_discarded": draft = nil
    case "test_url_edited": testURL = string("text")
    case "test_source_picked": testSource = string("source_id")
    case "fallback_target_picked": fallbackTarget = string("target_id")
    case "fallback_private_set": fallbackPrivate = bool("private")
    case "set_theme": theme = string("theme")
    case "set_native_popup": general["native_popup"] = bool("value")
    case "set_show_hotkeys": general["show_hotkeys"] = bool("value")
    case "set_quit_on_lost_focus": general["quit_on_lost_focus"] = bool("value")
    case "set_unwrap_urls": general["unwrap_urls"] = bool("value")
    case "set_run_in_background": general["run_in_background"] = bool("value")
    default:
        print("UNKNOWN \(type)")
        return
    }
    // a complete draft is saved as a rule (Draft::is_complete)
    if let d = draft, let target = d.target, (!d.pattern.isEmpty && patternValid(d.pattern)) || !d.source.isEmpty {
        rules.append(Rule(pattern: d.pattern, source: d.source, target: target))
        draft = nil
    }
    sendUpdate()
}

func sendUpdate() {
    json(settingsState()).withCString { browsers_settings_update($0) }
    if browsers_about_is_open() {
        json(aboutState(withAppearance: true)).withCString { browsers_about_update($0) }
    }
}

// MARK: synthetic input

func keyEvent(_ type: NSEvent.EventType, _ chars: String, keyCode: UInt16, flags: NSEvent.ModifierFlags, window: NSWindow) -> NSEvent {
    NSEvent.keyEvent(
        with: type, location: .zero, modifierFlags: flags, timestamp: ProcessInfo.processInfo.systemUptime,
        windowNumber: window.windowNumber, context: nil, characters: chars,
        charactersIgnoringModifiers: chars, isARepeat: false, keyCode: keyCode)!
}

let namedKeys: [String: (String, UInt16)] = [
    "Return": ("\r", 36), "Esc": ("\u{1b}", 53), "Space": (" ", 49), "Tab": ("\t", 48),
    "Backspace": ("\u{7f}", 51), "Up": ("\u{f700}", 126), "Down": ("\u{f701}", 125),
    "Left": ("\u{f702}", 123), "Right": ("\u{f703}", 124),
]

func press(_ token: String, in window: NSWindow) {
    if token.hasPrefix("Cmd+") || token.hasPrefix("CmdShift+") {
        let shift = token.hasPrefix("CmdShift+")
        let key = String(token.dropFirst(shift ? 9 : 4))
        let flags: NSEvent.ModifierFlags = shift ? [.command, .shift] : [.command]
        let event = keyEvent(.keyDown, key, keyCode: 0, flags: flags, window: window)
        // what NSApplication does with a ⌘ key: offer it to the key window first
        let handled = window.performKeyEquivalent(with: event)
        print("KEY \(token) handled=\(handled)")
        return
    }
    let (chars, code) = namedKeys[token] ?? (token, 0)
    window.sendEvent(keyEvent(.keyDown, chars, keyCode: code, flags: [], window: window))
    window.sendEvent(keyEvent(.keyUp, chars, keyCode: code, flags: [], window: window))
}

// a click at a point in top-left window coordinates
func click(_ x: CGFloat, _ y: CGFloat, in window: NSWindow) {
    let point = NSPoint(x: x, y: window.frame.height - y)
    for (i, type) in [NSEvent.EventType.leftMouseDown, .leftMouseUp].enumerated() {
        let event = NSEvent.mouseEvent(
            with: type, location: point, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!
        // the mouse-up is queued, so it's also handled from inside a tracking loop the down starts
        if i == 0 { window.sendEvent(event) } else { NSApp.postEvent(event, atStart: false) }
    }
}

// keys for an open menu, queued so its tracking loop picks them up
func postMenuKeys(_ tokens: [String], in window: NSWindow) {
    for token in tokens {
        guard let (chars, code) = namedKeys[token] else { continue }
        for type in [NSEvent.EventType.keyDown, .keyUp] {
            NSApp.postEvent(keyEvent(type, chars, keyCode: code, flags: [], window: window), atStart: false)
        }
    }
}

func capture(_ window: NSWindow, _ name: String) {
    try? FileManager.default.createDirectory(atPath: shotDir, withIntermediateDirectories: true)
    let path = "\(shotDir)/\(name).png"
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    process.arguments = ["-o", "-x", "-l", "\(window.windowNumber)", path]
    try? process.run()
    process.waitUntilExit()
    print("SHOT \(path)")
}

@MainActor
func describeFocus(_ window: NSWindow) {
    let responder = window.firstResponder
    var text = responder.map { String(describing: type(of: $0)) } ?? "nil"
    if let editor = responder as? NSTextView, editor.isFieldEditor { text += " \"\(editor.string)\"" }
    print("FOCUS \(text) page=\(SettingsController.shared.model.page.rawValue)")
}

func printOrigin(_ window: NSWindow) {
    let screenHeight = NSScreen.screens.first?.frame.height ?? 0
    print("ORIGIN \(Int(window.frame.minX)),\(Int(screenHeight - window.frame.maxY)) SIZE \(Int(window.frame.width))x\(Int(window.frame.height))")
}

// MARK: run

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
// the same frame every run, for comparable screenshots
MainActor.assumeIsolated {
    UserDefaults.standard.removeObject(forKey: "NSWindow Frame \(SettingsController.frameAutosaveName)")
}

browsers_settings_init { json in
    guard let json else { return }
    let text = String(cString: json)
    print("EVENT \(text)")
    guard answer, let object = (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any] else { return }
    MainActor.assumeIsolated { handle(object) }
}

func after(_ delay: TimeInterval, _ block: @escaping @MainActor () -> Void) {
    // .common so it also fires while a menu is tracking
    let timer = Timer(timeInterval: delay, repeats: false) { _ in MainActor.assumeIsolated { block() } }
    RunLoop.main.add(timer, forMode: .common)
}

@MainActor
func currentWindow() -> NSWindow? {
    if let window = NSApp.keyWindow { return window }
    return SettingsController.shared.window?.isVisible == true ? SettingsController.shared.window : AboutController.shared.window
}

// Steps, separated by ";":
//   wait:S  click:X,Y  keys:TOKEN,TOKEN  type:TEXT  menu:Down,Return  update  show[:PAGE]  close
//   about  about-update  shot:NAME  focus  is-open  origin
@MainActor
func run(_ step: String) {
    let (name, value): (String, String) = {
        guard let colon = step.firstIndex(of: ":") else { return (step, "") }
        return (String(step[..<colon]), String(step[step.index(after: colon)...]))
    }()
    print("STEP \(step)")
    // these work with no window on screen
    switch name {
    case "update": sendUpdate(); return
    case "show":
        json(settingsState()).withCString { state in
            if value.isEmpty { browsers_settings_show(state, nil) } else { value.withCString { browsers_settings_show(state, $0) } }
        }
        return
    case "close": browsers_settings_close(); return
    case "about": json(aboutState(withAppearance: true)).withCString { browsers_about_show($0) }; return
    case "about-update": json(aboutState(withAppearance: true)).withCString { browsers_about_update($0) }; return
    case "about-close": browsers_about_close(); return
    case "theme": theme = value; sendUpdate(); return
    case "is-open": print("OPEN settings=\(browsers_settings_is_open()) about=\(browsers_about_is_open())"); return
    default: break
    }
    guard let window = currentWindow() else { print("NO WINDOW"); return }
    switch name {
    case "click":
        let parts = value.split(separator: ",").compactMap { Double($0) }
        click(parts[0], parts[1], in: window)
    case "keys": value.split(separator: ",").forEach { press(String($0), in: window) }
    case "type": value.forEach { press(String($0), in: window) }
    case "menu": postMenuKeys(value.split(separator: ",").map(String.init), in: window)
    case "shot": capture(window, value)
    case "focus": describeFocus(window)
    case "origin": printOrigin(window)
    case "scroll":
        // scrolls the scroll view under X,Y (top-left window coordinates) by DY points, positive = down
        let parts = value.split(separator: ",").compactMap { Double($0) }
        guard let content = window.contentView else { break }
        let point = content.convert(NSPoint(x: parts[0], y: window.frame.height - parts[1]), from: nil)
        func scrollViews(_ view: NSView) -> [NSScrollView] {
            (view as? NSScrollView).map { [$0] } ?? [] + view.subviews.flatMap(scrollViews)
        }
        let hit = scrollViews(content).last { $0.convert($0.bounds, to: content).contains(point) }
        guard let scrollView = hit else { print("NO SCROLL VIEW"); break }
        let clip = scrollView.contentView
        let maxY = (scrollView.documentView?.frame.height ?? 0) - clip.bounds.height
        clip.scroll(to: NSPoint(x: clip.bounds.minX, y: min(max(0, clip.bounds.minY + parts[2]), max(0, maxY))))
        scrollView.reflectScrolledClipView(clip)
    case "toolbar":
        let toolbar = window.toolbar
        print("TOOLBAR \(toolbar.map { "visible=\($0.isVisible) items=\($0.items.map(\.itemIdentifier.rawValue))" } ?? "none") titleVisibility=\(window.titleVisibility.rawValue) title=\(window.title)")
    default: print("UNKNOWN STEP \(step)")
    }
}

after(0.1) {
    app.activate(ignoringOtherApps: true)
    let window: NSWindow
    if standardAbout {
        app.orderFrontStandardAboutPanel(options: [
            .applicationIcon: NSImage(contentsOfFile: icons["browsers"]!) ?? NSImage(),
            .applicationName: "Browsers",
            .applicationVersion: "0.6.0",
            .version: "",
            .credits: NSAttributedString(string: "Made with Slint"),
        ])
        guard let panel = NSApp.windows.first(where: { $0.isVisible }) else { exit(1) }
        window = panel
    } else if showAbout {
        json(aboutState(withAppearance: true)).withCString { browsers_about_show($0) }
        window = AboutController.shared.window!
    } else {
        json(settingsState()).withCString { state in page.withCString { browsers_settings_show(state, $0) } }
        window = SettingsController.shared.window!
        if !testURL.isEmpty {
            let text = testURL
            testURL = ""
            SettingsController.shared.model.setTestURL(text)
        }
    }
    print("WINDOW \(window.windowNumber)")
    printOrigin(window)

    var delay = 0.8
    for step in script {
        if step.hasPrefix("wait:") {
            delay += Double(step.dropFirst(5)) ?? 0
            continue
        }
        after(delay) { run(step) }
        delay += 0.35
    }
    after(max(seconds, delay + 0.3)) {
        browsers_settings_close()
        browsers_about_close()
        exit(0)
    }
}

app.run()
