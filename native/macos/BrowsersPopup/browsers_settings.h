// C interface between the Rust app and the native (SwiftUI) macOS Settings and About windows.
// The Swift side exports these with @_cdecl, the Rust side declares them in
// src/macos/native_settings.rs.
// Everything is called on the main thread only, see SETTINGS_CONTRACT.md for the JSON shapes and
// the rules both sides follow.
//
// Strings cross as NUL-terminated UTF-8 JSON.
// The callee copies what it needs, a pointer is only valid for the duration of the call.
//
// Rust may call any of these functions again from inside the event callback (an event handler
// refreshing the window right away), so Swift must be done updating its own state before it
// emits an event.

#pragma once
#include <stdbool.h>

// Swift -> Rust: one user action in Settings or About, as an event JSON object
// (SETTINGS_CONTRACT.md "Events")
typedef void (*browsers_settings_event_cb)(const char *event_json);

// call once, before any other function in this file.
// One callback serves both windows, the event's "type" says which one it came from.
void browsers_settings_init(browsers_settings_event_cb on_event);

// ---- Settings

// opens the Settings window, or brings it to the front when it's already open, with the given
// state (SETTINGS_CONTRACT.md "Settings state").
// Activates the app and makes the window key.
//
// `page` is "browsers", "rules", "general" or "about", the page to select.
// NULL or "" keeps the current page when the window is already open, and selects "browsers" when
// it isn't.
//
// Opening a window that isn't open starts from a clean slate: filter and test-link fields empty,
// scroll positions at the top.
// Bringing an already-open window to the front keeps all of that.
void browsers_settings_show(const char *state_json, const char *page);

// replaces the content of an open Settings window without moving it, changing the page or
// touching a text field the user is editing (rescans, hides, rule edits, theme changes, the
// default browser check).
// A no-op while the window isn't open.
void browsers_settings_update(const char *state_json);

// closes the Settings window without emitting "settings_closed" (the Rust side asked for it, so
// it already knows).
// A no-op while it isn't open.
void browsers_settings_close(void);

// whether the Settings window is on screen.
// Already false while the "settings_closed" event is being handled.
bool browsers_settings_is_open(void);

// ---- About

// opens the About window, or brings it to the front when it's already open, with the given
// content (SETTINGS_CONTRACT.md "About state").
// Activates the app and makes the window key.
void browsers_about_show(const char *about_json);

// replaces the content of an open About window without bringing it to the front (the theme
// changed in Settings).
// A no-op while it isn't open.
void browsers_about_update(const char *about_json);

// closes the About window without emitting "about_closed".
// A no-op while it isn't open.
void browsers_about_close(void);

// whether the About window is on screen.
// Already false while the "about_closed" event is being handled.
bool browsers_about_is_open(void);
