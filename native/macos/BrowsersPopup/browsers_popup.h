// C interface between the Rust app and the native (SwiftUI) macOS popup.
// The Swift side exports these with @_cdecl, the Rust side declares them in src/macos/native_popup.rs.
// Everything is called on the main thread only.
//
// Strings cross as NUL-terminated UTF-8 JSON, see CONTRACT.md for the shapes.
// The callee copies what it needs, a pointer is only valid for the duration of the call.

#pragma once
#include <stdbool.h>

// Swift -> Rust: one user action, as an event JSON object (CONTRACT.md "Events")
typedef void (*browsers_popup_event_cb)(const char *event_json);

// call once before anything else
void browsers_popup_init(browsers_popup_event_cb on_event);

// shows the popup for a new link: replaces the content, resets the per-appearance state (filter,
// selection, private mode), positions it at the mouse cursor and makes it the key window.
// The Rust side activates the app right before calling this.
void browsers_popup_show(const char *state_json);

// replaces the content of the popup without moving it or resetting the filter/selection
// (profiles rescanned, hidden, restored, theme changed). Also fine while hidden.
void browsers_popup_update(const char *state_json);

// makes an already-visible popup the key window again (after Settings/About closed)
void browsers_popup_focus(void);

void browsers_popup_hide(void);

bool browsers_popup_is_visible(void);
