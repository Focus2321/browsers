// Placeholder so the Rust build links while the real popup is being written.
import AppKit

private var onEvent: (@convention(c) (UnsafePointer<CChar>) -> Void)?

@_cdecl("browsers_popup_init")
public func browsers_popup_init(_ cb: @convention(c) (UnsafePointer<CChar>) -> Void) { onEvent = cb }

@_cdecl("browsers_popup_show")
public func browsers_popup_show(_ json: UnsafePointer<CChar>) {}

@_cdecl("browsers_popup_update")
public func browsers_popup_update(_ json: UnsafePointer<CChar>) {}

@_cdecl("browsers_popup_focus")
public func browsers_popup_focus() {}

@_cdecl("browsers_popup_hide")
public func browsers_popup_hide() {}

@_cdecl("browsers_popup_is_visible")
public func browsers_popup_is_visible() -> Bool { false }
