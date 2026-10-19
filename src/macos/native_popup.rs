// the native (SwiftUI) link picker in native/macos/BrowsersPopup/, which build.rs compiles and
// links in.
// The Rust side owns the data and every action, the Swift side only draws and reports what the
// user did, see CONTRACT.md there for the JSON shapes.
//
// Everything here runs on the main thread only.
// Before init (or when the native popup isn't available) every call is a no-op, so nothing ever
// reaches Swift on a macOS version that can't run it.

use std::cell::Cell;
use std::ffi::{CStr, CString, c_char};
use std::sync::OnceLock;

use objc2_foundation::NSProcessInfo;
use serde::{Deserialize, Serialize};

use crate::gui::app::AppState;
use crate::gui::app_state::UIBrowser;
use crate::gui::main_window;
use crate::utils::ConfiguredTheme;

// see browsers_popup.h
unsafe extern "C" {
    fn browsers_popup_init(on_event: extern "C" fn(event_json: *const c_char));
    fn browsers_popup_show(state_json: *const c_char);
    fn browsers_popup_update(state_json: *const c_char);
    fn browsers_popup_focus();
    fn browsers_popup_hide();
    fn browsers_popup_is_visible() -> bool;
}

// one user action in the popup (CONTRACT.md "Events")
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PopupEvent {
    Open {
        profile_index: i64,
        #[serde(default)]
        private: bool,
    },
    CopyLink,
    AlwaysOpenHere {
        unique_id: String,
        profile_index: i64,
    },
    HideProfile {
        unique_id: String,
    },
    HideApp {
        unique_app_id: String,
    },
    RestoreProfile {
        unique_id: String,
    },
    Refresh,
    SetDefault,
    Settings,
    About,
    Quit,
    Dismiss,
}

pub fn parse_event(json: &str) -> Result<PopupEvent, serde_json::Error> {
    serde_json::from_str(json)
}

// what the popup shows (CONTRACT.md "State")
#[derive(Serialize, Debug)]
pub struct PopupState<'a> {
    url: &'a str,
    url_domain: String,
    url_path: String,
    source_app_name: &'a str,
    show_hotkeys: bool,
    show_set_as_default: bool,
    appearance: &'static str,
    browsers: Vec<PopupBrowser<'a>>,
    restorable: Vec<PopupRestorable<'a>>,
}

#[derive(Serialize, Debug)]
struct PopupBrowser<'a> {
    profile_index: usize,
    unique_id: &'a str,
    unique_app_id: &'a str,
    browser_name: &'a str,
    profile_name: &'a str,
    full_name: String,
    supports_profiles: bool,
    supports_incognito: bool,
    icon_path: &'a str,
    profile_icon_path: &'a str,
    profile_initial: String,
    avatar_color: String,
}

#[derive(Serialize, Debug)]
struct PopupRestorable<'a> {
    unique_id: &'a str,
    full_name: String,
}

impl<'a> PopupState<'a> {
    // `browsers` is the list as shown (already narrowed down by the link's opening rules)
    pub fn new(
        url: &'a str,
        source_app_name: &'a str,
        show_hotkeys: bool,
        show_set_as_default: bool,
        theme: ConfiguredTheme,
        browsers: &'a [UIBrowser],
        restorable: &'a [UIBrowser],
    ) -> Self {
        let (url_domain, url_path) = main_window::split_url_for_header(url);
        PopupState {
            url,
            url_domain,
            url_path,
            source_app_name,
            show_hotkeys,
            show_set_as_default,
            appearance: match theme {
                ConfiguredTheme::Auto => "system",
                ConfiguredTheme::Light => "light",
                ConfiguredTheme::Dark => "dark",
            },
            browsers: browsers
                .iter()
                .map(|b| PopupBrowser {
                    profile_index: b.browser_profile_index,
                    unique_id: &b.unique_id,
                    unique_app_id: &b.unique_app_id,
                    browser_name: &b.browser_name,
                    profile_name: &b.profile_name,
                    full_name: b.get_full_name(),
                    supports_profiles: b.supports_profiles,
                    supports_incognito: b.supports_incognito,
                    icon_path: &b.icon_path,
                    profile_icon_path: &b.profile_icon_path,
                    profile_initial: main_window::profile_initial(&b.profile_name),
                    avatar_color: main_window::avatar_color_hex(&b.unique_id),
                })
                .collect(),
            restorable: restorable
                .iter()
                .map(|b| PopupRestorable {
                    unique_id: &b.unique_id,
                    full_name: b.get_full_name(),
                })
                .collect(),
        }
    }

    pub fn from_app_state(st: &'a AppState) -> Self {
        PopupState::new(
            &st.url,
            &st.source_app_name,
            st.ui_settings.visual_settings.show_hotkeys,
            st.show_set_as_default,
            st.ui_settings.visual_settings.theme,
            &st.filtered_browsers,
            &st.restorable_app_profiles,
        )
    }

    pub fn to_json(&self) -> CString {
        let json = serde_json::to_string(self).expect("popup state is always serializable");
        // serde_json escapes control characters, so there's never a NUL inside a string
        CString::new(json).expect("JSON never contains a NUL byte")
    }
}

// SwiftUI as used by the popup needs macOS 13
pub fn is_supported() -> bool {
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| {
        NSProcessInfo::processInfo()
            .operatingSystemVersion()
            .majorVersion
            >= 13
    })
}

// why the Slint popup is used no matter what the setting says, if anything.
// SLINT_BACKEND=headless is the MCP-driven UI test mode (see CLAUDE.md), which can only see Slint
// windows.
pub fn unavailable_reason() -> Option<&'static str> {
    if std::env::var("SLINT_BACKEND").is_ok_and(|backend| backend == "headless") {
        Some("SLINT_BACKEND=headless")
    } else if std::env::var("BROWSERS_POPUP").is_ok_and(|popup| popup == "slint") {
        Some("BROWSERS_POPUP=slint")
    } else if !is_supported() {
        Some("needs macOS 13 or later")
    } else {
        None
    }
}

thread_local! {
    static ON_EVENT: Cell<Option<fn(PopupEvent)>> = const { Cell::new(None) };
}

// the handler is a fn pointer in a Cell rather than a boxed closure in a RefCell, so nothing is
// borrowed while it runs and it can call back into Swift (an event handler hiding the popup)
extern "C" fn on_event_json(event_json: *const c_char) {
    if event_json.is_null() {
        return;
    }
    // the pointer is only valid during this call, see browsers_popup.h
    let json = unsafe { CStr::from_ptr(event_json) }.to_string_lossy();
    let event = match parse_event(&json) {
        Ok(event) => event,
        Err(err) => {
            tracing::warn!("Ignoring an unknown event from the native popup {json}: {err}");
            return;
        }
    };
    tracing::debug!("Native popup event: {event:?}");
    let Some(handler) = ON_EVENT.with(Cell::get) else {
        return;
    };
    // unwinding into Swift is undefined behavior, so a panic stops here
    let result = std::panic::catch_unwind(move || handler(event));
    if result.is_err() {
        tracing::error!("Handling a native popup event panicked");
    }
}

// call once, before anything else here.
// Leaves the native popup off (every other call a no-op) when unavailable_reason says so, returns
// whether it's on
pub fn init(on_event: fn(PopupEvent)) -> bool {
    if is_initialized() {
        return true;
    }
    if let Some(reason) = unavailable_reason() {
        tracing::info!("Native popup unavailable ({reason}), using the Slint popup");
        return false;
    }
    ON_EVENT.with(|cell| cell.set(Some(on_event)));
    unsafe { browsers_popup_init(on_event_json) };
    true
}

pub fn is_initialized() -> bool {
    ON_EVENT.with(Cell::get).is_some()
}

// `state_json` comes from PopupState::to_json, built before the call so the caller doesn't hold
// on to AppState while Swift runs.
// The caller activates the app right before this, see browsers_popup.h
pub fn show(state_json: &CStr) {
    if is_initialized() {
        unsafe { browsers_popup_show(state_json.as_ptr()) };
    }
}

pub fn update(state_json: &CStr) {
    if is_initialized() {
        unsafe { browsers_popup_update(state_json.as_ptr()) };
    }
}

pub fn focus() {
    if is_initialized() {
        unsafe { browsers_popup_focus() };
    }
}

pub fn hide() {
    if is_initialized() {
        unsafe { browsers_popup_hide() };
    }
}

pub fn is_visible() -> bool {
    is_initialized() && unsafe { browsers_popup_is_visible() }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{Value, json};

    use super::*;

    fn browser(index: usize, name: &str, profile: &str, supports_profiles: bool) -> UIBrowser {
        UIBrowser {
            browser_profile_index: index,
            is_first: false,
            is_last: false,
            restricted_url_matchers: Arc::new(vec![]),
            browser_name: name.to_string(),
            profile_name: profile.to_string(),
            supports_profiles,
            supports_incognito: supports_profiles,
            icon_path: format!("/cache/icons/{name}.png"),
            profile_icon_path: String::new(),
            unique_id: format!("/Applications/{name}.app#{profile}"),
            unique_app_id: format!("/Applications/{name}.app"),
            bundle_id: String::new(),
            filtered_index: 0,
        }
    }

    fn state_value(state: &PopupState) -> Value {
        serde_json::from_str(state.to_json().to_str().unwrap()).unwrap()
    }

    fn keys(value: &Value) -> Vec<&str> {
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        keys.sort();
        keys
    }

    #[test]
    fn state_has_the_contract_shape() {
        let browsers = vec![
            browser(3, "Chrome", "Tallgrass", true),
            browser(0, "Safari", "", false),
        ];
        let restorable = vec![browser(5, "Firefox", "Work", true)];
        let state = PopupState::new(
            "https://github.com/slint-ui/slint/pull/123",
            "Slack",
            true,
            false,
            ConfiguredTheme::Auto,
            &browsers,
            &restorable,
        );
        let value = state_value(&state);

        assert_eq!(
            keys(&value),
            [
                "appearance",
                "browsers",
                "restorable",
                "show_hotkeys",
                "show_set_as_default",
                "source_app_name",
                "url",
                "url_domain",
                "url_path",
            ]
        );
        assert_eq!(value["url"], "https://github.com/slint-ui/slint/pull/123");
        assert_eq!(value["url_domain"], "github.com");
        assert_eq!(value["url_path"], "/slint-ui/slint/pull/123");
        assert_eq!(value["source_app_name"], "Slack");
        assert_eq!(value["show_hotkeys"], true);
        assert_eq!(value["show_set_as_default"], false);
        assert_eq!(value["appearance"], "system");

        let chrome = &value["browsers"][0];
        assert_eq!(
            keys(chrome),
            [
                "avatar_color",
                "browser_name",
                "full_name",
                "icon_path",
                "profile_icon_path",
                "profile_index",
                "profile_initial",
                "profile_name",
                "supports_incognito",
                "supports_profiles",
                "unique_app_id",
                "unique_id",
            ]
        );
        assert_eq!(
            *chrome,
            json!({
                "profile_index": 3,
                "unique_id": "/Applications/Chrome.app#Tallgrass",
                "unique_app_id": "/Applications/Chrome.app",
                "browser_name": "Chrome",
                "profile_name": "Tallgrass",
                "full_name": "Chrome (Tallgrass)",
                "supports_profiles": true,
                "supports_incognito": true,
                "icon_path": "/cache/icons/Chrome.png",
                "profile_icon_path": "",
                "profile_initial": "T",
                "avatar_color": main_window::avatar_color_hex("/Applications/Chrome.app#Tallgrass"),
            })
        );
        // display order is kept, and an app without profiles shows just its name
        assert_eq!(value["browsers"][1]["profile_index"], 0);
        assert_eq!(value["browsers"][1]["full_name"], "Safari");

        assert_eq!(
            value["restorable"],
            json!([{ "unique_id": "/Applications/Firefox.app#Work", "full_name": "Firefox (Work)" }])
        );
    }

    #[test]
    fn avatar_color_is_a_hex_color() {
        let browsers = vec![browser(0, "Chrome", "Work", true)];
        let state = PopupState::new("", "", false, true, ConfiguredTheme::Dark, &browsers, &[]);
        let value = state_value(&state);
        let color = value["browsers"][0]["avatar_color"].as_str().unwrap();
        assert_eq!(color.len(), 7);
        assert!(color.starts_with('#'));
        assert!(color[1..].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn state_without_a_link() {
        let state = PopupState::new("", "", false, true, ConfiguredTheme::Light, &[], &[]);
        let value = state_value(&state);
        assert_eq!(value["url_domain"], "");
        assert_eq!(value["url_path"], "");
        assert_eq!(value["appearance"], "light");
        assert_eq!(value["show_set_as_default"], true);
        assert_eq!(value["browsers"], json!([]));
        assert_eq!(value["restorable"], json!([]));
    }

    #[test]
    fn appearance_follows_the_theme() {
        let appearance = |theme| {
            state_value(&PopupState::new("", "", true, false, theme, &[], &[]))["appearance"]
                .clone()
        };
        assert_eq!(appearance(ConfiguredTheme::Auto), "system");
        assert_eq!(appearance(ConfiguredTheme::Light), "light");
        assert_eq!(appearance(ConfiguredTheme::Dark), "dark");
    }

    #[test]
    fn parses_every_event() {
        let cases = [
            (
                r#"{ "type": "open", "profile_index": 3, "private": false }"#,
                PopupEvent::Open {
                    profile_index: 3,
                    private: false,
                },
            ),
            (
                r#"{ "type": "open", "profile_index": 1, "private": true }"#,
                PopupEvent::Open {
                    profile_index: 1,
                    private: true,
                },
            ),
            (r#"{ "type": "copy_link" }"#, PopupEvent::CopyLink),
            (
                r#"{ "type": "always_open_here", "unique_id": "chrome#work", "profile_index": 3 }"#,
                PopupEvent::AlwaysOpenHere {
                    unique_id: "chrome#work".to_string(),
                    profile_index: 3,
                },
            ),
            (
                r#"{ "type": "hide_profile", "unique_id": "chrome#work" }"#,
                PopupEvent::HideProfile {
                    unique_id: "chrome#work".to_string(),
                },
            ),
            (
                r#"{ "type": "hide_app", "unique_app_id": "chrome" }"#,
                PopupEvent::HideApp {
                    unique_app_id: "chrome".to_string(),
                },
            ),
            (
                r#"{ "type": "restore_profile", "unique_id": "firefox#work" }"#,
                PopupEvent::RestoreProfile {
                    unique_id: "firefox#work".to_string(),
                },
            ),
            (r#"{ "type": "refresh" }"#, PopupEvent::Refresh),
            (r#"{ "type": "set_default" }"#, PopupEvent::SetDefault),
            (r#"{ "type": "settings" }"#, PopupEvent::Settings),
            (r#"{ "type": "about" }"#, PopupEvent::About),
            (r#"{ "type": "quit" }"#, PopupEvent::Quit),
            (r#"{ "type": "dismiss" }"#, PopupEvent::Dismiss),
        ];
        for (json, expected) in cases {
            assert_eq!(parse_event(json).unwrap(), expected, "{json}");
        }
    }

    #[test]
    fn open_without_private_is_a_regular_window() {
        assert_eq!(
            parse_event(r#"{ "type": "open", "profile_index": 2 }"#).unwrap(),
            PopupEvent::Open {
                profile_index: 2,
                private: false
            }
        );
    }

    #[test]
    fn rejects_unknown_or_incomplete_events() {
        assert!(parse_event(r#"{ "type": "explode" }"#).is_err());
        assert!(parse_event(r#"{ "type": "open" }"#).is_err());
        assert!(parse_event(r#"{ "type": "hide_app", "unique_id": "x" }"#).is_err());
        assert!(parse_event("not json").is_err());
    }
}
