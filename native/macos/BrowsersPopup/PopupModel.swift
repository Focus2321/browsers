// What the popup shows right now: the Rust state plus the per-appearance UI state (filter,
// selection, private mode) that only lives on this side.
import AppKit
import SwiftUI

// asks the list to bring a row into view; `token` makes repeated requests for the same row count
struct ScrollRequest: Equatable {
    var position: Int
    var anchor: UnitPoint?
    var token: Int
}

@MainActor
final class PopupModel: ObservableObject {
    @Published private(set) var state = PopupState()
    // `state.browsers` narrowed down by the filter, in display order. Hotkeys and the selection
    // index into this list.
    @Published private(set) var rows: [BrowserEntry] = []
    @Published private(set) var filter = ""
    @Published private(set) var selection = 0
    @Published var privateMode = false
    @Published private(set) var linkCopied = false
    @Published private(set) var scrollRequest = ScrollRequest(position: 0, anchor: .top, token: 0)

    var emit: ([String: Any]) -> Void = { _ in }
    // the panel resizes to fit the rows when their count changes
    var onRowCountChange: () -> Void = {}
    // the "···" button asks for the options menu, anchored at this rect (hosting view coordinates)
    var onOptionsMenu: (CGRect) -> Void = { _ in }

    private var linkCopiedReset: DispatchWorkItem?
    // the mouse position the last hover was handled at, see hover(_:)
    private var lastHoverLocation = NSPoint(x: CGFloat.nan, y: .nan)

    // MARK: content

    func reset(with newState: PopupState) {
        state = newState
        filter = ""
        selection = 0
        privateMode = false
        setLinkCopied(false)
        // a still cursor that happens to end up over a row doesn't take the selection
        lastHoverLocation = NSEvent.mouseLocation
        refilter()
        scroll(to: 0, anchor: .top)
    }

    func replace(with newState: PopupState) {
        state = newState
        refilter()
    }

    private func refilter() {
        let oldCount = rows.count
        rows = state.browsers.filter { $0.matches(filter) }
        selection = min(selection, max(rows.count - 1, 0))
        if rows.count != oldCount { onRowCountChange() }
    }

    // MARK: filter

    func setFilter(_ text: String) {
        guard text != filter else { return }
        filter = text
        // the best match is the first row, same as when the popup opens
        selection = 0
        refilter()
        scroll(to: 0, anchor: .top)
    }

    // MARK: selection

    func moveSelection(by delta: Int) {
        guard !rows.isEmpty else { return }
        selection = min(max(selection + delta, 0), rows.count - 1)
        scroll(to: selection, anchor: nil)
    }

    // only an actual mouse move takes the selection - a list scrolling under a still cursor (after
    // arrowing down) would otherwise yank it back to whatever row lands under the pointer
    func hover(_ position: Int) {
        let location = NSEvent.mouseLocation
        guard location != lastHoverLocation, rows.indices.contains(position) else { return }
        lastHoverLocation = location
        selection = position
    }

    private func scroll(to position: Int, anchor: UnitPoint?) {
        scrollRequest = ScrollRequest(position: position, anchor: anchor, token: scrollRequest.token + 1)
    }

    // MARK: actions

    func openSelection() { open(selection, privately: privateMode) }

    func click(_ position: Int) {
        selection = position
        // the click itself may be the first thing to see ⇧ (pressed while another window was key)
        open(position, privately: privateMode || NSEvent.modifierFlags.contains(.shift))
    }

    func open(_ position: Int, privately: Bool) {
        guard rows.indices.contains(position) else { return }
        let row = rows[position]
        // rows without a private mode are faded out while ⇧ is held - opening one of those in a
        // regular window instead would quietly break the "private" the user asked for
        if privately && !row.supportsIncognito { return }
        emit(["type": "open", "profile_index": row.profileIndex, "private": privately])
    }

    func copyLink() {
        emit(["type": "copy_link"])
        setLinkCopied(true)
    }

    func alwaysOpenHere(_ row: BrowserEntry) {
        emit(["type": "always_open_here", "unique_id": row.uniqueId, "profile_index": row.profileIndex])
    }

    func send(_ type: String, _ extra: [String: Any] = [:]) {
        emit(extra.merging(["type": type]) { $1 })
    }

    func showOptionsMenu(anchor: CGRect) { onOptionsMenu(anchor) }

    private func setLinkCopied(_ copied: Bool) {
        linkCopiedReset?.cancel()
        linkCopied = copied
        guard copied else { return }
        let reset = DispatchWorkItem { [weak self] in self?.linkCopied = false }
        linkCopiedReset = reset
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.5, execute: reset)
    }
}
