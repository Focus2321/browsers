// Owns the panel: showing, sizing and placing it, and turning keys and menu picks into model
// changes or events for the Rust side.
import AppKit
import SwiftUI

@MainActor
final class PopupController {
    static let shared = PopupController()

    var eventSink: ([String: Any]) -> Void = { _ in }

    private let model = PopupModel()
    private lazy var panel = makePanel()
    private var hostingView: NSView?

    private init() {
        model.emit = { [weak self] event in self?.eventSink(event) }
        model.onRowCountChange = { [weak self] in self?.fitHeight() }
        model.onOptionsMenu = { [weak self] anchor in self?.popUpOptionsMenu(below: anchor) }
    }

    var isVisible: Bool { panel.isVisible }

    // the panel, for the preview harness (screenshots, synthetic keys)
    var window: NSWindow { panel }

    // MARK: C interface

    func show(_ state: PopupState) {
        model.reset(with: state)
        applyAppearance(state.appearance)
        panel.setFrame(frameAtMouse(height: fittedHeight), display: false)
        // lay out the new content before the panel appears, so the previous link doesn't flash
        hostingView?.layoutSubtreeIfNeeded()
        panel.makeKeyAndOrderFront(nil)
        panel.orderFrontRegardless()
        panel.invalidateShadow()
    }

    func update(_ state: PopupState) {
        model.replace(with: state)
        applyAppearance(state.appearance)
        fitHeight()
    }

    func focus() {
        guard panel.isVisible else { return }
        panel.makeKeyAndOrderFront(nil)
    }

    func hide() {
        panel.orderOut(nil)
    }

    // MARK: window

    private func makePanel() -> PopupPanel {
        let panel = PopupPanel()

        // real behind-window vibrancy, rounded through the mask so the system shadow follows
        // the rounded shape too
        let background = NSVisualEffectView()
        background.material = .menu
        background.blendingMode = .behindWindow
        background.state = .active
        background.maskImage = Self.roundedMask(radius: Metrics.cornerRadius)

        let hosting = PopupHostingView(rootView: PopupView(model: model))
        // the panel's size is set from the row count here, not from SwiftUI's ideal size
        hosting.sizingOptions = []
        hosting.autoresizingMask = [.width, .height]
        hosting.frame = background.bounds
        background.addSubview(hosting)
        hostingView = hosting

        panel.contentView = background
        panel.onKeyDown = { [weak self] event in self?.handleKey(event) ?? false }
        panel.onModifiersChanged = { [weak self] flags in
            guard let self else { return }
            let shift = flags.contains(.shift)
            if self.model.privateMode != shift { self.model.privateMode = shift }
        }
        return panel
    }

    private static func roundedMask(radius: CGFloat) -> NSImage {
        let edge = radius * 2 + 1
        let image = NSImage(size: NSSize(width: edge, height: edge), flipped: false) { rect in
            NSColor.black.setFill()
            NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius).fill()
            return true
        }
        image.capInsets = NSEdgeInsets(top: radius, left: radius, bottom: radius, right: radius)
        image.resizingMode = .stretch
        return image
    }

    private func applyAppearance(_ appearance: Appearance) {
        switch appearance {
        case .system: panel.appearance = nil
        case .light: panel.appearance = NSAppearance(named: .aqua)
        case .dark: panel.appearance = NSAppearance(named: .darkAqua)
        }
    }

    private var fittedHeight: CGFloat { Metrics.panelHeight(rowCount: model.rows.count) }

    // top-left corner at the mouse, pushed back inside the visible area of the screen it's on
    // (AppKit's y axis goes up, so the top edge is the frame's maxY)
    private func frameAtMouse(height: CGFloat) -> NSRect {
        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first { NSMouseInRect(mouse, $0.frame, false) } ?? NSScreen.main
        let visible = screen?.visibleFrame ?? NSRect(origin: mouse, size: .zero)
        let x = min(max(mouse.x, visible.minX), visible.maxX - Metrics.width)
        let top = max(min(mouse.y, visible.maxY), visible.minY + height)
        // a fractional origin blurs the content on non-Retina screens
        return NSRect(x: x.rounded(), y: (top - height).rounded(), width: Metrics.width, height: height)
    }

    // re-fits the height to the rows shown, keeping the top edge where it is
    private func fitHeight() {
        let height = fittedHeight
        var frame = panel.frame
        guard frame.height != height else { return }
        frame.origin.y = frame.maxY - height
        frame.size.height = height
        if let visible = panel.screen?.visibleFrame, frame.minY < visible.minY {
            frame.origin.y = visible.minY
        }
        panel.setFrame(frame, display: true)
        panel.invalidateShadow()
    }

    // MARK: keyboard

    private func handleKey(_ event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)

        if flags.contains(.command) {
            switch event.charactersIgnoringModifiers?.lowercased() {
            case "c": model.copyLink()
            case ",": model.send("settings")
            case "q": model.send("quit")
            default: return false
            }
            return true
        }

        switch event.keyCode {
        case KeyCode.escape:
            // the first Esc only clears the filter, the next one closes
            if model.filter.isEmpty { model.send("dismiss") } else { model.setFilter("") }
        case KeyCode.returnKey, KeyCode.keypadEnter:
            model.openSelection()
        case KeyCode.upArrow:
            model.moveSelection(by: -1)
        case KeyCode.downArrow:
            model.moveSelection(by: 1)
        case KeyCode.delete:
            model.setFilter(String(model.filter.dropLast()))
        default:
            if let digit = hotkeyDigit(event) {
                model.open(digit - 1, privately: model.privateMode)
            } else if let text = event.characters, !flags.contains(.control) {
                if text == " " && model.filter.isEmpty {
                    model.openSelection()
                } else if Self.isTypedText(text) {
                    model.setFilter(model.filter + text)
                }
            }
        }
        // every other plain key is swallowed too, the popup has nothing else that wants them and
        // passing them on only ends in the "unhandled key" beep
        return true
    }

    // 1-9 from the number row or the keypad. The physical key counts too, so ⇧2 (private) works
    // even though it types "@", and so do layouts that need ⇧ for digits, except where the key
    // types a letter (those go into the filter instead).
    private func hotkeyDigit(_ event: NSEvent) -> Int? {
        if let text = event.characters, text.count == 1, let digit = Int(text), (1...9).contains(digit) {
            return digit
        }
        if let text = event.characters, text.contains(where: \.isLetter) { return nil }
        return KeyCode.digits[event.keyCode]
    }

    // something the user typed, as opposed to arrows, function keys and other special keys
    // (control characters or private-use codepoints U+F700..U+F8FF in AppKit). Digits stay hotkeys.
    private static func isTypedText(_ text: String) -> Bool {
        !text.isEmpty && text.unicodeScalars.allSatisfy { scalar in
            !CharacterSet.controlCharacters.contains(scalar)
                && !(0xF700...0xF8FF).contains(scalar.value)
                && !("0"..."9").contains(scalar)
        }
    }

    // MARK: options menu

    private func popUpOptionsMenu(below anchor: CGRect) {
        guard let hostingView else { return }
        let state = model.state
        let menu = ActionMenu()
        menu.add("Refresh") { [model] in model.send("refresh") }
        if state.showSetAsDefault {
            menu.add("Make Browsers Default") { [model] in model.send("set_default") }
        }
        let restore = ActionMenu()
        if state.restorable.isEmpty {
            restore.addDisabled("No hidden apps or profiles")
        }
        for entry in state.restorable {
            restore.add(entry.fullName) { [model] in
                model.send("restore_profile", ["unique_id": entry.uniqueId])
            }
        }
        menu.addSubmenu("Restore", restore)
        menu.addItem(.separator())
        menu.add("Settings…", key: ",") { [model] in model.send("settings") }
        menu.add("About") { [model] in model.send("about") }
        menu.addItem(.separator())
        menu.add("Quit", key: "q") { [model] in model.send("quit") }

        // NSHostingView is flipped, so the anchor's maxY is its bottom edge
        menu.popUp(positioning: nil, at: NSPoint(x: anchor.minX, y: anchor.maxY + 4), in: hostingView)
    }
}

// virtual key codes (Carbon's kVK_*), layout independent
private enum KeyCode {
    static let returnKey: UInt16 = 36
    static let delete: UInt16 = 51
    static let escape: UInt16 = 53
    static let keypadEnter: UInt16 = 76
    static let downArrow: UInt16 = 125
    static let upArrow: UInt16 = 126
    static let digits: [UInt16: Int] = [
        18: 1, 19: 2, 20: 3, 21: 4, 23: 5, 22: 6, 26: 7, 28: 8, 25: 9,
        83: 1, 84: 2, 85: 3, 86: 4, 87: 5, 88: 6, 89: 7, 91: 8, 92: 9,
    ]
}
