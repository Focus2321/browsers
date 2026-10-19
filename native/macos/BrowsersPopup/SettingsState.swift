// The Settings and About state JSON the Rust side sends (SETTINGS_CONTRACT.md "Settings state",
// "About state").
// Decoded leniently like PopupState: a missing or mistyped field falls back to its default, only
// JSON that isn't an object at all drops the call.
import Foundation

struct SettingsRow: Equatable {
    enum Kind: String { case single, header, child }

    struct MenuItem: Equatable {
        var action = ""
        var title = ""
    }

    var id = ""
    var kind = Kind.single
    var appId = ""
    var name = ""
    var detail = ""
    var iconPath = ""
    var letter = ""
    var letterColor = ""
    var hidden = false
    var isDefault = false
    var collapsed = false
    var draggable = false
    var menu: [MenuItem] = []

    init(_ o: [String: Any]) {
        id = o.string("id")
        kind = Kind(rawValue: o.string("kind")) ?? .single
        appId = o.string("app_id")
        name = o.string("name")
        detail = o.string("detail")
        iconPath = o.string("icon_path")
        letter = o.string("letter")
        letterColor = o.string("letter_color")
        hidden = o.bool("hidden")
        isDefault = o.bool("is_default")
        collapsed = o.bool("collapsed")
        draggable = o.bool("draggable")
        menu = o.objects("menu").map { MenuItem(action: $0.string("action"), title: $0.string("title")) }
    }
}

struct SettingsBanner: Equatable {
    var title = ""
    var message = ""
}

struct SettingsBrowsersState: Equatable {
    var filter = ""
    var showHidden = false
    var hiddenCount = 0
    var reorderEnabled = true
    var orderHint = ""
    var banner: SettingsBanner?
    var foundText = ""
    var emptyText = ""
    var rows: [SettingsRow] = []
    var specialRows: [SettingsRow] = []

    init() {}

    init(_ o: [String: Any]) {
        filter = o.string("filter")
        showHidden = o.bool("show_hidden")
        hiddenCount = o.int("hidden_count")
        reorderEnabled = o.bool("reorder_enabled", default: true)
        orderHint = o.string("order_hint")
        banner = o.object("banner").map { SettingsBanner(title: $0.string("title"), message: $0.string("message")) }
        foundText = o.string("found_text")
        emptyText = o.string("empty_text")
        rows = o.objects("rows").map(SettingsRow.init)
        specialRows = o.objects("special_rows").map(SettingsRow.init)
    }
}

// one entry of `source_options` / `target_options`
struct SettingsOption: Equatable, Hashable {
    var id = ""
    var label = ""
    var iconPath = ""

    init(id: String, label: String, iconPath: String = "") {
        self.id = id
        self.label = label
        self.iconPath = iconPath
    }

    init(_ o: [String: Any]) {
        self.init(id: o.string("id"), label: o.string("label"), iconPath: o.string("icon_path"))
    }
}

struct SettingsRule: Equatable {
    var pattern = ""
    var patternInvalid = false
    var patternEdit = ""
    var sourceId = ""
    var sourceLabel = ""
    var targetId = ""
    var targetLabel = ""
    var targetIconPath = ""
    var isPrivate = false
    var supportsPrivate = false
    var matched = false

    // what the pattern field shows while it isn't being edited
    var displayedPattern: String { patternInvalid ? patternEdit : pattern }

    init(_ o: [String: Any]) {
        pattern = o.string("pattern")
        patternInvalid = o.bool("pattern_invalid")
        patternEdit = o.string("pattern_edit")
        sourceId = o.string("source_id")
        sourceLabel = o.string("source_label")
        targetId = o.string("target_id")
        targetLabel = o.string("target_label")
        targetIconPath = o.string("target_icon_path")
        isPrivate = o.bool("private")
        supportsPrivate = o.bool("supports_private")
        matched = o.bool("matched")
    }
}

struct SettingsDraft: Equatable {
    var pattern = ""
    var patternPlaceholder = ""
    var patternInvalid = false
    var sourceId = ""
    var sourceLabel = ""
    // nil = not picked yet, "" = show the picker
    var targetId: String?
    var targetLabel = ""
    var help = ""

    init(_ o: [String: Any]) {
        pattern = o.string("pattern")
        patternPlaceholder = o.string("pattern_placeholder")
        patternInvalid = o.bool("pattern_invalid")
        sourceId = o.string("source_id")
        sourceLabel = o.string("source_label")
        targetId = o["target_id"] as? String
        targetLabel = o.string("target_label")
        help = o.string("help")
    }
}

struct SettingsTest: Equatable {
    enum ResultKind: String { case none, rule, fallback, invalid }

    var url = ""
    var sourceId = ""
    var sourceLabel = ""
    var resultKind = ResultKind.none
    var resultText = ""

    init() {}

    init(_ o: [String: Any]) {
        url = o.string("url")
        sourceId = o.string("source_id")
        sourceLabel = o.string("source_label")
        resultKind = ResultKind(rawValue: o.string("result_kind")) ?? .none
        resultText = o.string("result_text")
    }
}

struct SettingsFallback: Equatable {
    var targetId = ""
    var targetLabel = ""
    var targetIconPath = ""
    var isPrivate = false
    var supportsPrivate = false
    var matched = false

    init() {}

    init(_ o: [String: Any]) {
        targetId = o.string("target_id")
        targetLabel = o.string("target_label")
        targetIconPath = o.string("target_icon_path")
        isPrivate = o.bool("private")
        supportsPrivate = o.bool("supports_private")
        matched = o.bool("matched")
    }
}

struct SettingsRulesState: Equatable {
    var rules: [SettingsRule] = []
    var sourceOptions: [SettingsOption] = []
    var targetOptions: [SettingsOption] = []
    var draft: SettingsDraft?
    var test = SettingsTest()
    var fallback = SettingsFallback()

    init() {}

    init(_ o: [String: Any]) {
        rules = o.objects("rules").map(SettingsRule.init)
        sourceOptions = o.objects("source_options").map(SettingsOption.init)
        targetOptions = o.objects("target_options").map(SettingsOption.init)
        draft = o.object("draft").map(SettingsDraft.init)
        test = o.object("test").map(SettingsTest.init) ?? SettingsTest()
        fallback = o.object("fallback").map(SettingsFallback.init) ?? SettingsFallback()
    }
}

struct SettingsGeneralState: Equatable {
    var theme = Appearance.system
    var nativePopupAvailable = false
    var nativePopup = false
    var showHotkeys = true
    var quitOnLostFocusVisible = false
    var quitOnLostFocus = false
    var unwrapUrls = true
    var runInBackground = false

    init() {}

    init(_ o: [String: Any]) {
        theme = Appearance(rawValue: o.string("theme")) ?? .system
        nativePopupAvailable = o.bool("native_popup_available")
        nativePopup = o.bool("native_popup")
        showHotkeys = o.bool("show_hotkeys", default: true)
        quitOnLostFocusVisible = o.bool("quit_on_lost_focus_visible")
        quitOnLostFocus = o.bool("quit_on_lost_focus")
        unwrapUrls = o.bool("unwrap_urls", default: true)
        runInBackground = o.bool("run_in_background")
    }
}

struct AboutDirectory: Equatable {
    var label = ""
    var path = ""
}

struct AboutAttribution: Equatable {
    var label = "Made with Slint"
    var versionText = ""
    var url = "https://slint.dev"
}

// the About window's content, and the Settings' About page
struct AboutState: Equatable {
    var appearance = Appearance.system
    var appIconPath = ""
    var appName = "Browsers"
    var versionText = ""
    var copyright = ""
    var websiteUrl = ""
    var directories: [AboutDirectory] = []
    var madeWithSlint = AboutAttribution()

    static func decode(_ json: UnsafePointer<CChar>?) -> AboutState? {
        decodeSettingsObject(json, what: "about").map(AboutState.init)
    }

    init() {}

    init(_ o: [String: Any]) {
        appearance = Appearance(rawValue: o.string("appearance")) ?? .system
        appIconPath = o.string("app_icon_path")
        appName = o.string("app_name", default: "Browsers")
        versionText = o.string("version_text")
        copyright = o.string("copyright")
        websiteUrl = o.string("website_url")
        directories = o.objects("directories").map { AboutDirectory(label: $0.string("label"), path: $0.string("path")) }
        // the attribution is required by Slint's license, so it never disappears, even when the
        // object is missing
        let slint = o.object("made_with_slint") ?? [:]
        madeWithSlint = AboutAttribution(
            label: slint.string("label", default: "Made with Slint"),
            versionText: slint.string("version_text"),
            url: slint.string("url", default: "https://slint.dev")
        )
    }
}

struct SettingsState: Equatable {
    var appearance = Appearance.system
    var isDefaultBrowser = false
    var browsers = SettingsBrowsersState()
    var rules = SettingsRulesState()
    var general = SettingsGeneralState()
    var about = AboutState()

    static func decode(_ json: UnsafePointer<CChar>?) -> SettingsState? {
        decodeSettingsObject(json, what: "settings").map(SettingsState.init)
    }

    init() {}

    init(_ o: [String: Any]) {
        appearance = Appearance(rawValue: o.string("appearance")) ?? .system
        isDefaultBrowser = o.bool("is_default_browser")
        browsers = o.object("browsers").map(SettingsBrowsersState.init) ?? SettingsBrowsersState()
        rules = o.object("rules").map(SettingsRulesState.init) ?? SettingsRulesState()
        general = o.object("general").map(SettingsGeneralState.init) ?? SettingsGeneralState()
        about = o.object("about").map(AboutState.init) ?? AboutState()
        about.appearance = appearance
    }
}

private func decodeSettingsObject(_ json: UnsafePointer<CChar>?, what: String) -> [String: Any]? {
    guard let json else { return nil }
    let data = Data(String(cString: json).utf8)
    guard let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else {
        FileHandle.standardError.write(Data("browsers settings: invalid \(what) state JSON\n".utf8))
        return nil
    }
    return object
}

private extension Dictionary where Key == String, Value == Any {
    func string(_ key: String, default fallback: String = "") -> String { self[key] as? String ?? fallback }
    func bool(_ key: String, default fallback: Bool = false) -> Bool { self[key] as? Bool ?? fallback }
    func int(_ key: String) -> Int { (self[key] as? NSNumber)?.intValue ?? 0 }
    func object(_ key: String) -> [String: Any]? { self[key] as? [String: Any] }
    func objects(_ key: String) -> [[String: Any]] { self[key] as? [[String: Any]] ?? [] }
}
