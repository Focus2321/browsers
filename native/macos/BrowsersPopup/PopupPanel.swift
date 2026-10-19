// The borderless floating panel the popup lives in.
import AppKit
import SwiftUI

final class PopupPanel: NSPanel {
    // return true when the key was handled, so it isn't passed on
    var onKeyDown: (NSEvent) -> Bool = { _ in false }
    var onModifiersChanged: (NSEvent.ModifierFlags) -> Void = { _ in }

    init() {
        super.init(contentRect: .zero, styleMask: [.borderless], backing: .buffered, defer: false)
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        // kCGFloatingWindowLevel, the level the Slint popup used
        level = .floating
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle, .transient]
        isExcludedFromWindowsMenu = true
        // NSPanel defaults to hiding when the app deactivates, but the Rust side decides when the
        // popup goes away (it reacts to the app resigning active itself)
        hidesOnDeactivate = false
        becomesKeyOnlyIfNeeded = false
        isReleasedWhenClosed = false
        isMovable = false
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    // keys are handled here rather than in SwiftUI, so they work no matter which view (if any)
    // has the focus
    override func sendEvent(_ event: NSEvent) {
        switch event.type {
        case .keyDown:
            if onKeyDown(event) { return }
        case .flagsChanged:
            onModifiersChanged(event.modifierFlags)
        default:
            break
        }
        super.sendEvent(event)
    }

    // ⌘-shortcuts arrive as key equivalents, before sendEvent and the app's main menu see them
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if event.modifierFlags.contains(.command), onKeyDown(event) { return true }
        return super.performKeyEquivalent(with: event)
    }

    override func resignKey() {
        super.resignKey()
        // ⇧ released while another window was key never reaches flagsChanged here
        onModifiersChanged([])
    }
}

// lets the first click on the popup count, even if it arrives before the panel became key
final class PopupHostingView<Content: View>: NSHostingView<Content> {
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}
