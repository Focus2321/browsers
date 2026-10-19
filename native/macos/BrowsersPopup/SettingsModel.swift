// What the Settings window shows: the Rust state plus what only lives on this side (the page,
// the text being typed, drags in progress). Every user action goes out as an event, Rust answers
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

// a row being dragged, and where it would land (an insertion index into the original list, like
// onMove's destination)
struct SettingsDrag: Equatable {
    enum Kind { case browserRow, rule }

    var kind: Kind
    var from: Int
    var id: String
    var insertion: Int?
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
    @Published var drag: SettingsDrag?
    private var hoverSerial = 0
    private var dragWatch: Timer?

    var emit: (String, KeyValuePairs<String, Any>) -> Void = { _, _ in }

    // MARK: content

    func reset(with newState: SettingsState, page newPage: SettingsPage?) {
        filter = ""
        testURL = ""
        drag = nil
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
        // a drag over a list that changed shape would drop at a stale index
        if drag != nil, state.browsers.rows.map(\.id) != newState.browsers.rows.map(\.id)
            || state.rules.rules.count != newState.rules.rules.count {
            drag = nil
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

    // MARK: drag and drop

    func beginDrag(_ kind: SettingsDrag.Kind, from index: Int, id: String) {
        drag = SettingsDrag(kind: kind, from: index, id: id, insertion: nil)
        watchForDragEnd()
    }

    // the mouse is over row `index`, in its upper or lower half
    func dragHovers(_ kind: SettingsDrag.Kind, row index: Int, lowerHalf: Bool) {
        guard var current = drag, current.kind == kind else { return }
        hoverSerial += 1
        let raw = lowerHalf ? index + 1 : index
        current.insertion = kind == .browserRow ? snappedInsertion(from: current.from, raw) : raw
        if current != drag { drag = current }
    }

    // Leaving a row usually means entering the next one, so the line only goes away when no
    // other row picks the drag up right after (the gaps between rows don't count as leaving).
    func dragLeaves(_ kind: SettingsDrag.Kind) {
        guard drag?.kind == kind else { return }
        let serial = hoverSerial
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak self] in
            guard let self, self.hoverSerial == serial, self.drag?.insertion != nil else { return }
            self.drag?.insertion = nil
        }
    }

    // SwiftUI doesn't say when a drag ends without a drop (released outside the list, Esc), so
    // the mouse button is watched instead, to take the insertion line away
    private func watchForDragEnd() {
        dragWatch?.invalidate()
        let timer = Timer(timeInterval: 0.1, repeats: true) { [weak self] timer in
            MainActor.assumeIsolated {
                guard let self, self.drag != nil else {
                    timer.invalidate()
                    return
                }
                guard NSEvent.pressedMouseButtons & 1 == 0 else { return }
                timer.invalidate()
                // a drop is still being delivered right after the release
                let ended = self.drag
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) { [weak self] in
                    if let self, self.drag == ended { self.drag = nil }
                }
            }
        }
        RunLoop.main.add(timer, forMode: .common)
        dragWatch = timer
    }

    func drop(_ kind: SettingsDrag.Kind) -> Bool {
        guard let current = drag, current.kind == kind, let insertion = current.insertion else {
            drag = nil
            return false
        }
        drag = nil
        switch kind {
        case .browserRow:
            guard state.browsers.rows.indices.contains(current.from),
                  state.browsers.rows[current.from].id == current.id,
                  !isNoOpBrowserMove(from: current.from, to: insertion)
            else { return false }
            moveBrowserRow(current.from, to: insertion)
        case .rule:
            moveRule(current.from, to: insertion)
        }
        return true
    }

    func endDrag() { drag = nil }

    // Where Rust will put the row (snap_drop_index), mirrored for the insertion line only, Rust
    // snaps the drop itself: a profile stays inside its group, anything else lands between apps
    // and groups, never between another app's profiles.
    private func snappedInsertion(from: Int, _ raw: Int) -> Int {
        let rows = state.browsers.rows
        guard rows.indices.contains(from) else { return raw }
        let kinds = rows.map(\.kind)
        if kinds[from] == .child {
            let header = (0..<from).last { kinds[$0] == .header } ?? 0
            return min(max(raw, header + 1), groupEnd(after: header, kinds))
        }
        // inserting before a profile is inside a group: go to the nearer edge of it
        guard raw < kinds.count, kinds[raw] == .child else { return raw }
        let header = (0..<raw).last { kinds[$0] == .header } ?? 0
        let end = groupEnd(after: header, kinds)
        return raw - header <= end - raw ? header : end
    }

    // the index right after a group's last listed profile
    private func groupEnd(after header: Int, _ kinds: [SettingsRow.Kind]) -> Int {
        var end = header + 1
        while end < kinds.count, kinds[end] == .child { end += 1 }
        return end
    }

    private func isNoOpBrowserMove(from: Int, to insertion: Int) -> Bool {
        let kinds = state.browsers.rows.map(\.kind)
        let end = kinds[from] == .header ? groupEnd(after: from, kinds) : from + 1
        return insertion >= from && insertion <= end
    }
}
