# Native popup contract

The Rust side owns the data and every action, the Swift side owns the window, the drawing and the keyboard/mouse handling.
See `browsers_popup.h` for the functions.

## State (Rust -> Swift)

```json
{
  "url": "https://github.com/slint-ui/slint/pull/123",
  "url_domain": "github.com",
  "url_path": "/slint-ui/slint/pull/123",
  "source_app_name": "Slack",
  "show_hotkeys": true,
  "show_set_as_default": false,
  "appearance": "system",
  "browsers": [
    {
      "profile_index": 3,
      "unique_id": "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome#Profile 1",
      "unique_app_id": "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
      "browser_name": "Chrome",
      "profile_name": "Tallgrass",
      "full_name": "Chrome (Tallgrass)",
      "supports_profiles": true,
      "supports_incognito": true,
      "icon_path": "/Users/me/Library/Caches/software.Browsers/icons/chrome.png",
      "profile_icon_path": "",
      "profile_initial": "T",
      "avatar_color": "#7c5ce6"
    }
  ],
  "restorable": [
    { "unique_id": "...", "full_name": "Firefox (Work)" }
  ]
}
```

- `browsers` is already narrowed down by the link's opening rules and in display order.
  Type-to-filter, the 1-9 hotkeys and keyboard selection happen on the Swift side over this list.
- `appearance` is `"system"`, `"light"` or `"dark"`.
- `url_domain` is empty when there's no link (popup opened without one): show "No link".
- `source_app_name` may be empty.
- `icon_path`/`profile_icon_path` are PNG files, or empty.
  With an empty `profile_icon_path` on a `supports_profiles` row, draw the `profile_initial` in an `avatar_color` circle as the profile badge.

## Events (Swift -> Rust)

```json
{ "type": "open", "profile_index": 3, "private": false }
{ "type": "copy_link" }
{ "type": "always_open_here", "unique_id": "...", "profile_index": 3 }
{ "type": "hide_profile", "unique_id": "..." }
{ "type": "hide_app", "unique_app_id": "..." }
{ "type": "restore_profile", "unique_id": "..." }
{ "type": "refresh" }
{ "type": "set_default" }
{ "type": "settings" }
{ "type": "about" }
{ "type": "quit" }
{ "type": "dismiss" }
```

- `open` with `private: true` is only sent for rows with `supports_incognito`.
- `dismiss` is Esc with an empty filter (the first Esc clears the filter).
  The Rust side decides whether that hides the popup or quits.
- Swift never hides the popup by itself on `open`/`dismiss`, Rust calls `browsers_popup_hide` when it's done.
- Clicking another app is handled on the Rust side (app resign-active), not by the popup.
