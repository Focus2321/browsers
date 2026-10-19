// Pieces the Settings and About windows share: the event channel to Rust, the window class
// (keyboard handling for an app without an Edit menu), appearance, and icon views.
import AppKit
import SwiftUI

// MARK: events

// the one callback both windows report to (browsers_settings_init)
@MainActor
enum SettingsEvents {
    static var sink: (String) -> Void = { _ in }

    // `type` first, then the fields in the order given (SETTINGS_CONTRACT.md "Events")
    static func send(_ type: String, _ fields: KeyValuePairs<String, Any> = [:]) {
        var parts = ["\"type\":\(jsonValue(type))"]
        for (key, value) in fields {
            parts.append("\(jsonValue(key)):\(jsonValue(value))")
        }
        sink("{" + parts.joined(separator: ",") + "}")
    }

    private static func jsonValue(_ value: Any) -> String {
        guard let data = try? JSONSerialization.data(
            withJSONObject: value, options: [.fragmentsAllowed, .withoutEscapingSlashes]),
            let text = String(data: data, encoding: .utf8)
        else { return "null" }
        return text
    }
}

// MARK: window

// A standard titled window that handles the shortcuts a menu bar would normally provide: the app
// is an accessory app without File or Edit menus, so nothing else turns ⌘W, ⌘C, ⌘Z, ... into
// actions.
final class SettingsWindow: NSWindow {
    // the user asked to close it (close button, ⌘W); the owner orders it out and reports it
    var onUserClose: () -> Void = {}

    override func performClose(_ sender: Any?) {
        // not super: that would close() the window, which then has to be rebuilt, and beeps when
        // the close is refused. Ordering out keeps the window and its frame for the next open.
        onUserClose()
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if super.performKeyEquivalent(with: event) { return true }
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        guard flags.contains(.command), !flags.contains(.control), !flags.contains(.option),
              let key = event.charactersIgnoringModifiers?.lowercased()
        else { return false }
        let shift = flags.contains(.shift)

        let action: Selector?
        switch (key, shift) {
        case ("w", false): performClose(nil); return true
        case ("x", false): action = #selector(NSText.cut(_:))
        case ("c", false): action = #selector(NSText.copy(_:))
        case ("v", false): action = #selector(NSText.paste(_:))
        case ("a", false): action = #selector(NSText.selectAll(_:))
        case ("z", false): action = Selector(("undo:"))
        case ("z", true): action = Selector(("redo:"))
        default: action = nil
        }
        guard let action else { return false }
        return NSApp.sendAction(action, to: nil, from: self)
    }
}

extension NSWindow {
    func apply(_ appearance: Appearance) {
        switch appearance {
        case .system: self.appearance = nil
        case .light: self.appearance = NSAppearance(named: .aqua)
        case .dark: self.appearance = NSAppearance(named: .darkAqua)
        }
    }
}

// like macos_native::activate_app: an accessory app has to bring itself forward explicitly
@MainActor
func activateSettingsApp() {
    NSApp.unhideWithoutActivation()
    NSApp.activate(ignoringOtherApps: true)
}

@MainActor
func openSettingsLink(_ text: String) {
    guard let url = URL(string: text) else { return }
    NSWorkspace.shared.open(url)
}

// MARK: icons

// an app icon PNG (IconCache, shared with the popup), the generic app icon when it's missing
struct SettingsAppIcon: View {
    let path: String
    let size: CGFloat

    var body: some View {
        Image(nsImage: IconCache.image(path) ?? NSWorkspace.shared.icon(for: .applicationBundle))
            .resizable()
            .interpolation(.high)
            .aspectRatio(contentMode: .fit)
            .frame(width: size, height: size)
    }
}

// a row's picture: its icon, else the letter in a `letter_color` circle
struct SettingsAvatar: View {
    let iconPath: String
    let letter: String
    let color: String
    let size: CGFloat
    // profile pictures are round, app icons keep their own shape
    var circular = false

    var body: some View {
        if let image = IconCache.image(iconPath) {
            Image(nsImage: image)
                .resizable()
                .interpolation(.high)
                .aspectRatio(contentMode: circular ? .fill : .fit)
                .frame(width: size, height: size)
                .clipShape(circular ? AnyShape(Circle()) : AnyShape(Rectangle()))
        } else {
            Circle()
                .fill(Color(avatarHex: color) ?? Color(nsColor: .systemGray))
                .frame(width: size, height: size)
                .overlay(
                    Text(verbatim: letter)
                        .font(.system(size: size * 0.5, weight: .semibold))
                        .foregroundColor(.white)
                )
        }
    }
}

// small icons for menu items, which only show an image at its own size
@MainActor
enum SettingsMenuIcon {
    private static var images: [String: NSImage] = [:]

    static func image(_ path: String, size: CGFloat = 16) -> NSImage? {
        let key = "\(size)|\(path)"
        if let image = images[key] { return image }
        guard let source = IconCache.image(path) else { return nil }
        let image = NSImage(size: NSSize(width: size, height: size), flipped: false) { rect in
            source.draw(in: rect, from: .zero, operation: .sourceOver, fraction: 1)
            return true
        }
        images[key] = image
        return image
    }
}

extension Color {
    // "#7c5ce6", the avatar colors Rust derives from the data
    init?(avatarHex hex: String) {
        let digits = hex.hasPrefix("#") ? String(hex.dropFirst()) : hex
        guard digits.count == 6, let value = UInt32(digits, radix: 16) else { return nil }
        self.init(
            .sRGB,
            red: Double((value >> 16) & 0xFF) / 255,
            green: Double((value >> 8) & 0xFF) / 255,
            blue: Double(value & 0xFF) / 255
        )
    }
}
