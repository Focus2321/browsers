// The state JSON the Rust side sends (CONTRACT.md "State").
// Decoded leniently: a missing or mistyped field falls back to its default instead of dropping
// the whole update, so the two sides can evolve a field at a time.
import Foundation

struct BrowserEntry: Equatable {
    var profileIndex = 0
    var uniqueId = ""
    var uniqueAppId = ""
    var browserName = ""
    var profileName = ""
    var fullName = ""
    var supportsProfiles = false
    var supportsIncognito = false
    var iconPath = ""
    var profileIconPath = ""
    var profileInitial = ""
    var avatarColor = ""

    // profile first - it's what you're actually choosing - the app it belongs to second
    var title: String { supportsProfiles ? profileName : browserName }

    func matches(_ query: String) -> Bool {
        let query = query.trimmingCharacters(in: .whitespaces).lowercased()
        if query.isEmpty { return true }
        let names = supportsProfiles ? [browserName, profileName] : [browserName]
        return names.contains { $0.lowercased().contains(query) }
    }
}

struct RestorableEntry: Equatable {
    var uniqueId = ""
    var fullName = ""
}

enum Appearance: String {
    case system, light, dark
}

struct PopupState: Equatable {
    var url = ""
    var urlDomain = ""
    var urlPath = ""
    var sourceAppName = ""
    var showHotkeys = true
    var showSetAsDefault = false
    var appearance = Appearance.system
    var browsers: [BrowserEntry] = []
    var restorable: [RestorableEntry] = []

    static func decode(_ json: UnsafePointer<CChar>?) -> PopupState? {
        guard let json else { return nil }
        let data = Data(String(cString: json).utf8)
        guard let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else {
            FileHandle.standardError.write(Data("browsers popup: invalid state JSON\n".utf8))
            return nil
        }
        return PopupState(object)
    }

    init() {}

    init(_ o: [String: Any]) {
        url = o.string("url")
        urlDomain = o.string("url_domain")
        urlPath = o.string("url_path")
        sourceAppName = o.string("source_app_name")
        showHotkeys = o.bool("show_hotkeys", default: true)
        showSetAsDefault = o.bool("show_set_as_default")
        appearance = Appearance(rawValue: o.string("appearance")) ?? .system
        browsers = o.objects("browsers").map { b in
            BrowserEntry(
                profileIndex: b.int("profile_index"),
                uniqueId: b.string("unique_id"),
                uniqueAppId: b.string("unique_app_id"),
                browserName: b.string("browser_name"),
                profileName: b.string("profile_name"),
                fullName: b.string("full_name"),
                supportsProfiles: b.bool("supports_profiles"),
                supportsIncognito: b.bool("supports_incognito"),
                iconPath: b.string("icon_path"),
                profileIconPath: b.string("profile_icon_path"),
                profileInitial: b.string("profile_initial"),
                avatarColor: b.string("avatar_color")
            )
        }
        restorable = o.objects("restorable").map {
            RestorableEntry(uniqueId: $0.string("unique_id"), fullName: $0.string("full_name"))
        }
    }
}

private extension Dictionary where Key == String, Value == Any {
    func string(_ key: String) -> String { self[key] as? String ?? "" }
    func bool(_ key: String, default fallback: Bool = false) -> Bool { self[key] as? Bool ?? fallback }
    func int(_ key: String) -> Int { (self[key] as? NSNumber)?.intValue ?? 0 }
    func objects(_ key: String) -> [[String: Any]] { self[key] as? [[String: Any]] ?? [] }
}
