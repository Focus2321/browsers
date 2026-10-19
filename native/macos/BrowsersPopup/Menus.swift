// NSMenu with closure-backed items, for the "···" options menu.
import AppKit

final class ActionMenu: NSMenu {
    init() {
        super.init(title: "")
        autoenablesItems = false
    }

    required init(coder: NSCoder) { fatalError("not used from nibs") }

    func add(_ title: String, key: String = "", action: @escaping () -> Void) {
        addItem(ActionMenuItem(title: title, key: key, action: action))
    }

    func addDisabled(_ title: String) {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.isEnabled = false
        addItem(item)
    }

    func addSubmenu(_ title: String, _ submenu: NSMenu) {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.submenu = submenu
        addItem(item)
    }
}

private final class ActionMenuItem: NSMenuItem {
    private let handler: () -> Void

    init(title: String, key: String, action: @escaping () -> Void) {
        handler = action
        super.init(title: title, action: #selector(run), keyEquivalent: key)
        target = self
    }

    required init(coder: NSCoder) { fatalError("not used from nibs") }

    @objc private func run() { handler() }
}
