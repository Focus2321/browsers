// Placeholder for the native Settings and About windows (browsers_settings.h), so the Rust side
// builds and links before the real SwiftUI implementation lands.
// Delete this file once SettingsExports.swift exists.
// Showing and updating do nothing, and neither window ever reports itself open.
import AppKit

@_cdecl("browsers_settings_init")
public func browsers_settings_init(_ onEvent: (@convention(c) (UnsafePointer<CChar>?) -> Void)?) {}

@_cdecl("browsers_settings_show")
public func browsers_settings_show(_ stateJSON: UnsafePointer<CChar>?, _ page: UnsafePointer<CChar>?) {}

@_cdecl("browsers_settings_update")
public func browsers_settings_update(_ stateJSON: UnsafePointer<CChar>?) {}

@_cdecl("browsers_settings_close")
public func browsers_settings_close() {}

@_cdecl("browsers_settings_is_open")
public func browsers_settings_is_open() -> Bool { false }

@_cdecl("browsers_about_show")
public func browsers_about_show(_ aboutJSON: UnsafePointer<CChar>?) {}

@_cdecl("browsers_about_update")
public func browsers_about_update(_ aboutJSON: UnsafePointer<CChar>?) {}

@_cdecl("browsers_about_close")
public func browsers_about_close() {}

@_cdecl("browsers_about_is_open")
public func browsers_about_is_open() -> Bool { false }
