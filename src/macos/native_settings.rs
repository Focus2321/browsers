// the native (SwiftUI) Settings and About windows in native/macos/BrowsersPopup/, linked in with
// the native popup.
// Rust owns the data and every action, Swift only draws and reports what the user did, see
// SETTINGS_CONTRACT.md there for the JSON shapes.
// The data comes from gui::settings_model, the same as the Slint Settings window's.
//
// Everything here runs on the main thread only.
// Before init (or when the native windows aren't available) every call is a no-op, and the Slint
// windows are used instead.

use std::cell::Cell;
use std::ffi::{CStr, CString, c_char};
use std::path::Path;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::gui::settings_model::{
    BrowserRow, BrowsersModel, GeneralModel, RowAction, RowKind, RowList, RulesModel,
    SettingsModel, TargetDisplay, TestResultKind,
};
use crate::gui::{about_window, main_window};
use crate::macos::native_popup;
use crate::paths;
use crate::utils::ConfiguredTheme;

// see browsers_settings.h
unsafe extern "C" {
    fn browsers_settings_init(on_event: extern "C" fn(event_json: *const c_char));
    fn browsers_settings_show(state_json: *const c_char, page: *const c_char);
    fn browsers_settings_update(state_json: *const c_char);
    fn browsers_settings_close();
    fn browsers_settings_is_open() -> bool;
    fn browsers_about_show(about_json: *const c_char);
    fn browsers_about_update(about_json: *const c_char);
    fn browsers_about_close();
    fn browsers_about_is_open() -> bool;
}

// ------------------------------------------------------------------------------------ Events

// the theme as the windows name it, same values as the popup's "appearance"
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    System,
    Light,
    Dark,
}

impl Appearance {
    pub fn theme(self) -> ConfiguredTheme {
        match self {
            Appearance::System => ConfiguredTheme::Auto,
            Appearance::Light => ConfiguredTheme::Light,
            Appearance::Dark => ConfiguredTheme::Dark,
        }
    }
}

// one user action in Settings or About (SETTINGS_CONTRACT.md "Events").
// `row`/`index` refer to the last state sent, see gui::settings_window::handle_native_event for
// how stale ones are caught
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SettingsEvent {
    SettingsClosed,
    AboutClosed,
    SetDefaultBrowser,

    BrowsersFilterChanged {
        text: String,
    },
    SetShowHidden {
        show: bool,
    },
    Rescan,
    HideNonBrowsers,
    DismissNonBrowserBanner,
    ToggleRowVisibility {
        list: RowList,
        row: usize,
        row_id: String,
    },
    SetGroupCollapsed {
        app_id: String,
        collapsed: bool,
    },
    RowMenu {
        list: RowList,
        row: usize,
        row_id: String,
        action: RowAction,
    },
    MoveBrowserRow {
        row: usize,
        row_id: String,
        to: usize,
    },

    AddRule,
    RulePatternEdited {
        index: usize,
        pattern: String,
    },
    RuleSourcePicked {
        index: usize,
        source_id: String,
    },
    RuleTargetPicked {
        index: usize,
        target_id: String,
    },
    RulePrivateSet {
        index: usize,
        private: bool,
    },
    RuleDuplicate {
        index: usize,
    },
    RuleDelete {
        index: usize,
    },
    MoveRule {
        from: usize,
        to: usize,
    },
    DraftPatternEdited {
        text: String,
    },
    DraftPatternCommitted,
    DraftSourcePicked {
        source_id: String,
    },
    DraftTargetPicked {
        target_id: String,
    },
    DraftDiscarded,
    TestUrlEdited {
        text: String,
    },
    TestSourcePicked {
        source_id: String,
    },
    FallbackTargetPicked {
        target_id: String,
    },
    FallbackPrivateSet {
        private: bool,
    },

    SetTheme {
        theme: Appearance,
    },
    SetNativePopup {
        value: bool,
    },
    SetShowHotkeys {
        value: bool,
    },
    SetQuitOnLostFocus {
        value: bool,
    },
    SetUnwrapUrls {
        value: bool,
    },
    SetRunInBackground {
        value: bool,
    },
}

pub fn parse_event(json: &str) -> Result<SettingsEvent, serde_json::Error> {
    serde_json::from_str(json)
}

// ------------------------------------------------------------------------------------ About

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Directory {
    label: String,
    path: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct MadeWithSlint {
    label: &'static str,
    version_text: String,
    url: &'static str,
}

// the About window's content, also the Settings' About page
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct AboutInfo {
    app_icon_path: String,
    app_name: &'static str,
    version_text: String,
    copyright: &'static str,
    website_url: &'static str,
    directories: Vec<Directory>,
    made_with_slint: MadeWithSlint,
}

impl AboutInfo {
    pub fn new(app_icon_path: String, directories: Vec<(String, String)>) -> Self {
        AboutInfo {
            app_icon_path,
            app_name: about_window::APP_NAME,
            version_text: about_window::version_text(),
            copyright: about_window::COPYRIGHT,
            website_url: about_window::WEBSITE_URL,
            directories: directories
                .into_iter()
                .map(|(label, path)| Directory { label, path })
                .collect(),
            made_with_slint: MadeWithSlint {
                label: "Made with Slint",
                // build.rs reads it from Cargo.lock, so it's the version actually linked in
                version_text: format!(
                    "Version {}",
                    option_env!("BROWSERS_SLINT_VERSION").unwrap_or("1")
                ),
                url: "https://slint.dev",
            },
        }
    }

    // none of it changes during a run
    pub fn current() -> &'static AboutInfo {
        static CURRENT: OnceLock<AboutInfo> = OnceLock::new();
        CURRENT.get_or_init(|| {
            AboutInfo::new(
                paths::get_app_icon_path().to_string_lossy().to_string(),
                about_window::directories(),
            )
        })
    }
}

#[derive(Serialize)]
struct AboutState<'a> {
    appearance: &'static str,
    #[serde(flatten)]
    about: &'a AboutInfo,
}

pub fn about_state_json(theme: ConfiguredTheme, about: &AboutInfo) -> CString {
    to_c_json(&AboutState {
        appearance: native_popup::appearance(theme),
        about,
    })
}

// ------------------------------------------------------------------------------------ Settings

#[derive(Serialize)]
struct SettingsState<'a> {
    appearance: &'static str,
    is_default_browser: bool,
    browsers: BrowsersState<'a>,
    rules: RulesState<'a>,
    general: GeneralState,
    about: &'a AboutInfo,
}

#[derive(Serialize)]
struct BrowsersState<'a> {
    filter: &'a str,
    show_hidden: bool,
    hidden_count: usize,
    reorder_enabled: bool,
    order_hint: &'static str,
    banner: Option<BannerState>,
    found_text: &'a str,
    empty_text: String,
    rows: Vec<RowState<'a>>,
    special_rows: Vec<RowState<'a>>,
}

#[derive(Serialize)]
struct BannerState {
    title: String,
    message: String,
}

#[derive(Serialize)]
struct RowState<'a> {
    id: &'a str,
    kind: RowKind,
    app_id: &'a str,
    name: &'a str,
    detail: &'a str,
    icon_path: &'a str,
    letter: &'a str,
    letter_color: String,
    hidden: bool,
    is_default: bool,
    collapsed: bool,
    draggable: bool,
    menu: Vec<MenuItemState>,
}

#[derive(Serialize)]
struct MenuItemState {
    action: RowAction,
    title: &'static str,
}

#[derive(Serialize)]
struct RulesState<'a> {
    rules: Vec<RuleState<'a>>,
    source_options: Vec<SourceOptionState<'a>>,
    target_options: Vec<TargetOptionState<'a>>,
    draft: Option<DraftState<'a>>,
    test: TestState<'a>,
    fallback: FallbackState<'a>,
}

#[derive(Serialize)]
struct RuleState<'a> {
    pattern: &'a str,
    pattern_invalid: bool,
    pattern_edit: &'a str,
    source_id: &'a str,
    source_label: &'a str,
    target_id: &'a str,
    target_label: &'a str,
    target_icon_path: &'a str,
    private: bool,
    supports_private: bool,
    matched: bool,
}

#[derive(Serialize)]
struct SourceOptionState<'a> {
    id: &'a str,
    label: &'a str,
}

#[derive(Serialize)]
struct TargetOptionState<'a> {
    id: &'a str,
    label: &'a str,
    icon_path: &'a str,
}

#[derive(Serialize)]
struct DraftState<'a> {
    pattern: &'a str,
    pattern_placeholder: &'static str,
    pattern_invalid: bool,
    source_id: &'a str,
    source_label: &'a str,
    target_id: Option<&'a str>,
    target_label: &'a str,
    help: &'a str,
}

#[derive(Serialize)]
struct TestState<'a> {
    url: &'a str,
    source_id: &'a str,
    source_label: &'a str,
    result_kind: TestResultKind,
    result_text: &'a str,
}

#[derive(Serialize)]
struct FallbackState<'a> {
    target_id: &'a str,
    target_label: &'a str,
    target_icon_path: &'a str,
    private: bool,
    supports_private: bool,
    matched: bool,
}

#[derive(Serialize)]
struct GeneralState {
    theme: &'static str,
    native_popup_available: bool,
    native_popup: bool,
    show_hotkeys: bool,
    quit_on_lost_focus_visible: bool,
    quit_on_lost_focus: bool,
    unwrap_urls: bool,
    run_in_background: bool,
}

// a target's picture: the profile's own one, else the app icon.
// Checked here because Swift treats a picture that fails to load as none, without falling back
fn target_icon_path<'a>(profile_icon_path: &'a str, icon_path: &'a str) -> &'a str {
    if !profile_icon_path.is_empty() && Path::new(profile_icon_path).is_file() {
        profile_icon_path
    } else {
        icon_path
    }
}

fn target_icon(target: &TargetDisplay) -> &str {
    target_icon_path(&target.profile_icon_path, &target.icon_path)
}

fn row_state(row: &BrowserRow) -> RowState<'_> {
    RowState {
        id: &row.meta.id,
        kind: row.meta.kind,
        app_id: &row.meta.app_id,
        name: &row.name,
        detail: &row.detail,
        icon_path: &row.icon_path,
        letter: &row.letter,
        // the popup's avatar colors, so a profile looks the same in both
        letter_color: main_window::avatar_color_hex(&row.meta.id),
        hidden: row.meta.hidden,
        is_default: row.is_default,
        collapsed: row.collapsed,
        draggable: row.meta.draggable,
        menu: row
            .menu()
            .iter()
            .map(|&action| MenuItemState {
                action,
                title: action.title(row.is_default),
            })
            .collect(),
    }
}

fn browsers_state(model: &BrowsersModel) -> BrowsersState<'_> {
    BrowsersState {
        filter: &model.filter,
        show_hidden: model.show_hidden,
        hidden_count: model.hidden_count,
        reorder_enabled: model.reorder_enabled,
        order_hint: model.order_hint(),
        banner: model
            .banner()
            .map(|(title, message)| BannerState { title, message }),
        found_text: &model.found_text,
        empty_text: model.empty_text(),
        rows: model.rows.iter().map(row_state).collect(),
        special_rows: model.special_rows.iter().map(row_state).collect(),
    }
}

fn rules_state(model: &RulesModel) -> RulesState<'_> {
    RulesState {
        rules: model
            .rules
            .iter()
            .map(|rule| RuleState {
                pattern: &rule.pattern,
                pattern_invalid: rule.pattern_edit.is_some(),
                pattern_edit: rule.pattern_edit.as_deref().unwrap_or_default(),
                source_id: &rule.source_id,
                source_label: &rule.source_label,
                target_id: &rule.target_id,
                target_label: &rule.target.label,
                target_icon_path: target_icon(&rule.target),
                private: rule.private,
                supports_private: rule.target.supports_private,
                matched: rule.matched,
            })
            .collect(),
        source_options: model
            .sources
            .iter()
            .map(|source| SourceOptionState {
                id: &source.id,
                label: &source.label,
            })
            .collect(),
        target_options: model
            .targets
            .iter()
            .map(|target| TargetOptionState {
                id: target.id.as_deref().unwrap_or_default(),
                label: &target.label,
                icon_path: target_icon_path(&target.profile_icon_path, &target.icon_path),
            })
            .collect(),
        draft: model.draft.as_ref().map(|draft| DraftState {
            pattern: &draft.pattern,
            pattern_placeholder: draft.pattern_placeholder,
            pattern_invalid: draft.pattern_invalid,
            source_id: &draft.source_id,
            source_label: &draft.source_label,
            target_id: draft.target_id.as_deref(),
            target_label: &draft.target_label,
            help: &draft.help,
        }),
        test: TestState {
            url: &model.test.url,
            source_id: &model.test.source_id,
            source_label: &model.test.source_label,
            result_kind: model.test.result_kind,
            result_text: &model.test.result_text,
        },
        fallback: FallbackState {
            target_id: &model.fallback.target_id,
            target_label: &model.fallback.target.label,
            target_icon_path: target_icon(&model.fallback.target),
            private: model.fallback.private,
            supports_private: model.fallback.target.supports_private,
            matched: model.fallback.matched,
        },
    }
}

fn general_state(model: &GeneralModel) -> GeneralState {
    GeneralState {
        theme: native_popup::appearance(model.theme),
        native_popup_available: model.native_popup_available,
        native_popup: model.native_popup,
        show_hotkeys: model.show_hotkeys,
        quit_on_lost_focus_visible: model.quit_on_lost_focus_visible,
        quit_on_lost_focus: model.quit_on_lost_focus,
        unwrap_urls: model.unwrap_urls,
        run_in_background: model.run_in_background,
    }
}

// the complete Settings state, built before calling Swift so nothing is borrowed while it runs
pub fn settings_state_json(model: &SettingsModel, about: &AboutInfo) -> CString {
    to_c_json(&SettingsState {
        // the window follows the theme setting, so it's always the same as general.theme
        appearance: native_popup::appearance(model.general.theme),
        is_default_browser: model.is_default_browser,
        browsers: browsers_state(&model.browsers),
        rules: rules_state(&model.rules),
        general: general_state(&model.general),
        about,
    })
}

fn to_c_json(value: &impl Serialize) -> CString {
    let json = serde_json::to_string(value).expect("settings state is always serializable");
    // serde_json escapes control characters, so there's never a NUL inside a string
    CString::new(json).expect("JSON never contains a NUL byte")
}

// ------------------------------------------------------------------------------------ Calls

thread_local! {
    static ON_EVENT: Cell<Option<fn(SettingsEvent)>> = const { Cell::new(None) };
}

// the handler is a fn pointer in a Cell rather than a boxed closure in a RefCell, so nothing is
// borrowed while it runs and it can call back into Swift (answering with an update)
extern "C" fn on_event_json(event_json: *const c_char) {
    if event_json.is_null() {
        return;
    }
    // the pointer is only valid during this call, see browsers_settings.h
    let json = unsafe { CStr::from_ptr(event_json) }.to_string_lossy();
    let event = match parse_event(&json) {
        Ok(event) => event,
        Err(err) => {
            tracing::warn!("Ignoring an unknown event from the native Settings {json}: {err}");
            return;
        }
    };
    tracing::debug!("Native Settings event: {event:?}");
    let Some(handler) = ON_EVENT.with(Cell::get) else {
        return;
    };
    // unwinding into Swift is undefined behavior, so a panic stops here
    let result = std::panic::catch_unwind(move || handler(event));
    if result.is_err() {
        tracing::error!("Handling a native Settings event panicked");
    }
}

// call once, before anything else here.
// Uses the native windows whenever the native popup could be used, whatever the "Native popup"
// setting says (that one only picks the popup), returns whether they're on
pub fn init(on_event: fn(SettingsEvent)) -> bool {
    if is_initialized() {
        return true;
    }
    if let Some(reason) = native_popup::unavailable_reason() {
        tracing::info!("Native Settings and About unavailable ({reason}), using the Slint windows");
        return false;
    }
    ON_EVENT.with(|cell| cell.set(Some(on_event)));
    unsafe { browsers_settings_init(on_event_json) };
    tracing::info!("Settings and About: Native");
    true
}

pub fn is_initialized() -> bool {
    ON_EVENT.with(Cell::get).is_some()
}

// `page` is "browsers", "rules", "general" or "about", None keeps the page an open window is on
// (Browsers for a closed one)
pub fn settings_show(state_json: &CStr, page: Option<&str>) {
    if !is_initialized() {
        return;
    }
    let page = page.and_then(|page| CString::new(page).ok());
    let page_ptr = page.as_ref().map_or(std::ptr::null(), |page| page.as_ptr());
    unsafe { browsers_settings_show(state_json.as_ptr(), page_ptr) };
}

pub fn settings_update(state_json: &CStr) {
    if is_initialized() {
        unsafe { browsers_settings_update(state_json.as_ptr()) };
    }
}

// closes without a settings_closed event
pub fn settings_close() {
    if is_initialized() {
        unsafe { browsers_settings_close() };
    }
}

pub fn settings_is_open() -> bool {
    is_initialized() && unsafe { browsers_settings_is_open() }
}

pub fn about_show(about_json: &CStr) {
    if is_initialized() {
        unsafe { browsers_about_show(about_json.as_ptr()) };
    }
}

pub fn about_update(about_json: &CStr) {
    if is_initialized() {
        unsafe { browsers_about_update(about_json.as_ptr()) };
    }
}

// closes without an about_closed event
pub fn about_close() {
    if is_initialized() {
        unsafe { browsers_about_close() };
    }
}

pub fn about_is_open() -> bool {
    is_initialized() && unsafe { browsers_about_is_open() }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use serde_json::{Value, json};

    use super::*;
    use crate::gui::app_state::UIProfileAndIncognito;
    use crate::gui::settings_model::tests::{browser, rule, settings, special};
    use crate::gui::settings_model::{
        BrowsersView, Draft, RulesView, browsers_model, general_model, rules_model,
    };

    fn keys(value: &Value) -> Vec<&str> {
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort();
        keys
    }

    fn sorted(mut keys: Vec<&str>) -> Vec<&str> {
        keys.sort();
        keys
    }

    fn about() -> AboutInfo {
        AboutInfo::new(
            "/Applications/Browsers.app/icon.png".to_string(),
            vec![("Config".to_string(), "/config/".to_string())],
        )
    }

    fn json_value(json: CString) -> Value {
        serde_json::from_str(json.to_str().unwrap()).unwrap()
    }

    // Safari | Chrome: Work (default), Home, hidden School | Slack, with a rule and a draft
    fn sample_state() -> Value {
        let visible = vec![
            browser("Safari", "", "com.apple.Safari"),
            browser("Chrome", "Work", "com.google.Chrome"),
            browser("Chrome", "Home", "com.google.Chrome"),
            special("Slack", "T0123", "com.tinyspeck.slackmacgap"),
        ];
        let hidden = vec![browser("Chrome", "School", "com.google.Chrome")];
        let work = visible[1].unique_id.clone();
        let mut settings = settings();
        settings.default_opener = Some(UIProfileAndIncognito {
            profile: work.clone(),
            incognito: false,
        });
        settings.rules = vec![rule(
            "com.tinyspeck.slackmacgap",
            "github.com/acme/**",
            Some(&work),
        )];

        let collapsed = HashSet::new();
        let browsers = browsers_model(
            &visible,
            &hidden,
            &settings,
            &BrowsersView {
                filter: "",
                show_hidden: false,
                collapsed: &collapsed,
            },
            None,
        );
        let draft = Draft {
            pattern: "notion.so".to_string(),
            ..Default::default()
        };
        let edits = HashMap::new();
        let rules = rules_model(
            &visible,
            &hidden,
            &settings,
            &RulesView {
                draft: Some(&draft),
                test_url: "https://github.com/acme/api",
                test_source: 0,
                pattern_edits: &edits,
            },
        );
        let model = SettingsModel {
            is_default_browser: false,
            browsers,
            rules,
            general: general_model(&settings, true),
        };
        json_value(settings_state_json(&model, &about()))
    }

    #[test]
    fn settings_state_has_the_contract_shape() {
        let value = sample_state();
        assert_eq!(
            keys(&value),
            sorted(vec![
                "appearance",
                "is_default_browser",
                "browsers",
                "rules",
                "general",
                "about"
            ])
        );
        assert_eq!(value["appearance"], "system");
        assert_eq!(value["appearance"], value["general"]["theme"]);
        assert_eq!(value["is_default_browser"], false);

        let browsers = &value["browsers"];
        assert_eq!(
            keys(browsers),
            sorted(vec![
                "filter",
                "show_hidden",
                "hidden_count",
                "reorder_enabled",
                "order_hint",
                "banner",
                "found_text",
                "empty_text",
                "rows",
                "special_rows"
            ])
        );
        assert_eq!(browsers["filter"], "");
        assert_eq!(browsers["hidden_count"], 1);
        assert_eq!(browsers["order_hint"], "Drag to reorder · 1–9 follow this order");
        assert_eq!(
            browsers["banner"],
            json!({
                "title": "1 of these isn't a web browser",
                "message": "Slack can open links, but you probably don't want them in the popup."
            })
        );
        assert_eq!(browsers["found_text"], "Found 3 apps");
        assert_eq!(browsers["empty_text"], "No apps found. Try Rescan.");

        let rows = browsers["rows"].as_array().unwrap();
        let row_keys = sorted(vec![
            "id",
            "kind",
            "app_id",
            "name",
            "detail",
            "icon_path",
            "letter",
            "letter_color",
            "hidden",
            "is_default",
            "collapsed",
            "draggable",
            "menu",
        ]);
        for row in rows
            .iter()
            .chain(browsers["special_rows"].as_array().unwrap())
        {
            assert_eq!(keys(row), row_keys);
        }
        assert_eq!(rows[0]["kind"], "single");
        assert_eq!(rows[0]["id"], "/Applications/Safari.app");
        assert_eq!(
            rows[0]["menu"],
            json!([
                { "action": "move_to_top", "title": "Move to top" },
                { "action": "move_to_bottom", "title": "Move to bottom" },
                { "action": "toggle_default", "title": "Open unmatched links here" }
            ])
        );
        assert_eq!(rows[1]["kind"], "header");
        assert_eq!(rows[1]["id"], "group:/Applications/Chrome.app");
        assert_eq!(rows[1]["app_id"], "/Applications/Chrome.app");
        assert_eq!(rows[1]["detail"], "3 profiles · 1 hidden");
        assert_eq!(rows[1]["menu"].as_array().unwrap().len(), 2);
        assert_eq!(rows[2]["kind"], "child");
        assert_eq!(rows[2]["name"], "Work");
        assert_eq!(rows[2]["icon_path"], "");
        assert_eq!(rows[2]["letter"], "W");
        assert_eq!(rows[2]["is_default"], true);
        assert_eq!(rows[2]["menu"][2]["title"], "Stop opening unmatched links here");
        assert_eq!(
            rows[2]["letter_color"],
            main_window::avatar_color_hex("/Applications/Chrome.app#Work")
        );
        let special = &browsers["special_rows"][0];
        assert_eq!(special["draggable"], false);
        assert_eq!(
            special["menu"],
            json!([{ "action": "toggle_default", "title": "Open unmatched links here" }])
        );

        let general = &value["general"];
        assert_eq!(
            keys(general),
            sorted(vec![
                "theme",
                "native_popup_available",
                "native_popup",
                "show_hotkeys",
                "quit_on_lost_focus_visible",
                "quit_on_lost_focus",
                "unwrap_urls",
                "run_in_background"
            ])
        );
        assert_eq!(general["native_popup_available"], true);
        assert_eq!(general["quit_on_lost_focus_visible"], false);

        let mut about_keys = keys(&value["about"]);
        about_keys.push("appearance");
        assert_eq!(
            sorted(about_keys),
            keys(&json_value(about_state_json(ConfiguredTheme::Auto, &about())))
        );
    }

    #[test]
    fn rules_state_has_the_contract_shape() {
        let value = sample_state();
        let rules = &value["rules"];
        assert_eq!(
            keys(rules),
            sorted(vec![
                "rules",
                "source_options",
                "target_options",
                "draft",
                "test",
                "fallback"
            ])
        );
        let first = &rules["rules"][0];
        assert_eq!(
            *first,
            json!({
                "pattern": "github.com/acme/**",
                "pattern_invalid": false,
                "pattern_edit": "",
                "source_id": "com.tinyspeck.slackmacgap",
                "source_label": "Slack",
                "target_id": "/Applications/Chrome.app#Work",
                "target_label": "Work · Chrome",
                "target_icon_path": "/icons/Chrome.png",
                "private": false,
                "supports_private": true,
                "matched": false
            })
        );
        assert_eq!(
            rules["source_options"][0],
            json!({ "id": "", "label": "Any app" })
        );
        assert_eq!(
            rules["target_options"][0],
            json!({ "id": "", "label": "Show the picker", "icon_path": "" })
        );
        let hidden_target = rules["target_options"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == "/Applications/Chrome.app#School")
            .unwrap();
        assert_eq!(hidden_target["label"], "School · Chrome (hidden)");
        assert_eq!(hidden_target["icon_path"], "/icons/Chrome.png");

        assert_eq!(
            rules["draft"],
            json!({
                "pattern": "notion.so",
                "pattern_placeholder": "e.g. notion.so",
                "pattern_invalid": false,
                "source_id": "",
                "source_label": "",
                "target_id": null,
                "target_label": "",
                "help": "Matches notion.so and every page on it. Saved once it has a pattern and a target."
            })
        );
        assert_eq!(
            rules["test"],
            json!({
                "url": "https://github.com/acme/api",
                "source_id": "",
                "source_label": "Any app",
                "result_kind": "fallback",
                "result_text": "No rule matches → opens in Work · Chrome"
            })
        );
        assert_eq!(
            rules["fallback"],
            json!({
                "target_id": "/Applications/Chrome.app#Work",
                "target_label": "Work · Chrome",
                "target_icon_path": "/icons/Chrome.png",
                "private": false,
                "supports_private": true,
                "matched": true
            })
        );
    }

    #[test]
    fn no_banner_and_no_draft_are_null() {
        let mut settings = settings();
        settings.visual_settings.non_browser_banner_dismissed = true;
        settings.visual_settings.theme = ConfiguredTheme::Dark;
        let collapsed = HashSet::new();
        let edits = HashMap::new();
        let model = SettingsModel {
            is_default_browser: true,
            browsers: browsers_model(
                &[],
                &[],
                &settings,
                &BrowsersView {
                    filter: "",
                    show_hidden: false,
                    collapsed: &collapsed,
                },
                None,
            ),
            rules: rules_model(
                &[],
                &[],
                &settings,
                &RulesView {
                    draft: None,
                    test_url: "",
                    test_source: 0,
                    pattern_edits: &edits,
                },
            ),
            general: general_model(&settings, false),
        };
        let value = json_value(settings_state_json(&model, &about()));
        assert_eq!(value["appearance"], "dark");
        assert_eq!(value["general"]["theme"], "dark");
        assert_eq!(value["is_default_browser"], true);
        assert!(value["browsers"]["banner"].is_null());
        assert!(value["rules"]["draft"].is_null());
        assert_eq!(value["rules"]["test"]["result_kind"], "none");
        assert_eq!(value["rules"]["test"]["result_text"], "");
        assert_eq!(value["rules"]["fallback"]["target_label"], "Show the picker");
        assert_eq!(value["rules"]["fallback"]["target_id"], "");
    }

    #[test]
    fn rejected_pattern_edits_come_back_in_the_state() {
        let mut settings = settings();
        settings.rules = vec![rule("", "notion.so", None)];
        let edits = HashMap::from([(0, "notion.so/[".to_string())]);
        let model = rules_model(
            &[],
            &[],
            &settings,
            &RulesView {
                draft: None,
                test_url: "",
                test_source: 0,
                pattern_edits: &edits,
            },
        );
        let value = serde_json::to_value(rules_state(&model)).unwrap();
        assert_eq!(value["rules"][0]["pattern"], "notion.so");
        assert_eq!(value["rules"][0]["pattern_invalid"], true);
        assert_eq!(value["rules"][0]["pattern_edit"], "notion.so/[");
    }

    #[test]
    fn about_state_has_the_contract_shape() {
        let value = json_value(about_state_json(ConfiguredTheme::Light, &about()));
        assert_eq!(
            keys(&value),
            sorted(vec![
                "appearance",
                "app_icon_path",
                "app_name",
                "version_text",
                "copyright",
                "website_url",
                "directories",
                "made_with_slint"
            ])
        );
        assert_eq!(value["appearance"], "light");
        assert_eq!(value["app_name"], "Browsers");
        assert_eq!(
            value["version_text"],
            format!("Version {}", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(value["copyright"], "© 2022-2024 Browsers.software team.");
        assert_eq!(value["website_url"], "https://browsers.software");
        assert_eq!(
            value["directories"],
            json!([{ "label": "Config", "path": "/config/" }])
        );
        assert_eq!(keys(&value["made_with_slint"]), ["label", "url", "version_text"]);
        assert_eq!(value["made_with_slint"]["label"], "Made with Slint");
        assert_eq!(value["made_with_slint"]["url"], "https://slint.dev");
        let slint_version = value["made_with_slint"]["version_text"].as_str().unwrap();
        assert!(slint_version.starts_with("Version 1."), "{slint_version}");
    }

    #[test]
    fn target_icon_falls_back_to_the_app_icon() {
        assert_eq!(target_icon_path("", "/app.png"), "/app.png");
        assert_eq!(target_icon_path("/no/such/picture.png", "/app.png"), "/app.png");
        let picture = std::env::temp_dir().join("browsers-native-settings-test.png");
        std::fs::write(&picture, b"").unwrap();
        let picture = picture.to_string_lossy().to_string();
        assert_eq!(target_icon_path(&picture, "/app.png"), picture);
    }

    #[test]
    fn parses_every_event() {
        use SettingsEvent::*;
        let cases: Vec<(&str, SettingsEvent)> = vec![
            (r#"{"type":"settings_closed"}"#, SettingsClosed),
            (r#"{"type":"about_closed"}"#, AboutClosed),
            (r#"{"type":"set_default_browser"}"#, SetDefaultBrowser),
            (
                r#"{"type":"browsers_filter_changed","text":"chr"}"#,
                BrowsersFilterChanged {
                    text: "chr".to_string(),
                },
            ),
            (
                r#"{"type":"set_show_hidden","show":true}"#,
                SetShowHidden { show: true },
            ),
            (r#"{"type":"rescan"}"#, Rescan),
            (r#"{"type":"hide_non_browsers"}"#, HideNonBrowsers),
            (
                r#"{"type":"dismiss_non_browser_banner"}"#,
                DismissNonBrowserBanner,
            ),
            (
                r#"{"type":"toggle_row_visibility","list":"rows","row":2,"row_id":"a#1"}"#,
                ToggleRowVisibility {
                    list: RowList::Rows,
                    row: 2,
                    row_id: "a#1".to_string(),
                },
            ),
            (
                r#"{"type":"set_group_collapsed","app_id":"a","collapsed":true}"#,
                SetGroupCollapsed {
                    app_id: "a".to_string(),
                    collapsed: true,
                },
            ),
            (
                r#"{"type":"row_menu","list":"special_rows","row":0,"row_id":"s","action":"toggle_default"}"#,
                RowMenu {
                    list: RowList::SpecialRows,
                    row: 0,
                    row_id: "s".to_string(),
                    action: RowAction::ToggleDefault,
                },
            ),
            (
                r#"{"type":"row_menu","list":"rows","row":1,"row_id":"r","action":"move_to_bottom"}"#,
                RowMenu {
                    list: RowList::Rows,
                    row: 1,
                    row_id: "r".to_string(),
                    action: RowAction::MoveToBottom,
                },
            ),
            (
                r#"{"type":"move_browser_row","row":1,"row_id":"group:a","to":0}"#,
                MoveBrowserRow {
                    row: 1,
                    row_id: "group:a".to_string(),
                    to: 0,
                },
            ),
            (r#"{"type":"add_rule"}"#, AddRule),
            (
                r#"{"type":"rule_pattern_edited","index":0,"pattern":"github.com"}"#,
                RulePatternEdited {
                    index: 0,
                    pattern: "github.com".to_string(),
                },
            ),
            (
                r#"{"type":"rule_source_picked","index":1,"source_id":"com.apple.mail"}"#,
                RuleSourcePicked {
                    index: 1,
                    source_id: "com.apple.mail".to_string(),
                },
            ),
            (
                r#"{"type":"rule_target_picked","index":0,"target_id":""}"#,
                RuleTargetPicked {
                    index: 0,
                    target_id: String::new(),
                },
            ),
            (
                r#"{"type":"rule_private_set","index":0,"private":true}"#,
                RulePrivateSet {
                    index: 0,
                    private: true,
                },
            ),
            (
                r#"{"type":"rule_duplicate","index":3}"#,
                RuleDuplicate { index: 3 },
            ),
            (r#"{"type":"rule_delete","index":0}"#, RuleDelete { index: 0 }),
            (
                r#"{"type":"move_rule","from":2,"to":0}"#,
                MoveRule { from: 2, to: 0 },
            ),
            (
                r#"{"type":"draft_pattern_edited","text":"notion.s"}"#,
                DraftPatternEdited {
                    text: "notion.s".to_string(),
                },
            ),
            (r#"{"type":"draft_pattern_committed"}"#, DraftPatternCommitted),
            (
                r#"{"type":"draft_source_picked","source_id":"com.apple.mail"}"#,
                DraftSourcePicked {
                    source_id: "com.apple.mail".to_string(),
                },
            ),
            (
                r#"{"type":"draft_target_picked","target_id":""}"#,
                DraftTargetPicked {
                    target_id: String::new(),
                },
            ),
            (r#"{"type":"draft_discarded"}"#, DraftDiscarded),
            (
                r#"{"type":"test_url_edited","text":"https://x.y"}"#,
                TestUrlEdited {
                    text: "https://x.y".to_string(),
                },
            ),
            (
                r#"{"type":"test_source_picked","source_id":""}"#,
                TestSourcePicked {
                    source_id: String::new(),
                },
            ),
            (
                r#"{"type":"fallback_target_picked","target_id":"/Applications/Safari.app"}"#,
                FallbackTargetPicked {
                    target_id: "/Applications/Safari.app".to_string(),
                },
            ),
            (
                r#"{"type":"fallback_private_set","private":false}"#,
                FallbackPrivateSet { private: false },
            ),
            (
                r#"{"type":"set_theme","theme":"dark"}"#,
                SetTheme {
                    theme: Appearance::Dark,
                },
            ),
            (
                r#"{"type":"set_native_popup","value":false}"#,
                SetNativePopup { value: false },
            ),
            (
                r#"{"type":"set_show_hotkeys","value":true}"#,
                SetShowHotkeys { value: true },
            ),
            (
                r#"{"type":"set_quit_on_lost_focus","value":true}"#,
                SetQuitOnLostFocus { value: true },
            ),
            (
                r#"{"type":"set_unwrap_urls","value":true}"#,
                SetUnwrapUrls { value: true },
            ),
            (
                r#"{"type":"set_run_in_background","value":false}"#,
                SetRunInBackground { value: false },
            ),
        ];
        for (json, expected) in cases {
            assert_eq!(parse_event(json).unwrap(), expected, "{json}");
        }
    }

    #[test]
    fn set_theme_maps_to_the_configured_theme() {
        assert_eq!(Appearance::System.theme(), ConfiguredTheme::Auto);
        assert_eq!(Appearance::Light.theme(), ConfiguredTheme::Light);
        assert_eq!(Appearance::Dark.theme(), ConfiguredTheme::Dark);
        for theme in [
            ConfiguredTheme::Auto,
            ConfiguredTheme::Light,
            ConfiguredTheme::Dark,
        ] {
            let json = format!(
                r#"{{"type":"set_theme","theme":"{}"}}"#,
                native_popup::appearance(theme)
            );
            let SettingsEvent::SetTheme { theme: parsed } = parse_event(&json).unwrap() else {
                panic!("not a set_theme event");
            };
            assert_eq!(parsed.theme(), theme);
        }
    }

    #[test]
    fn rejects_unknown_or_incomplete_events() {
        assert!(parse_event(r#"{"type":"no_such_event"}"#).is_err());
        assert!(parse_event(r#"{"type":"rule_delete"}"#).is_err());
        assert!(parse_event(r#"{"type":"rule_delete","index":-1}"#).is_err());
        assert!(
            parse_event(r#"{"type":"row_menu","list":"rows","row":0,"row_id":"a","action":"x"}"#)
                .is_err()
        );
        assert!(parse_event(r#"{"type":"set_theme","theme":"auto"}"#).is_err());
        assert!(parse_event("not json").is_err());
    }
}
