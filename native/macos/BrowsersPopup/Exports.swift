// C entry points of the popup, see browsers_popup.h and CONTRACT.md.
// The Rust side calls all of them on the main thread, so they hop straight into the main actor.
import AppKit

public typealias BrowsersPopupEventCallback = @convention(c) (UnsafePointer<CChar>?) -> Void

@_cdecl("browsers_popup_init")
public func browsers_popup_init(_ onEvent: BrowsersPopupEventCallback?) {
    MainActor.assumeIsolated {
        PopupController.shared.eventSink = { event in
            guard let onEvent,
                  let data = try? JSONSerialization.data(withJSONObject: event, options: [.sortedKeys, .withoutEscapingSlashes]),
                  let json = String(data: data, encoding: .utf8)
            else { return }
            json.withCString { onEvent($0) }
        }
    }
}

@_cdecl("browsers_popup_show")
public func browsers_popup_show(_ stateJSON: UnsafePointer<CChar>?) {
    guard let state = PopupState.decode(stateJSON) else { return }
    MainActor.assumeIsolated { PopupController.shared.show(state) }
}

@_cdecl("browsers_popup_update")
public func browsers_popup_update(_ stateJSON: UnsafePointer<CChar>?) {
    guard let state = PopupState.decode(stateJSON) else { return }
    MainActor.assumeIsolated { PopupController.shared.update(state) }
}

@_cdecl("browsers_popup_focus")
public func browsers_popup_focus() {
    MainActor.assumeIsolated { PopupController.shared.focus() }
}

@_cdecl("browsers_popup_hide")
public func browsers_popup_hide() {
    MainActor.assumeIsolated { PopupController.shared.hide() }
}

@_cdecl("browsers_popup_is_visible")
public func browsers_popup_is_visible() -> Bool {
    MainActor.assumeIsolated { PopupController.shared.isVisible }
}
