// Drag to reorder for the Settings lists. The row (or a whole group, with its profiles) lifts and
// follows the pointer, the others slide out of its way, the list scrolls when the pointer nears
// its edges, and on release the row settles into its new place before the move goes to Rust.
// It's all one mouse drag inside the list, no pasteboard drag, so there's no drop to miss.
import AppKit
import SwiftUI

enum SettingsReorderLevel: Equatable {
    // an app, a group's header or a rule: moves among the other top rows, with its children
    case top
    // a group's profile: moves only among its siblings
    case child
}

// Starts a drag on the view it's applied to, a row or just its grip. Inert for rows that can't
// move.
struct SettingsReorderGrip: ViewModifier {
    fileprivate let enabled: Bool
    // how far the pointer has gone since the press, down positive
    fileprivate let onChanged: (_ translation: CGFloat) -> Void
    fileprivate let onEnded: () -> Void

    @GestureState private var active = false
    @State private var ended = false

    func body(content: Content) -> some View {
        if enabled {
            content.gesture(
                DragGesture(minimumDistance: 3, coordinateSpace: .global)
                    .updating($active) { _, active, _ in active = true }
                    .onChanged { value in onChanged(value.translation.height) }
                    .onEnded { _ in
                        ended = true
                        onEnded()
                    }
            )
            // a gesture taken away without an end (the window closing mid-drag) still ends it,
            // once
            .onChange(of: active) { isActive in
                if !isActive && !ended { onEnded() }
                ended = false
            }
        } else {
            content
        }
    }
}

struct SettingsReorderStack<RowContent: View>: View {
    // one per row, identifies it
    var keys: [String] = []
    // what the rows hold, in order, for rows without keys of their own (rules go by position):
    // they're told apart by it instead, and a drag only lasts until it changes
    var contents: [String]? = nil
    let levels: [SettingsReorderLevel]
    let draggable: [Bool]
    var rowPadding = EdgeInsets(top: 10, leading: 10, bottom: 10, trailing: 10)
    // where a row's separator starts, from the row's leading edge
    var separatorInset: CGFloat = 10
    // the whole row starts a drag, rather than just the part the row applies its grip to
    var wholeRowGrip = false
    // a row dragged from `from` to insertion index `to` of the original rows (onMove semantics)
    let onMove: (_ from: Int, _ to: Int) -> Void
    @ViewBuilder let row: (_ index: Int, _ grip: SettingsReorderGrip) -> RowContent

    private enum Scope: Equatable {
        case top
        case children(ofBlock: Int)
    }

    private struct Drag {
        // tells this drag from the next one, for what's still pending when it's over
        var id = UUID()
        // released, and settling or sliding back
        var ended = false
        var scope: Scope
        // the unit being dragged, an index into the scope's units
        var unit: Int
        var contents: [String]
        var startMouseY: CGFloat
        var startScrollY: CGFloat
    }

    @State private var heights: [String: CGFloat] = [:]
    @State private var drag: Drag?
    @State private var target = 0
    @State private var translation: CGFloat = 0
    @State private var settling = false
    // a press that came while a dropped row settled, taken up once the move is in
    @State private var waiting: WaitingPress?
    @State private var autoscroll: Timer?
    @State private var probe = SettingsViewProbe()
    @State private var identities = SettingsReorderIdentities()
    @State private var latest = Latest()

    // this view as last laid out: what's run later (onChange, a timer) gets the rows as they are
    // then, not as they were when it was set up
    private final class Latest {
        var stack: SettingsReorderStack?
    }

    var body: some View {
        let _ = latest.stack = self
        VStack(spacing: 0) {
            ForEach(Array(blocks.enumerated()), id: \.element.key) { blockIndex, block in
                VStack(spacing: 0) {
                    // by key, so a row keeps its view (and a press in progress) when it moves
                    ForEach(block.rows.map { Slot(index: $0, key: ids[$0]) }, id: \.key) { slot in
                        rowView(slot.index, block: blockIndex)
                    }
                }
                .background(lift(blockLifted(blockIndex)))
                .offset(y: blockOffset(blockIndex))
                .zIndex(blockLifted(blockIndex) ? 1 : 0)
            }
        }
        .onPreferenceChange(SettingsRowHeights.self) { heights = $0 }
        .background(SettingsViewProbeView(probe: probe))
        // the rows changed under the drag: Rust's answer to the drop, or something else
        .onChange(of: order) { _ in
            guard drag != nil else { return }
            stopDrag()
            latest.stack?.takeUpWaiting()
        }
        .onDisappear {
            waiting = nil
            stopDrag()
        }
    }

    private func rowView(_ index: Int, block blockIndex: Int) -> some View {
        let lifted = childLifted(index, block: blockIndex)
        let grip = grip(index, block: blockIndex)
        let inert = SettingsReorderGrip(enabled: false, onChanged: { _ in }, onEnded: {})
        return row(index, wholeRowGrip ? inert : grip)
            .padding(rowPadding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .modifier(wholeRowGrip ? grip : inert)
            .background(
                GeometryReader { geometry in
                    Color.clear.preference(
                        key: SettingsRowHeights.self, value: [ids[index]: geometry.size.height]
                    )
                }
            )
            .overlay(alignment: .top) {
                if index > 0 {
                    Rectangle()
                        .fill(Color(nsColor: .separatorColor))
                        .frame(height: 1)
                        .padding(.leading, separatorInset)
                        .padding(.trailing, rowPadding.trailing)
                        .opacity(separatorHidden(index) ? 0 : 1)
                }
            }
            .background(lift(lifted))
            .offset(y: childOffset(index, block: blockIndex))
            .zIndex(lifted ? 1 : 0)
    }

    // the lifted look: a raised card under the dragged rows
    @ViewBuilder private func lift(_ lifted: Bool) -> some View {
        if lifted {
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(Color(nsColor: SettingsReorderColors.lifted))
                .overlay(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .strokeBorder(Color(nsColor: .separatorColor), lineWidth: 0.5)
                )
                .shadow(color: .black.opacity(0.22), radius: 9, x: 0, y: 4)
                .padding(.horizontal, 4)
                .allowsHitTesting(false)
        }
    }

    // MARK: structure

    private struct Block {
        var key: String
        // flat row indices: the top row, then its children
        var rows: [Int]
    }

    private struct WaitingPress {
        var id: String
        var startMouseY: CGFloat
        var released = false
    }

    private struct Slot {
        var index: Int
        var key: String
    }

    // what the rows' views go by: their keys, or identities that follow what they hold
    private var ids: [String] {
        contents.map { identities.ids(for: $0) } ?? keys
    }

    private var blocks: [Block] {
        let ids = ids
        var result: [Block] = []
        for index in ids.indices {
            if levels[index] == .child, !result.isEmpty {
                result[result.count - 1].rows.append(index)
            } else {
                result.append(Block(key: ids[index], rows: [index]))
            }
        }
        return result
    }

    private func blockHeight(_ block: Block, ids: [String]) -> CGFloat {
        block.rows.reduce(0) { $0 + (heights[ids[$1]] ?? 0) }
    }

    // the scope's units as flat row ranges, with their heights
    private func units(_ scope: Scope) -> [(rows: [Int], height: CGFloat)] {
        let blocks = blocks, ids = ids
        switch scope {
        case .top:
            return blocks.map { ($0.rows, blockHeight($0, ids: ids)) }
        case .children(let blockIndex):
            guard blocks.indices.contains(blockIndex) else { return [] }
            return blocks[blockIndex].rows.dropFirst().map { ([$0], heights[ids[$0]] ?? 0) }
        }
    }

    // MARK: offsets

    private var order: [String] { contents ?? keys }

    // a drag only applies to the rows it started on: once Rust's answer reorders them, they're
    // laid out in their new places and the offsets go away in the same update
    private var liveDrag: Drag? {
        guard let drag, drag.contents == order else { return nil }
        return drag
    }

    private func blockLifted(_ blockIndex: Int) -> Bool {
        guard let drag = liveDrag, drag.scope == .top else { return false }
        return drag.unit == blockIndex
    }

    private func childLifted(_ index: Int, block blockIndex: Int) -> Bool {
        guard let drag = liveDrag, drag.scope == .children(ofBlock: blockIndex) else { return false }
        return units(drag.scope)[safe: drag.unit]?.rows.first == index
    }

    // no line on the lifted rows' top edge, on the row they sit right above, or at the very top
    // of the list, wherever the rows have slid to
    private func separatorHidden(_ index: Int) -> Bool {
        guard let drag = liveDrag else { return false }
        let units = units(drag.scope)
        guard units.indices.contains(drag.unit) else { return false }
        var visual = Array(units.indices)
        visual.remove(at: drag.unit)
        visual.insert(drag.unit, at: min(target, visual.count))
        var hidden = [drag.unit]
        if let after = visual.firstIndex(of: drag.unit).flatMap({ visual[safe: $0 + 1] }) {
            hidden.append(after)
        }
        if drag.scope == .top, let first = visual.first { hidden.append(first) }
        return hidden.contains { units[$0].rows.first == index }
    }

    private func blockOffset(_ blockIndex: Int) -> CGFloat {
        guard let drag = liveDrag, drag.scope == .top else { return 0 }
        return offset(ofUnit: blockIndex, drag: drag)
    }

    private func childOffset(_ index: Int, block blockIndex: Int) -> CGFloat {
        guard let drag = liveDrag, drag.scope == .children(ofBlock: blockIndex),
              let unit = units(drag.scope).firstIndex(where: { $0.rows.first == index })
        else { return 0 }
        return offset(ofUnit: unit, drag: drag)
    }

    private func offset(ofUnit unit: Int, drag: Drag) -> CGFloat {
        if unit == drag.unit { return translation }
        let moving = units(drag.scope)[safe: drag.unit]?.height ?? 0
        if drag.unit < unit && unit <= target { return -moving }
        if target <= unit && unit < drag.unit { return moving }
        return 0
    }

    // MARK: dragging

    private func grip(_ index: Int, block blockIndex: Int) -> SettingsReorderGrip {
        SettingsReorderGrip(
            enabled: draggable[index],
            onChanged: { dragChanged(index, block: blockIndex, translation: $0) },
            onEnded: { dragEnded() }
        )
    }

    private func dragChanged(_ index: Int, block blockIndex: Int, translation pressed: CGFloat) {
        guard let window = probe.view?.window else { return }
        // where the press was, so a drag taken up late still follows from there
        let startMouseY = window.mouseLocationOutsideOfEventStream.y + pressed
        if drag?.ended == true {
            // a dropped row is settling: this press waits the moment until the move is in
            guard !settling else {
                if waiting == nil { waiting = WaitingPress(id: ids[index], startMouseY: startMouseY) }
                return
            }
            // one sliding back gives way to it
            stopDrag()
        }
        if drag == nil { begin(index, block: blockIndex, startMouseY: startMouseY) }
        follow()
        NSCursor.closedHand.set()
    }

    private func begin(_ index: Int, block blockIndex: Int, startMouseY: CGFloat) {
        let scope: Scope = levels[index] == .child ? .children(ofBlock: blockIndex) : .top
        guard let unit = units(scope).firstIndex(where: { $0.rows.first == index }) else { return }
        drag = Drag(scope: scope, unit: unit, contents: order, startMouseY: startMouseY, startScrollY: scrollY)
        target = unit
        translation = 0
        startAutoscroll()
    }

    // the press that waited for the last drop, on its row wherever that went, and its release
    // when it's already been let go
    private func takeUpWaiting() {
        guard let press = waiting else { return }
        waiting = nil
        let blocks = blocks
        guard let index = ids.firstIndex(of: press.id), draggable[safe: index] == true,
              let blockIndex = blocks.firstIndex(where: { $0.rows.contains(index) })
        else { return }
        begin(index, block: blockIndex, startMouseY: press.startMouseY)
        follow()
        if press.released {
            dragEnded()
        } else {
            NSCursor.closedHand.set()
        }
    }

    // puts the lifted rows under the pointer and works out where they'd land
    private func follow() {
        guard let drag = liveDrag, !drag.ended, let window = probe.view?.window else { return }
        let units = units(drag.scope)
        guard units.indices.contains(drag.unit) else { return }
        let tops = units.reduce(into: [CGFloat]()) { tops, unit in
            tops.append((tops.last ?? 0) + (tops.isEmpty ? 0 : units[tops.count - 1].height))
        }
        let total = (tops.last ?? 0) + (units.last?.height ?? 0)
        let moving = units[drag.unit].height

        // window coordinates go up, the list goes down
        var moved = drag.startMouseY - window.mouseLocationOutsideOfEventStream.y
        moved += scrollY - drag.startScrollY
        let top = tops[drag.unit]
        translation = min(max(moved, -top), total - top - moving)

        // a unit makes way once the lifted rows' leading edge passes its middle, so a tall group
        // swaps with a short row as soon as it covers half of it
        let liftedTop = top + translation
        let liftedBottom = liftedTop + moving
        var landing = drag.unit
        for unit in units.indices where unit > drag.unit && liftedBottom > tops[unit] + units[unit].height / 2 {
            landing = unit
        }
        for unit in units.indices.reversed() where unit < drag.unit && liftedTop < tops[unit] + units[unit].height / 2 {
            landing = unit
        }
        if landing != target {
            withAnimation(.spring(response: 0.28, dampingFraction: 0.86)) { target = landing }
        }
    }

    private func dragEnded() {
        if waiting != nil {
            waiting?.released = true
            return
        }
        guard var drag, !drag.ended else { return }
        drag.ended = true
        self.drag = drag
        stopAutoscroll()
        NSCursor.arrow.set()
        let units = units(drag.scope)
        guard drag.contents == order, units.indices.contains(drag.unit), target != drag.unit else {
            withAnimation(.spring(response: 0.3, dampingFraction: 0.86)) { translation = 0 }
            settle(after: 0.3) { if self.drag?.id == drag.id { stopDrag() } }
            return
        }

        // where the lifted rows end up: past the units they've jumped over
        let landing = target
        let slot: CGFloat
        if landing > drag.unit {
            slot = units[(drag.unit + 1)...landing].reduce(0) { $0 + $1.height }
        } else {
            slot = -units[landing..<drag.unit].reduce(0) { $0 + $1.height }
        }
        let from = units[drag.unit].rows[0]
        let to = landing > drag.unit ? units[landing].rows.last! + 1 : units[landing].rows[0]

        settling = true
        withAnimation(.spring(response: 0.26, dampingFraction: 0.9)) { translation = slot }
        // the move goes out once the row has settled, Rust's answer then lays the rows out in their
        // new order with nothing left to animate
        settle(after: 0.22) {
            // the rows changed under it meanwhile, the indices are stale
            guard self.drag?.id == drag.id else { return }
            onMove(from, to)
            // no answer (Rust didn't take the move): slide back
            settle(after: 1.0) {
                guard self.drag?.id == drag.id else { return }
                withAnimation(.spring(response: 0.3, dampingFraction: 0.86)) {
                    self.target = drag.unit
                    self.translation = 0
                }
                settle(after: 0.3) {
                    guard self.drag?.id == drag.id else { return }
                    stopDrag()
                    latest.stack?.takeUpWaiting()
                }
            }
        }
    }

    private func settle(after seconds: Double, _ action: @escaping () -> Void) {
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds, execute: action)
    }

    private func stopDrag() {
        stopAutoscroll()
        var transaction = Transaction()
        transaction.disablesAnimations = true
        withTransaction(transaction) {
            drag = nil
            settling = false
            translation = 0
        }
    }

    // MARK: scrolling along

    private var scrollView: NSScrollView? { probe.view?.enclosingScrollView }

    // how far the list has scrolled, growing as it scrolls down
    private var scrollY: CGFloat {
        guard let scrollView else { return 0 }
        let y = scrollView.contentView.bounds.origin.y
        return scrollView.documentView?.isFlipped == false ? -y : y
    }

    private func startAutoscroll() {
        stopAutoscroll()
        let timer = Timer(timeInterval: 1.0 / 60, repeats: true) { _ in
            MainActor.assumeIsolated { autoscrollTick() }
        }
        RunLoop.main.add(timer, forMode: .common)
        autoscroll = timer
    }

    private func stopAutoscroll() {
        autoscroll?.invalidate()
        autoscroll = nil
    }

    // near the visible top or bottom edge the list scrolls, faster the closer the pointer gets
    private func autoscrollTick() {
        guard liveDrag != nil, !settling, let scrollView, let window = scrollView.window,
              let document = scrollView.documentView
        else { return }
        let frame = scrollView.convert(scrollView.bounds, to: nil)
        let visibleTop = frame.maxY - scrollView.contentInsets.top
        let visibleBottom = frame.minY + scrollView.contentInsets.bottom
        let mouse = window.mouseLocationOutsideOfEventStream.y
        let zone: CGFloat = 36
        var step: CGFloat = 0
        if mouse > visibleTop - zone {
            step = -min(1, (mouse - (visibleTop - zone)) / zone) * 14
        } else if mouse < visibleBottom + zone {
            step = min(1, ((visibleBottom + zone) - mouse) / zone) * 14
        }
        guard step != 0 else { return }
        // only as far as the list goes: once its end is in view there's nowhere further to drop
        if let probeView = probe.view {
            let list = probeView.convert(probeView.bounds, to: nil)
            if step > 0 && list.minY >= visibleBottom { return }
            if step < 0 && list.maxY <= visibleTop { return }
        }

        let clip = scrollView.contentView
        var origin = clip.bounds.origin
        let flipped = document.isFlipped
        let minY = -scrollView.contentInsets.top
        let maxY = max(minY, document.frame.height - clip.bounds.height + scrollView.contentInsets.bottom)
        origin.y = min(max(origin.y + (flipped ? step : -step), minY), maxY)
        guard origin.y != clip.bounds.origin.y else { return }
        clip.scroll(to: origin)
        scrollView.reflectScrolledClipView(clip)
        follow()
    }
}

// identities for rows told apart only by what they hold, so a row keeps its view (its text
// field's focus, a press in progress) through edits and moves: a row unchanged in its place keeps
// its identity, a moved one is found by what it holds, and an edited one takes the identity left
// over
final class SettingsReorderIdentities {
    private var contents: [String] = []
    private var ids: [String] = []
    private var made = 0

    func ids(for newContents: [String]) -> [String] {
        guard newContents != contents else { return ids }
        var result = [String?](repeating: nil, count: newContents.count)
        var used = Set<String>()
        for index in newContents.indices where contents[safe: index] == newContents[index] {
            result[index] = ids[index]
            used.insert(ids[index])
        }
        var byContent: [String: [String]] = [:]
        for (content, id) in zip(contents, ids) where !used.contains(id) {
            byContent[content, default: []].append(id)
        }
        for index in newContents.indices where result[index] == nil {
            guard let id = byContent[newContents[index]]?.first else { continue }
            byContent[newContents[index]]?.removeFirst()
            result[index] = id
            used.insert(id)
        }
        var leftover = ids.filter { !used.contains($0) }
        let resolved = result.map { id -> String in
            if let id { return id }
            if !leftover.isEmpty { return leftover.removeFirst() }
            made += 1
            return "row \(made)"
        }
        contents = newContents
        ids = resolved
        return resolved
    }
}

private enum SettingsReorderColors {
    // a touch lighter than the grouped rows in dark mode, white in light mode
    static let lifted = NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(white: 0.24, alpha: 1)
            : NSColor.white
    }
}

private struct SettingsRowHeights: PreferenceKey {
    static var defaultValue: [String: CGFloat] = [:]

    static func reduce(value: inout [String: CGFloat], nextValue: () -> [String: CGFloat]) {
        value.merge(nextValue()) { $1 }
    }
}

// the AppKit view behind the list, for its window and the scroll view it sits in
final class SettingsViewProbe {
    weak var view: NSView?
}

private struct SettingsViewProbeView: NSViewRepresentable {
    let probe: SettingsViewProbe

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        probe.view = view
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {
        probe.view = view
    }
}

extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}

