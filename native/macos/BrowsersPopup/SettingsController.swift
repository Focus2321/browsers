// Owns the Settings and About windows: creating, showing, closing them and reporting the user
// closing them (SETTINGS_CONTRACT.md "Window lifecycle").
import AppKit
import SwiftUI

@MainActor
final class SettingsController {
    static let shared = SettingsController()

    static let defaultSize = NSSize(width: 780, height: 552)
    static let minimumSize = NSSize(width: 680, height: 460)
    static let frameAutosaveName = "BrowsersSettingsWindow"

    let model = SettingsModel()
    private var settingsWindow: SettingsWindow?

    private init() {
        model.emit = { type, fields in SettingsEvents.send(type, fields) }
    }

    var isOpen: Bool { settingsWindow?.isVisible ?? false }

    // the window, for the preview harness (screenshots, synthetic input)
    var window: NSWindow? { settingsWindow }

    func show(_ state: SettingsState, page: SettingsPage?) {
        let window = settingsWindow ?? makeWindow()
        if window.isVisible {
            model.replace(with: state)
            if let page { model.select(page) }
        } else {
            model.reset(with: state, page: page)
        }
        window.apply(state.appearance)
        activateSettingsApp()
        window.makeKeyAndOrderFront(nil)
    }

    func update(_ state: SettingsState) {
        guard let window = settingsWindow, window.isVisible else { return }
        model.replace(with: state)
        window.apply(state.appearance)
    }

    func close() {
        settingsWindow?.orderOut(nil)
    }

    private func userClosed() {
        guard let window = settingsWindow, window.isVisible else { return }
        // ordered out first, so is_open is already false while Rust handles the event
        window.orderOut(nil)
        model.endDrag()
        SettingsEvents.send("settings_closed")
    }

    private func makeWindow() -> SettingsWindow {
        let window = SettingsWindow(
            contentRect: NSRect(origin: .zero, size: Self.defaultSize),
            styleMask: [.titled, .closable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = "Settings"
        window.isReleasedWhenClosed = false
        window.toolbarStyle = .unified
        window.contentMinSize = Self.minimumSize
        // above the popup, which floats too (like app::make_window_floating did for Slint)
        window.level = .floating
        window.collectionBehavior = [.fullScreenAuxiliary]
        window.tabbingMode = .disallowed
        window.onUserClose = { [weak self] in self?.userClosed() }

        let hosting = NSHostingController(rootView: SettingsRootView(model: model))
        // the window's size is the user's, not SwiftUI's ideal size
        hosting.sizingOptions = []
        window.contentViewController = hosting
        window.setContentSize(Self.defaultSize)
        window.center()
        // restores the last frame when there is one
        window.setFrameAutosaveName(Self.frameAutosaveName)
        settingsWindow = window
        return window
    }
}

@MainActor
final class AboutController {
    static let shared = AboutController()

    private let model = AboutModel()
    private var aboutWindow: SettingsWindow?

    private init() {}

    var isOpen: Bool { aboutWindow?.isVisible ?? false }

    var window: NSWindow? { aboutWindow }

    func show(_ state: AboutState) {
        let window = aboutWindow ?? makeWindow()
        model.state = state
        window.apply(state.appearance)
        if !window.isVisible {
            // sized for the new content first, so it's centered as it will be
            if let view = window.contentViewController?.view {
                view.layoutSubtreeIfNeeded()
                window.setContentSize(view.fittingSize)
            }
            window.center()
        }
        activateSettingsApp()
        window.makeKeyAndOrderFront(nil)
    }

    func update(_ state: AboutState) {
        guard let window = aboutWindow, window.isVisible else { return }
        model.state = state
        window.apply(state.appearance)
    }

    func close() {
        aboutWindow?.orderOut(nil)
    }

    private func userClosed() {
        guard let window = aboutWindow, window.isVisible else { return }
        window.orderOut(nil)
        SettingsEvents.send("about_closed")
    }

    private func makeWindow() -> SettingsWindow {
        let window = SettingsWindow(
            contentRect: NSRect(x: 0, y: 0, width: 300, height: 400),
            styleMask: [.titled, .closable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = ""
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isMovableByWindowBackground = true
        window.isReleasedWhenClosed = false
        window.level = .floating
        window.collectionBehavior = [.fullScreenAuxiliary]
        window.tabbingMode = .disallowed
        window.standardWindowButton(.miniaturizeButton)?.isHidden = true
        window.standardWindowButton(.zoomButton)?.isHidden = true
        window.onUserClose = { [weak self] in self?.userClosed() }

        let hosting = NSHostingController(rootView: AboutWindowView(model: model))
        // the window follows the content's size, there's nothing to resize
        hosting.sizingOptions = [.preferredContentSize]
        window.contentViewController = hosting
        aboutWindow = window
        return window
    }
}

@MainActor
final class AboutModel: ObservableObject {
    @Published var state = AboutState()
}
