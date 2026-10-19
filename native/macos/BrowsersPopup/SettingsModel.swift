// What the Settings window shows: the Rust state plus what only lives on this side (the page,
// the text being typed). Every user action goes out as an event, Rust answers
// with a fresh state (SETTINGS_CONTRACT.md).
import AppKit
import SwiftUI

enum SettingsPage: String, CaseIterable, Identifiable {
    case browsers, rules, general, about

    var id: String { rawValue }

    var title: String {
        switch self {
        case .browsers: return "Browsers"
        case .rules: return "Rules"
        case .general: return "General"
        case .about: return "About"
        }
    }

    var symbol: String {
        switch self {
        case .browsers: return "globe"
        case .rules: return "arrow.triangle.branch"
        case .general: return "gearshape"
        case .about: return "info.circle"
        }
    }
}

// which list a Browsers row lives in, as the events name it
enum SettingsRowList: String {
    case rows, specialRows = "special_rows"
}

@MainActor
final class SettingsModel: ObservableObject {
    @Published private(set) var state = SettingsState()
    @Published var page = SettingsPage.browsers
    // text fields this side owns (see "Text fields" in the contract)
    @Published private(set) var filter = ""
    @Published private(set) var testURL = ""
    @Published private(set) var draftPattern = ""
    // bumped for every fresh open: the view tree is rebuilt, so scroll positions and per-view
    // state start over
    @Published private(set) var session = 0
    // bumped when a draft appears, the draft's field takes the focus
    @Published private(set) var draftFocusRequest = 0

    var emit: (String, KeyValuePairs<String, Any>) -> Void = { _, _ in }

    // MARK: content

    func reset(with newState: SettingsState, page newPage: SettingsPage?) {
        filter = ""
        testURL = ""
        draftPattern = newState.rules.draft?.pattern ?? ""
        page = newPage ?? .browsers
        state = newState
        session += 1
        if newState.rules.draft != nil { draftFocusRequest += 1 }
    }

    func replace(with newState: SettingsState) {
        let hadDraft = state.rules.draft != nil
        if let draft = newState.rules.draft {
            if !hadDraft {
                draftPattern = draft.pattern
                draftFocusRequest += 1
            }
        } else {
            draftPattern = ""
        }
        state = newState
    }

    func select(_ newPage: SettingsPage) {
        page = newPage
    }

    // MARK: window

    func setDefaultBrowser() { emit("set_default_browser", [:]) }

    // MARK: Browsers page

    func setFilter(_ text: String) {
        guard text != filter else { return }
        filter = text
        emit("browsers_filter_changed", ["text": text])
    }

    func setShowHidden(_ show: Bool) { emit("set_show_hidden", ["show": show]) }
    func rescan() { emit("rescan", [:]) }
    func hideNonBrowsers() { emit("hide_non_browsers", [:]) }
    func dismissBanner() { emit("dismiss_non_browser_banner", [:]) }

    func toggleVisibility(_ list: SettingsRowList, _ index: Int) {
        guard let row = row(list, index) else { return }
        emit("toggle_row_visibility", ["list": list.rawValue, "row": index, "row_id": row.id])
    }

    func setCollapsed(_ row: SettingsRow, _ collapsed: Bool) {
        emit("set_group_collapsed", ["app_id": row.appId, "collapsed": collapsed])
    }

    func rowMenu(_ list: SettingsRowList, _ index: Int, _ action: String) {
        guard let row = row(list, index) else { return }
        emit("row_menu", ["list": list.rawValue, "row": index, "row_id": row.id, "action": action])
    }

    func moveBrowserRow(_ index: Int, to destination: Int) {
        guard let row = row(.rows, index) else { return }
        emit("move_browser_row", ["row": index, "row_id": row.id, "to": destination])
    }

    private func row(_ list: SettingsRowList, _ index: Int) -> SettingsRow? {
        let rows = list == .rows ? state.browsers.rows : state.browsers.specialRows
        return rows.indices.contains(index) ? rows[index] : nil
    }

    // MARK: Rules page

    func addRule() { emit("add_rule", [:]) }

    func editRulePattern(_ index: Int, _ pattern: String) {
        emit("rule_pattern_edited", ["index": index, "pattern": pattern])
    }

    func pickRuleSource(_ index: Int, _ id: String) { emit("rule_source_picked", ["index": index, "source_id": id]) }
    func pickRuleTarget(_ index: Int, _ id: String) { emit("rule_target_picked", ["index": index, "target_id": id]) }
    func setRulePrivate(_ index: Int, _ value: Bool) { emit("rule_private_set", ["index": index, "private": value]) }
    func duplicateRule(_ index: Int) { emit("rule_duplicate", ["index": index]) }
    func deleteRule(_ index: Int) { emit("rule_delete", ["index": index]) }

    func moveRule(_ from: Int, to destination: Int) {
        // onMove semantics: dropping right before or after itself changes nothing
        guard destination != from, destination != from + 1 else { return }
        emit("move_rule", ["from": from, "to": destination])
    }

    func setDraftPattern(_ text: String) {
        guard text != draftPattern else { return }
        draftPattern = text
        emit("draft_pattern_edited", ["text": text])
    }

    func commitDraftPattern() { emit("draft_pattern_committed", [:]) }
    func pickDraftSource(_ id: String) { emit("draft_source_picked", ["source_id": id]) }
    func pickDraftTarget(_ id: String) { emit("draft_target_picked", ["target_id": id]) }
    func discardDraft() { emit("draft_discarded", [:]) }

    func setTestURL(_ text: String) {
        guard text != testURL else { return }
        testURL = text
        emit("test_url_edited", ["text": text])
    }

    func pickTestSource(_ id: String) { emit("test_source_picked", ["source_id": id]) }
    func pickFallbackTarget(_ id: String) { emit("fallback_target_picked", ["target_id": id]) }
    func setFallbackPrivate(_ value: Bool) { emit("fallback_private_set", ["private": value]) }

    // MARK: General page

    func setTheme(_ theme: Appearance) { emit("set_theme", ["theme": theme.rawValue]) }
    func setNativePopup(_ value: Bool) { emit("set_native_popup", ["value": value]) }
    func setShowHotkeys(_ value: Bool) { emit("set_show_hotkeys", ["value": value]) }
    func setQuitOnLostFocus(_ value: Bool) { emit("set_quit_on_lost_focus", ["value": value]) }
    func setUnwrapUrls(_ value: Bool) { emit("set_unwrap_urls", ["value": value]) }
    func setRunInBackground(_ value: Bool) { emit("set_run_in_background", ["value": value]) }
}
