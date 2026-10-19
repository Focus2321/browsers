// C entry points of the Settings and About windows, see browsers_settings.h and
// SETTINGS_CONTRACT.md.
// The Rust side calls all of them on the main thread, so they hop straight into the main actor.
import AppKit

public typealias BrowsersSettingsEventCallback = @convention(c) (UnsafePointer<CChar>?) -> Void

@_cdecl("browsers_settings_init")
public func browsers_settings_init(_ onEvent: BrowsersSettingsEventCallback?) {
    MainActor.assumeIsolated {
        SettingsEvents.sink = { json in
            guard let onEvent else { return }
            json.withCString { onEvent($0) }
        }
    }
}

@_cdecl("browsers_settings_show")
public func browsers_settings_show(_ stateJSON: UnsafePointer<CChar>?, _ page: UnsafePointer<CChar>?) {
    guard let state = SettingsState.decode(stateJSON) else { return }
    // NULL or "" keeps the page (or starts on Browsers), an unknown name too
    let selected = page.flatMap { SettingsPage(rawValue: String(cString: $0)) }
    MainActor.assumeIsolated { SettingsController.shared.show(state, page: selected) }
}

@_cdecl("browsers_settings_update")
public func browsers_settings_update(_ stateJSON: UnsafePointer<CChar>?) {
    guard let state = SettingsState.decode(stateJSON) else { return }
    MainActor.assumeIsolated { SettingsController.shared.update(state) }
}

@_cdecl("browsers_settings_close")
public func browsers_settings_close() {
    MainActor.assumeIsolated { SettingsController.shared.close() }
}

@_cdecl("browsers_settings_is_open")
public func browsers_settings_is_open() -> Bool {
    MainActor.assumeIsolated { SettingsController.shared.isOpen }
}

@_cdecl("browsers_about_show")
public func browsers_about_show(_ aboutJSON: UnsafePointer<CChar>?) {
    guard let state = AboutState.decode(aboutJSON) else { return }
    MainActor.assumeIsolated { AboutController.shared.show(state) }
}

@_cdecl("browsers_about_update")
public func browsers_about_update(_ aboutJSON: UnsafePointer<CChar>?) {
    guard let state = AboutState.decode(aboutJSON) else { return }
    MainActor.assumeIsolated { AboutController.shared.update(state) }
}

@_cdecl("browsers_about_close")
public func browsers_about_close() {
    MainActor.assumeIsolated { AboutController.shared.close() }
}

@_cdecl("browsers_about_is_open")
public func browsers_about_is_open() -> Bool {
    MainActor.assumeIsolated { AboutController.shared.isOpen }
}
