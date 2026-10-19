# Native Settings and About contract

Same split as the popup (see `CONTRACT.md`): the Rust side owns the data, every action and every piece of logic that exists today, the Swift side owns the windows, the drawing and the mouse/keyboard handling.
Swift renders the state it was given and reports what the user did, Rust answers with `browsers_settings_update` (or `browsers_about_update`).
See `browsers_settings.h` for the functions.

This replaces `ui/settings_window.slint` + `src/gui/settings_window.rs` and `ui/about_window.slint` + `src/gui/about_window.rs` on macOS 13+.
Every feature of those is covered below, the few deliberate changes are listed in "Changes from the Slint windows".

## Files and names

- Swift: new top-level `.swift` files in this directory (build.rs compiles every top-level `.swift` file here into the same `BrowsersPopup` module and static library as the popup).
  All files share one module, so Swift types use a `Settings`/`About` prefix to stay clear of the popup's (`PopupState`, `PopupModel`, ...).
  The `@_cdecl` exports go in `SettingsExports.swift`.
  Don't change the popup's files.
  An optional preview harness can live next to `Preview/` (that folder isn't compiled into the library).
- Rust: `src/macos/native_settings.rs`, built like `src/macos/native_popup.rs` (extern block, serde types for state and events, `catch_unwind` around the handler, fn pointer in a `Cell`).
  The native windows are used whenever the native popup is available (`native_popup::unavailable_reason()` is `None`).
  The Slint Settings/About stay as the fallback, `SLINT_BACKEND=headless` (the MCP UI tests in CLAUDE.md) keeps using them.

## Ground rules

- Main thread only, both directions.
- Every state JSON is complete: Swift replaces everything it shows with it, there are no partial updates.
- Swift ignores JSON fields it doesn't know, and drops (and logs) a call whose JSON doesn't decode.
- Rust ignores (and logs) an event it doesn't know or whose row/index no longer exists.
- Rust may call back into Swift (`update`, `close`, `is_open`) synchronously from inside an event, so Swift finishes its own state changes before emitting.
- Every string that depends on data is precomputed by Rust (row details, the banner, hints, the found-text, picker labels, the test result, the draft help).
  Fixed copy (titles, button labels, helper lines) is listed per page below and hardcoded in Swift.
- Icons are absolute paths to PNG files, `""` when there is none.
  A path that fails to load counts as `""`.
- `letter_color` values are `#rrggbb` avatar colors derived from the data (the same colors the popup uses), not theme colors.
  Everything else uses system colors.

### Text fields

Swift owns what's typed, Rust owns everything derived from it:

| Field | Swift sends | State echoes | Swift sets the field from state |
|---|---|---|---|
| Browsers filter | `browsers_filter_changed` on every edit | `browsers.filter` | only when a closed window opens (always `""`) |
| Test a link | `test_url_edited` on every edit | `rules.test.url` | only when a closed window opens (always `""`) |
| New rule pattern | `draft_pattern_edited` on every edit, `draft_pattern_committed` on Return and on focus loss | `rules.draft.pattern` | when a draft appears (focus the field then); clear it when `rules.draft` becomes `null` |
| A rule's pattern | `rule_pattern_edited` on every edit | `pattern`, `pattern_invalid`, `pattern_edit` | while the field isn't focused: shows `pattern_edit` (in red) when `pattern_invalid`, `pattern` otherwise |

Swift never replaces the text of a focused field from an update.

## Settings state (Rust -> Swift)

Passed to `browsers_settings_show` and `browsers_settings_update`.

```json
{
  "appearance": "system",
  "is_default_browser": false,
  "browsers": {
    "filter": "",
    "show_hidden": false,
    "hidden_count": 1,
    "reorder_enabled": true,
    "order_hint": "Drag to reorder · 1–9 follow this order",
    "banner": {
      "title": "2 of these aren't web browsers",
      "message": "Slack and Zoom can open links, but you probably don't want them in the popup."
    },
    "found_text": "Found 9 apps · checked just now",
    "empty_text": "No apps found. Try Rescan.",
    "rows": [
      {
        "id": "/Applications/Safari.app",
        "kind": "single",
        "app_id": "/Applications/Safari.app",
        "name": "Safari",
        "detail": "",
        "icon_path": "/Users/me/Library/Caches/software.Browsers/icons/safari.png",
        "letter": "S",
        "letter_color": "#5b8def",
        "hidden": false,
        "is_default": false,
        "collapsed": false,
        "draggable": true,
        "menu": [
          { "action": "move_to_top", "title": "Move to top" },
          { "action": "move_to_bottom", "title": "Move to bottom" },
          { "action": "toggle_default", "title": "Open unmatched links here" }
        ]
      },
      {
        "id": "group:/Applications/Google Chrome.app",
        "kind": "header",
        "app_id": "/Applications/Google Chrome.app",
        "name": "Chrome",
        "detail": "3 profiles · 1 hidden",
        "icon_path": "/Users/me/Library/Caches/software.Browsers/icons/chrome.png",
        "letter": "C",
        "letter_color": "#e5734a",
        "hidden": false,
        "is_default": false,
        "collapsed": false,
        "draggable": true,
        "menu": [
          { "action": "move_to_top", "title": "Move to top" },
          { "action": "move_to_bottom", "title": "Move to bottom" }
        ]
      },
      {
        "id": "/Applications/Google Chrome.app#Profile 1",
        "kind": "child",
        "app_id": "/Applications/Google Chrome.app",
        "name": "Work",
        "detail": "",
        "icon_path": "",
        "letter": "W",
        "letter_color": "#3fb27f",
        "hidden": false,
        "is_default": true,
        "collapsed": false,
        "draggable": true,
        "menu": [
          { "action": "move_to_top", "title": "Move to top" },
          { "action": "move_to_bottom", "title": "Move to bottom" },
          { "action": "toggle_default", "title": "Stop opening unmatched links here" }
        ]
      }
    ],
    "special_rows": [
      {
        "id": "/Applications/Slack.app#T0123",
        "kind": "single",
        "app_id": "/Applications/Slack.app",
        "name": "Slack",
        "detail": "Acme",
        "icon_path": "/Users/me/Library/Caches/software.Browsers/icons/slack.png",
        "letter": "S",
        "letter_color": "#a66de8",
        "hidden": false,
        "is_default": false,
        "collapsed": false,
        "draggable": false,
        "menu": [{ "action": "toggle_default", "title": "Open unmatched links here" }]
      }
    ]
  },
  "rules": {
    "rules": [
      {
        "pattern": "github.com/acme/**",
        "pattern_invalid": false,
        "pattern_edit": "",
        "source_id": "com.tinyspeck.slackmacgap",
        "source_label": "Slack",
        "target_id": "/Applications/Google Chrome.app#Profile 1",
        "target_label": "Work · Chrome",
        "target_icon_path": "/Users/me/Library/Caches/software.Browsers/icons/chrome.png",
        "private": false,
        "supports_private": true,
        "matched": false
      }
    ],
    "source_options": [
      { "id": "", "label": "Any app" },
      { "id": "com.google.Chrome", "label": "Chrome" },
      { "id": "com.apple.mail", "label": "Mail" }
    ],
    "target_options": [
      { "id": "", "label": "Show the picker", "icon_path": "" },
      {
        "id": "/Applications/Google Chrome.app#Profile 1",
        "label": "Work · Chrome",
        "icon_path": "/Users/me/Library/Caches/software.Browsers/icons/chrome.png"
      },
      {
        "id": "/Applications/Firefox.app#Home",
        "label": "Home · Firefox (hidden)",
        "icon_path": "/Users/me/Library/Caches/software.Browsers/icons/firefox.png"
      }
    ],
    "draft": {
      "pattern": "notion.so",
      "pattern_placeholder": "e.g. notion.so",
      "pattern_invalid": false,
      "source_id": "",
      "source_label": "",
      "target_id": null,
      "target_label": "",
      "help": "Matches notion.so and every page on it. Saved once it has a pattern and a target."
    },
    "test": {
      "url": "https://github.com/acme/api",
      "source_id": "",
      "source_label": "Any app",
      "result_kind": "rule",
      "result_text": "Rule 1 matches → opens in Work · Chrome"
    },
    "fallback": {
      "target_id": "",
      "target_label": "Show the picker",
      "target_icon_path": "",
      "private": false,
      "supports_private": false,
      "matched": false
    }
  },
  "general": {
    "theme": "system",
    "native_popup_available": true,
    "native_popup": true,
    "show_hotkeys": true,
    "quit_on_lost_focus_visible": false,
    "quit_on_lost_focus": false,
    "unwrap_urls": true,
    "run_in_background": true
  },
  "about": {
    "app_icon_path": "/Applications/Browsers.app/Contents/Resources/icons/512x512/software.Browsers.png",
    "app_name": "Browsers",
    "version_text": "Version 0.6.0",
    "copyright": "© 2022-2024 Browsers.software team.",
    "website_url": "https://browsers.software",
    "directories": [
      { "label": "Config", "path": "/Users/me/Library/Application Support/software.Browsers/" },
      { "label": "Cache", "path": "/Users/me/Library/Caches/software.Browsers/" },
      { "label": "Logs", "path": "/Users/me/Library/Logs/software.Browsers/" },
      { "label": "Resources", "path": "/Applications/Browsers.app/Contents/Resources/" }
    ],
    "made_with_slint": {
      "label": "Made with Slint",
      "version_text": "Version 1.18.1",
      "url": "https://slint.dev"
    }
  }
}
```

### Top level

| Field | Type | Meaning |
|---|---|---|
| `appearance` | `"system"` \| `"light"` \| `"dark"` | the window's appearance (`ConfiguredTheme` Auto/Light/Dark), same values as the popup. Swift sets `NSWindow.appearance` to `nil` / `.aqua` / `.darkAqua`. Always equal to `general.theme`. |
| `is_default_browser` | bool | drives the sidebar's default-browser card (`utils::is_default_web_browser()`). Rust re-checks it after `set_default_browser`, see the events. |
| `browsers` | object | the Browsers page |
| `rules` | object | the Rules page |
| `general` | object | the General page |
| `about` | object | the About page, same shape as the About window's content (see "About state") |

### `browsers`

Built by today's `refresh_browsers`.

| Field | Type | Meaning |
|---|---|---|
| `filter` | string | the filter text Rust last received (see "Text fields") |
| `show_hidden` | bool | the "Show hidden" toggle button is on (window-local, Rust's `Local.show_hidden`) |
| `hidden_count` | int | number of hidden profiles, the badge on "Show hidden" (no badge when 0) |
| `reorder_enabled` | bool | false while the filter isn't empty: no dragging at all |
| `order_hint` | string | the hint right of the "Popup order" heading: `"Drag to reorder · 1–9 follow this order"` or `"Clear the filter to reorder"` |
| `banner` | object \| null | the "not web browsers" banner, `null` = don't show it (no such apps, or dismissed for good via `non_browser_banner_dismissed`). `title` is `"1 of these isn't a web browser"` / `"N of these aren't web browsers"`, `message` is `"<names> can open links, but you probably don't want them in the popup."` with names joined like `"Slack, Zoom, Figma and 2 more"`. |
| `found_text` | string | footer, `"Found 9 apps · checked 2 min ago"` (or `"Found 9 apps"` without a scan time) |
| `empty_text` | string | shown in place of the list when `rows` and `special_rows` are both empty: `"Nothing matches \"<filter>\""` or `"No apps found. Try Rescan."` |
| `rows` | array of Row | the orderable apps and profiles, flattened in popup order, already filtered, hidden rows only included with `show_hidden`, children of collapsed groups left out |
| `special_rows` | array of Row | apps that only open their own links (`has_priority_ordering()`: Slack, Zoom, ...), always listed first in the popup, never draggable. Filtered the same way. Shown in their own section after `rows`. |

#### Row

| Field | Type | Meaning |
|---|---|---|
| `id` | string | unique within its list. The profile's `unique_id` for `single`/`child`, `"group:" + app_id` for a `header`. Sent back with row events as a staleness check. |
| `kind` | `"single"` \| `"header"` \| `"child"` | an app (or a lone profile) on one row / the group row of an app with several profiles / one profile under a header (indented) |
| `app_id` | string | the app's `unique_app_id`, what `set_group_collapsed` sends |
| `name` | string | primary text. App name for `single`/`header`, profile name for `child`. |
| `detail` | string | secondary text, `""` = none. The profile name for a `single` row of an app with profiles, `"3 profiles"` / `"3 profiles · 1 hidden"` for a `header`. |
| `icon_path` | string | app icon for `single`/`header`, the profile's own picture for `child`. `""` = draw `letter` in a `letter_color` circle instead. |
| `letter`, `letter_color` | string | the fallback avatar |
| `hidden` | bool | hidden from the popup: dim the row, show "Hidden", eye shows crossed out. For a `header`: every profile of the group is hidden. |
| `is_default` | bool | this profile is the "Everything else" target: show "Default profile" |
| `collapsed` | bool | `header` only: the group is collapsed (chevron points right). Always false while filtering. |
| `draggable` | bool | can be dragged (false for every row while filtering, and for every special row) |
| `menu` | array of `{action, title}` | the items of the row's "···" menu, in order. `action` is `"move_to_top"`, `"move_to_bottom"` or `"toggle_default"`. Empty = no "···" button. |

Which menu items Rust puts in:

- `single`/`child`, draggable: Move to top, Move to bottom, toggle default.
- `header`, draggable: Move to top, Move to bottom.
- not draggable (filtering, special rows): just toggle default for `single`/`child`, nothing for `header`.
- The toggle default title is `"Stop opening unmatched links here"` when `is_default`, `"Open unmatched links here"` otherwise.
- For a `child`, "top"/"bottom" mean within its group.

### `rules`

Built by today's `refresh_rules`.

| Field | Type | Meaning |
|---|---|---|
| `rules` | array of Rule | the saved rules, checked top to bottom. A rule's index in this array is its identity in events. Its number on screen is index + 1. |
| `source_options` | array of `{id, label}` | the "links from" choices, in menu order: `""` "Any app" first, then every found app, then common link senders (Mail, Messages, ...), then any source only a saved rule knows about. `id` is a bundle id. |
| `target_options` | array of `{id, label, icon_path}` | the "open in" choices: `""` "Show the picker" first, then every profile (hidden ones too, labeled `"... (hidden)"`), labels like `"Work · Chrome"` or `"Safari"`. `icon_path` is the profile picture, else the app icon. |
| `draft` | Draft \| null | the new rule being put together, `null` = none |
| `test` | Test | the "Test a link" card |
| `fallback` | Fallback | the "Everything else" card |

#### Rule

| Field | Type | Meaning |
|---|---|---|
| `pattern` | string | the saved URL pattern, `""` = any link (the rule only checks the source app) |
| `pattern_invalid` | bool | the user's latest text for this pattern isn't valid and wasn't saved: red border |
| `pattern_edit` | string | that unsaved text (only meaningful with `pattern_invalid`) |
| `source_id` | string | bundle id, `""` = any app |
| `source_label` | string | the source app's name, `""` with any app. A chip `"from <source_label>"` shows only when it isn't empty. |
| `target_id` | string | profile `unique_id`, `""` = show the picker. May be an id missing from `target_options` (the app was uninstalled). |
| `target_label` | string | what the target picker shows: a profile label, `"Show the picker"` or `"Missing app"` |
| `target_icon_path` | string | icon on the target picker |
| `private` | bool | opens in a private window |
| `supports_private` | bool | the target can open privately: show the Private toggle at all |
| `matched` | bool | it's the rule the test link hit: highlight it |

#### Draft

| Field | Type | Meaning |
|---|---|---|
| `pattern` | string | the pattern Rust last received |
| `pattern_placeholder` | string | `"any link"` when a source app is picked, `"e.g. notion.so"` otherwise |
| `pattern_invalid` | bool | red border on the field, help in red. Only set once the user is done typing (Return, focus loss, or a target picked). |
| `source_id`, `source_label` | string | like a rule's. With `source_label == ""` the source chip reads `"+ from app"` in placeholder style. |
| `target_id` | string \| null | `null` = not picked yet, `""` = show the picker, else a profile id |
| `target_label` | string | `""` while not picked: the picker reads `"Choose…"` in placeholder style |
| `help` | string | the line under the draft, e.g. `"Matches every page under github.com/acme. Saved once it has a pattern and a target."` or the invalid-pattern message |

The draft has no save button: Rust saves it as a rule the moment it has a valid pattern (or a source app) and a target, then sends `draft: null` with the new rule at the end of `rules`.

#### Test

| Field | Type | Meaning |
|---|---|---|
| `url` | string | the test URL Rust last received |
| `source_id`, `source_label` | string | the "from" picker of the card (one of `source_options`) |
| `result_kind` | `"none"` \| `"rule"` \| `"fallback"` \| `"invalid"` | nothing typed yet / a rule matches / no rule matches / not a link. The dot before the result is accent / secondary / red. |
| `result_text` | string | e.g. `"Rule 2 matches → opens in Work · Chrome (private)"`, `"No rule matches → shows the picker"`, `"That isn't a link yet"`. `""` with `"none"`. |

#### Fallback

The "Everything else" card, i.e. the saved `default_opener`.

| Field | Type | Meaning |
|---|---|---|
| `target_id`, `target_label`, `target_icon_path` | string | like a rule's (`""` = show the picker) |
| `private`, `supports_private` | bool | like a rule's |
| `matched` | bool | the test link falls through to here (`result_kind == "fallback"`): highlight the card |

### `general`

| Field | Type | Meaning |
|---|---|---|
| `theme` | `"system"` \| `"light"` \| `"dark"` | the Theme picker, labeled Auto / Light / Dark |
| `native_popup_available` | bool | show the "Native popup" row at all (`native_popup::is_supported()`) |
| `native_popup` | bool | |
| `show_hotkeys` | bool | |
| `quit_on_lost_focus_visible` | bool | show the "Close when clicking elsewhere" row at all (macOS and not `run_in_background`: in the background the popup always hides on a click elsewhere) |
| `quit_on_lost_focus` | bool | |
| `unwrap_urls` | bool | |
| `run_in_background` | bool | |

## About state (Rust -> Swift)

Passed to `browsers_about_show` and `browsers_about_update`: the `about` object of the Settings state plus `appearance`.

```json
{
  "appearance": "dark",
  "app_icon_path": "/Applications/Browsers.app/Contents/Resources/icons/512x512/software.Browsers.png",
  "app_name": "Browsers",
  "version_text": "Version 0.6.0",
  "copyright": "© 2022-2024 Browsers.software team.",
  "website_url": "https://browsers.software",
  "directories": [{ "label": "Config", "path": "/Users/me/Library/Application Support/software.Browsers/" }],
  "made_with_slint": { "label": "Made with Slint", "version_text": "Version 1.18.1", "url": "https://slint.dev" }
}
```

| Field | Type | Meaning |
|---|---|---|
| `appearance` | string | as in the Settings state |
| `app_icon_path` | string | `paths::get_app_icon_path()` |
| `app_name` | string | `"Browsers"` |
| `version_text` | string | `"Version " + CARGO_PKG_VERSION` |
| `copyright` | string | |
| `website_url` | string | shown as `"Visit us at <link>."`, Swift opens it with `NSWorkspace` |
| `directories` | array of `{label, path}` | Config, Cache, Logs, Resources (`about_window::directories()`), paths end in `/`. Shown as label/path pairs, paths selectable. |
| `made_with_slint` | `{label, version_text, url}` | required attribution for Slint's royalty-free license while the app links Slint (the Slint popup is still the fallback), the native stand-in for the `AboutSlint` widget. Never drop it. Clicking it opens `url`. |

Swift opens links itself, there are no events for them.

## Events (Swift -> Rust)

One JSON object per user action, `type` first.
"Rust handling" names today's code in `src/gui/settings_window.rs` that the event maps onto.
Every event that changes something is answered with a fresh state (`browsers_settings_update`), either right away (window-local state, rules, settings) or once the background thread is done (hide, restore, move, rescan arrive through `UiHandle::profiles_received`).

### Windows

```json
{ "type": "settings_closed" }
{ "type": "about_closed" }
{ "type": "set_default_browser" }
```

| Event | When | Rust handling |
|---|---|---|
| `settings_closed` | the user closed Settings (close button, Cmd+W) | today's `clear_on_close` closure: forget the window, then `on_last_window_closed` if nothing is visible, else `refocus_popup` on the next turn of the event loop. Not sent for `browsers_settings_close`. |
| `about_closed` | the user closed About | same, for About |
| `set_default_browser` | "Make default" in the sidebar card | `on_set_default_browser`: `MessageToMain::SetBrowsersAsDefaultBrowser` + `poll_default_browser`, which now sends an update with the new `is_default_browser` instead of setting a Slint property |

### Browsers page

```json
{ "type": "browsers_filter_changed", "text": "chr" }
{ "type": "set_show_hidden", "show": true }
{ "type": "rescan" }
{ "type": "hide_non_browsers" }
{ "type": "dismiss_non_browser_banner" }
{ "type": "toggle_row_visibility", "list": "rows", "row": 2, "row_id": "/Applications/Google Chrome.app#Profile 1" }
{ "type": "set_group_collapsed", "app_id": "/Applications/Google Chrome.app", "collapsed": true }
{ "type": "row_menu", "list": "rows", "row": 2, "row_id": "/Applications/Google Chrome.app#Profile 1", "action": "move_to_top" }
{ "type": "move_browser_row", "row": 1, "row_id": "group:/Applications/Google Chrome.app", "to": 0 }
```

`list` is `"rows"` or `"special_rows"`, `row` the index into that array of the last state.
Rust looks the row up by index and drops the event when that row's `id` isn't `row_id` (the list changed in between).

| Event | When | Rust handling |
|---|---|---|
| `browsers_filter_changed` | every edit of the filter field | `on_filter_changed`: `Local.filter = text`, refresh |
| `set_show_hidden` | the "Show hidden" toggle button | `on_show_hidden_toggled`, but setting `Local.show_hidden = show`, refresh |
| `rescan` | "Rescan" | `on_refresh`: `MessageToMain::Refresh` |
| `hide_non_browsers` | "Hide them" in the banner | `hide_non_browsers` (hides every special app, then dismisses the banner for good) |
| `dismiss_non_browser_banner` | "Not now" in the banner | `dismiss_non_browser_banner` |
| `toggle_row_visibility` | a row's eye button | `on_browser_visibility_toggled` / `on_special_visibility_toggled`: `toggle_visibility(meta)` with the meta from `Local.rows` / `Local.special_rows`: restores every id of a hidden row, hides all profiles for a header, hides the one profile otherwise |
| `set_group_collapsed` | a header's chevron | `on_browser_group_toggled`, but setting rather than flipping: insert into / remove from `Local.collapsed`, refresh |
| `row_menu` | a "···" menu item | `"rows"`: `on_browser_menu_picked` = `row_menu_picked(row, action)` with `move_to_top` = 0, `move_to_bottom` = 1, `toggle_default` = 2. `"special_rows"`: `on_special_menu_picked` = `toggle_default_profile(meta.ids[0])`. |
| `move_browser_row` | a drop in the `rows` list | `on_snap_browser_drop` + `on_browser_rows_dropped`, see "Reordering the Browsers list" |

### Rules page

```json
{ "type": "add_rule" }
{ "type": "rule_pattern_edited", "index": 0, "pattern": "github.com/acme/**" }
{ "type": "rule_source_picked", "index": 0, "source_id": "com.tinyspeck.slackmacgap" }
{ "type": "rule_target_picked", "index": 0, "target_id": "/Applications/Firefox.app#Home" }
{ "type": "rule_private_set", "index": 0, "private": true }
{ "type": "rule_duplicate", "index": 0 }
{ "type": "rule_delete", "index": 0 }
{ "type": "move_rule", "from": 2, "to": 0 }
{ "type": "draft_pattern_edited", "text": "notion.s" }
{ "type": "draft_pattern_committed" }
{ "type": "draft_source_picked", "source_id": "com.apple.mail" }
{ "type": "draft_target_picked", "target_id": "" }
{ "type": "draft_discarded" }
{ "type": "test_url_edited", "text": "https://github.com/acme/api" }
{ "type": "test_source_picked", "source_id": "" }
{ "type": "fallback_target_picked", "target_id": "/Applications/Safari.app" }
{ "type": "fallback_private_set", "private": false }
```

`index` is the rule's index in `rules.rules` of the last state.
`source_id`/`target_id` are an `id` from `source_options`/`target_options` (`""` = any app / show the picker), Rust ignores ids that aren't in the options it last sent.
The private events carry the new value rather than "toggle", so a double click can't undo itself.

| Event | When | Rust handling |
|---|---|---|
| `add_rule` | "+ New rule" | `on_add_rule`: start a draft (keeps an existing one) |
| `rule_pattern_edited` | every edit of a rule's pattern | `on_rule_pattern_edited`: valid (`has_valid_matcher`) = save it and forget the rule's invalid edit, invalid = don't save, remember the text as that rule's invalid edit (new `Local` map, index -> text) so the state can carry `pattern_invalid`/`pattern_edit`. The map is cleared whenever the rule list changes shape (add, duplicate, delete, move, draft saved) and on open. |
| `rule_source_picked` | the rule's "from" chip, or "Only links from" in its "···" menu | `on_rule_source_picked` |
| `rule_target_picked` | the rule's target picker | `on_rule_target_picked` (`retarget` keeps the private flag) |
| `rule_private_set` | the rule's Private toggle | `on_rule_private_toggled`, but setting `opener.incognito = private`, save |
| `rule_duplicate` | "Duplicate" in the "···" menu | `on_rule_menu_picked(index, 0)`: the copy goes right below |
| `rule_delete` | "Delete" in the "···" menu | `on_rule_menu_picked(index, 1)` |
| `move_rule` | a drop in the rules list | `on_rules_dropped`: `move_item(rules, from, to)`, `to` being the insertion point in the original list (0..=count, same as SwiftUI's `onMove` destination). `to == from` or `from + 1` is a no-op. |
| `draft_pattern_edited` | every edit of the draft's field | `on_draft_pattern_edited` |
| `draft_pattern_committed` | Return in the draft's field, or it losing focus | `on_draft_pattern_committed`: from now on an invalid pattern shows as invalid |
| `draft_source_picked` | the draft's source chip | `on_draft_source_picked` |
| `draft_target_picked` | the draft's target picker | `on_draft_target_picked` |
| `draft_discarded` | the draft's discard (x) button | `on_draft_discarded` |
| `test_url_edited` | every edit of the test field | `on_test_url_edited` |
| `test_source_picked` | the test card's "from" picker | `on_test_source_picked` (`Local.test_source` becomes the position of `source_id` in the sources) |
| `fallback_target_picked` | the "Everything else" picker | `on_fallback_target_picked` |
| `fallback_private_set` | the "Everything else" Private toggle | `on_fallback_private_toggled`, but setting `default_opener.incognito = private`, `SaveConfigDefaultOpener` |

### General page

```json
{ "type": "set_theme", "theme": "dark" }
{ "type": "set_native_popup", "value": false }
{ "type": "set_show_hotkeys", "value": true }
{ "type": "set_quit_on_lost_focus", "value": true }
{ "type": "set_unwrap_urls", "value": true }
{ "type": "set_run_in_background", "value": false }
```

| Event | Rust handling |
|---|---|
| `set_theme` | `on_theme_changed`: save, `app::apply_theme`, which now also sends the new `appearance` to the native popup, Settings and About |
| `set_native_popup` | `on_native_popup_changed` (hides a visible popup, the next link uses the other one) |
| `set_show_hotkeys` | `on_show_hotkeys_changed` |
| `set_quit_on_lost_focus` | `on_quit_on_lost_focus_changed` |
| `set_unwrap_urls` | `on_unwrap_urls_changed` |
| `set_run_in_background` | `on_run_in_background_changed` (the answer can show or hide "Close when clicking elsewhere") |

## Window lifecycle

- `browsers_settings_show` on a closed window: Rust resets its window-local state first (`Local::default()`: filter, show hidden, collapsed groups, draft, test link), builds the state, then calls show.
  Swift resets its own per-open state (text fields, scroll positions), selects `page` (default `"browsers"`), activates the app (`NSApp.unhideWithoutActivation()`, then `NSApp.activate(ignoringOtherApps: true)`, like `macos_native::activate_app`) and makes the window key.
- On an open window it's "bring to front": new state, `page` only if given, nothing reset.
- Rust opens Settings with a page for:
  - `--settings` and the popup's `settings` event ("Manage Browsers…" and "Settings…" in the popup's menus): `NULL` (Browsers on a fresh window, the current page on an open one).
  - Debug builds' `BROWSERS_DEBUG_SETTINGS_PAGE` (0-3): the page with that index (browsers, rules, general, about).
- The user closing the window: Swift orders it out, so `browsers_settings_is_open()` is false, and then emits `settings_closed`.
  Rust runs the close logic of today's `app::clear_on_close` for it.
  `browsers_settings_close()` closes without the event.
  Same for About.
- While the native Settings/About is open, Rust treats it exactly like today's `settings_window`/`about_window` being `Some`: `AppState::quit_on_lost_focus_applies`, `nothing_visible`, and the `settings_window.is_some()` check in `always_open_here` all have to see it.
- Every place that calls `settings_window::refresh` today (`profiles_received`, `always_open_here`, the handlers) sends a fresh state with `browsers_settings_update` instead.
  `app::apply_theme` also sends `browsers_about_update`.
- Both windows float (`NSWindow.Level.floating`, collection behavior `.fullScreenAuxiliary`), like the Slint ones do through `app::make_window_floating`: the popup floats too, and a normal-level window can never get in front of it.
- The app is an accessory app without an Edit or File menu, so the windows handle Cmd+W (close, emits the closed event) and the editing shortcuts in text fields (Cmd+X/C/V/A/Z, Shift+Cmd+Z) themselves, e.g. in `performKeyEquivalent` by sending the standard actions down the responder chain.
- Settings remembers its frame across opens (`setFrameAutosaveName`), centered the first time.
  About is centered every time.

## Pages

Fixed copy in quotes, hardcode it in Swift.

### Sidebar

- Pages, each with an SF Symbol:
  - "Browsers": `globe`
  - "Rules": `arrow.triangle.branch`, with the rule count as a badge when there are rules
  - "General": `gearshape`
  - "About": `info.circle`
- Default-browser card pinned to the sidebar's bottom.
  With `is_default_browser`: a green status dot, "Default browser", "Links from every app come here.".
  Without: an orange dot, "Not the default browser", "Links skip Browsers for now.", and a prominent "Make default" button (`set_default_browser`).

### Browsers

- Toolbar (or page header) on the right:
  - A search field, placeholder "Filter" (`browsers_filter_changed`).
  - A "Show hidden" toggle button, on = `show_hidden`, with `hidden_count` as a badge when > 0 (`set_show_hidden`).
  - "Rescan" with `arrow.clockwise` (`rescan`).
- Banner when `banner` isn't null: `banner.title` (bold), `banner.message`, buttons "Hide them" (prominent, `hide_non_browsers`) and "Not now" (plain, `dismiss_non_browser_banner`).
- Section "Popup order" with `order_hint` as trailing caption, then the `rows` list.
- Each row, left to right:
  - The drag affordance (when `draggable`).
  - For a header, a disclosure chevron (right when `collapsed`, down otherwise; `set_group_collapsed` with the opposite value).
  - The icon or the letter avatar (children indented and slightly smaller).
  - `name` (medium weight for headers) over `detail`.
  - Then trailing: "Default profile" when `is_default`, "Hidden" when `hidden`.
  - The eye button: `eye` / `eye.slash` when `hidden`, help/accessibility label "Show in the popup" / "Hide from the popup", sends `toggle_row_visibility`.
  - The "···" menu (`ellipsis`) with `menu`'s items, sends `row_menu`.
  - A hidden row is dimmed (secondary label color, icon at reduced opacity).
- Section "Only for their own links · Always listed first" with the `special_rows` (same row view, no drag affordance, no chevron), only when there are any.
- `empty_text` in place of both lists when both are empty.
- Footer:
  - Left, a disabled "+ Add an app…" link-style button with the tooltip "Coming soon - apps that can open links are found automatically".
  - Right, `found_text` as a caption.

### Rules

- Toolbar/header: "+ New rule", prominent (`add_rule`).
- "Test a link" card:
  - A text field, placeholder "Paste a link, like https://github.com/acme/api" (`test_url_edited`).
  - "from" and a source picker showing `test.source_label` (`test_source_picked`).
  - Below, either "See which rule a link would use before you click it." (secondary, with `"none"`) or the result dot plus `test.result_text`.
- Section "Checked top to bottom · First match wins", then the rules list.
  Each rule, left to right:
  - The drag affordance.
  - The number (index + 1, accent and semibold when `matched`).
  - The pattern as a monospaced text field with placeholder "any link" (`rule_pattern_edited`, red border with `pattern_invalid`).
  - The `"from <source_label>"` chip when `source_label` isn't empty (a small menu over `source_options`, `rule_source_picked`).
  - An arrow (`arrow.right`).
  - The target picker (`target_label` and `target_icon_path`, a menu over `target_options`, `rule_target_picked`).
  - A "Private" toggle when `supports_private` (`rule_private_set`).
  - The "···" menu: a submenu "Only links from" over `source_options` (`rule_source_picked`), a divider, "Duplicate" (`rule_duplicate`), "Delete" (`rule_delete`, destructive role).
  - A `matched` rule is highlighted (accent-tinted background and border).
- The draft, when `draft` isn't null, after the rules and visually set apart (dashed or accent outline):
  - Its number (rule count + 1).
  - The pattern field (monospaced, `pattern_placeholder`, focused when the draft appears, red with `pattern_invalid`; `draft_pattern_edited`, `draft_pattern_committed`).
  - The source chip (`source_label` or "+ from app" as a placeholder; `draft_source_picked`).
  - The arrow, the target picker (`target_label` or "Choose…" as a placeholder; `draft_target_picked`).
  - A discard button (`xmark`, accessibility label "Discard new rule"; `draft_discarded`).
  - Below, `help` (red with `pattern_invalid`).
- "Everything else" card after that:
  - "Everything else" over "When no rule matches".
  - The arrow, the target picker (`fallback_target_picked`), a "Private" toggle when `supports_private` (`fallback_private_set`).
  - Highlighted when `matched`.

The pickers are menus whose label is the precomputed `*_label` (a SwiftUI `Menu` with a picker look, or a `Picker` with a custom label), because a rule's target can be an id that isn't among the options ("Missing app").

### General

Grouped form, three sections:

- "Appearance":
  - "Theme": segmented Auto / Light / Dark (`set_theme`).
- "Popup":
  - "Native popup": "Use the macOS look for the link picker." Only with `native_popup_available`. `set_native_popup`.
  - "Show hotkey numbers": "Press 1–9 in the popup to open the matching app." `set_show_hotkeys`.
  - "Close when clicking elsewhere": "The popup goes away as soon as another window takes focus." Only with `quit_on_lost_focus_visible`. `set_quit_on_lost_focus`.
  - "Unwrap tracking links": "Opens the real destination of Outlook Safe Links and Messenger links." `set_unwrap_urls`.
- "Performance":
  - "Keep running in the background": "Opens links instantly. Uses a little memory while idle." `set_run_in_background`.

Each toggle shows the state's value and sends the new value.
It never flips locally on its own, the update that answers the event does.

### About (Settings page)

From `about`:

- App icon (64 pt), `app_name` (bold), `version_text` (selectable), `copyright`.
- "Visit us at <website_url>." with the URL as a link.
- Section "Folders": the `directories` as label/path rows, paths selectable.
- The `made_with_slint` attribution: label and `version_text`, the whole thing a link to `url`.

### About window

The same content in a small standard About panel: icon (64 pt) centered, `app_name` (bold, selectable), `version_text` and `copyright` (secondary, selectable), the website link, the folders in a small font, the Slint attribution at the bottom.

## Reordering the Browsers list

What may move where (enforced by Rust, see `snap_drop_index` and `drop_target`, keep those tests passing):

- A `single` row or a whole group (dragging its `header`) lands only between other apps/groups, never between another app's profiles.
  Dropping it inside a group snaps to the nearest group edge.
  A group always moves with all its profiles, hidden ones included.
- A `child` stays inside its own group.
  Dropping it at its group's end puts it after the group's last profile, even a hidden one.
- `special_rows` never move (they're always first in the popup), and nothing moves while filtering (`reorder_enabled` false, every row's `draggable` false).
  `move_app_profiles_before` in `src/lib.rs` also turns "before a special row" into "before the first orderable profile".

The event:

```json
{ "type": "move_browser_row", "row": 4, "row_id": "/Applications/Firefox.app", "to": 1 }
```

`row` is the dragged row's index in `browsers.rows`.
`to` is the insertion point in the original, unmodified `rows` (0..=count), exactly what SwiftUI's `onMove(perform:)` hands over as its destination.
For a header that's the header's own index, Swift doesn't add its children.

Rust handling, all existing code:

1. Drop the event if `rows[row].id != row_id`.
2. Compute `to = snap_drop_index(kinds, row, to)`.
3. Call `move_rows(state, row, to)`: `drop_target` turns it into "these ids before this id" (or a no-op) and sends `MessageToMain::MoveAppProfilesBefore(ids, before)`, which saves the new order and comes back through `profiles_received` as a fresh state.

Swift:

- Single-row drags only (no multi-selection).
- `moveDisabled` for rows that aren't `draggable` and for the whole special section.
- It doesn't reorder its rows itself, the answering update does.
- A native `List` can't restrict where the insertion line goes, that's fine: Rust snaps the drop.
- A custom drop indicator may mirror the snapping (a child: clamp between its header + 1 and the group's end; anything else: the nearest index that isn't between two children), but it's cosmetic.

Rules reorder the same way with `move_rule` (`from`, `to` as `onMove` gives them), no snapping, every rule can move anywhere.

## Validation: who checks what

All of it stays in Rust, Swift never parses patterns or URLs:

- A rule pattern's validity (`UISettingsRule::has_valid_matcher` / `url_rule::is_valid_pattern`): Rust, on every `rule_pattern_edited`, answered with `pattern_invalid`.
- Whether the draft is complete (valid pattern or source app, plus a target) and saving it: Rust, `Draft::is_complete`.
- When the draft shows as invalid (only after commit or a target pick): Rust, `Draft.show_invalid`.
- The test link's outcome: Rust, `crate::test_opening_rules`.
- Whether the Private toggle applies to a target: Rust, `supports_private`.
- Where a dragged row may land: Rust, `snap_drop_index`/`drop_target`.
- What's in a row's menu, what's draggable, what's hidden or collapsed: Rust.

## Native look

Target design for the Swift side: it should feel like macOS System Settings, not like a port of the Slint window.

- Settings window:
  - A standard titled `NSWindow` (`.titled, .closable, .resizable, .fullSizeContentView`, no `.miniaturizable`) hosting SwiftUI, transparent titlebar and unified toolbar.
  - About 780x552, min size about 680x460.
- `NavigationSplitView` with a fixed-width sidebar (about 200 pt) using the system sidebar material (`.listStyle(.sidebar)`).
  The sidebar can't be collapsed away.
  The page title shows in the toolbar like System Settings.
- SF Symbols everywhere (page icons, eye, chevron, ellipsis, arrow, xmark, arrow.clockwise), sized by the system.
  No hand-drawn paths.
- General and About: `Form { Section { ... } }.formStyle(.grouped)` with section headers, rows with a title and a secondary helper line under it, native `Toggle` switches (`.toggleStyle(.switch)`), a segmented `Picker` for the theme.
- Browsers and Rules:
  - The same grouped look, lists built with `List` + `ForEach` + `.onMove` so reordering uses native drag and drop.
  - Row height around 40 pt, app icons 24-26 pt, profile icons 20-22 pt.
  - Letter avatars are circles in `letter_color` with a white semibold letter.
  - Trailing buttons are borderless icon buttons, menus are `Menu` with `.menuStyle(.borderlessButton)` and the menu indicator hidden for "···".
- Pickers are menu-style (`.pickerStyle(.menu)` look), source chips small and rounded, the Private toggle a small `.checkbox` or `.button`-style toggle.
- Text fields: `.roundedBorder`, the filter a search field (`.searchable` in the toolbar, or an `NSSearchField`-looking `TextField`), patterns `.monospaced()`.
  An invalid field gets a red border (`Color.red` / `.red` system color) and red help text.
- Colors: system only.
  `Color.accentColor`, `.primary`/`.secondary`/`.tertiary` labels, `Color(nsColor: .controlBackgroundColor)`, `.separatorColor`, `.green`/`.orange`/`.red` system colors for status.
  No hex palettes (the avatar `letter_color` is data, the only exception), standard corner radii and spacing, system fonts at system sizes (`.body`, `.callout`, `.caption`).
- Highlights (a matched rule, the matched "Everything else"): accent at low opacity as background, accent border.
- Banner: an accent-tinted rounded box with the info symbol, like a System Settings notice.
- Default-browser card: a small rounded `.regularMaterial`/grouped box at the sidebar's bottom.
- Dark/light: follow `appearance` via `NSWindow.appearance`, everything else adapts by itself.
- About window:
  - Small, non-resizable, titled with an empty title, close button only.
  - About 300 wide, height fitting the content.
  - Centered, like the standard macOS About panel: app icon, bold name, version, copyright, website link, the folders in a small selectable font, the Slint attribution.

## Changes from the Slint windows

Deliberate, so neither side has to guess:

- Pickers and toggles carry ids and values instead of option indexes and "toggled" (`source_id`/`target_id`, `private`, `value`), and the row menus carry an `action` name instead of an index.
  The Rust handlers resolve these into the same existing calls.
- A header row's menu while filtering is empty (Slint offered "Open unmatched links here" there, which silently targeted the group's first profile).
- A rule's pattern is always an editable field with an "any link" placeholder.
  Slint showed a fixed "any link" text for a rule without a pattern, so it could never get one.
- Rejected rule-pattern edits come back in the state (`pattern_invalid`, `pattern_edit`) instead of a synchronous callback return value.
- Letter avatars use the popup's colors (`main_window::avatar_color_hex` of the row's id), so a profile looks the same in both places.
  Slint Settings had its own name-hashed palette.
- The drag preview ("· moves together" on a lifted group) and the debug-only `BROWSERS_DEBUG_HOLD_DRAG` aren't carried over, native drag and drop draws its own preview.
- The "Made with Slint" badge is text plus a link instead of Slint's logo image.
