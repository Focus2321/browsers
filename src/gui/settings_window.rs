// the Settings window: the Slint one here, or the native one on macOS 13+ (macos::native_settings).
// Both show the same gui::settings_model and call the same handlers below, so only drawing differs.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime};

use slint::{Color, ComponentHandle, Image, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::gui::app::{self, SharedState};
use crate::gui::app_state::UIProfileAndIncognito;
use crate::gui::generated::{
    Palette as StdWidgetPalette, SettingsBrowserRow, SettingsRuleRow, SettingsWindow, ThemeChoice,
};
use crate::gui::icon_loader::IconLoader;
use crate::gui::settings_model::{
    self, BrowserRow, BrowsersView, Draft, RowAction, RowKind, RowList, RowMeta, RulesView,
    SettingsModel, TargetDisplay, block_end, drop_target, join_names, move_item, retarget,
    snap_drop_index,
};
use crate::gui::theme::ColorScheme;
use crate::gui::ui_util::sync_vec_model;
#[cfg(target_os = "macos")]
use crate::macos::native_settings::{self, SettingsEvent};
use crate::utils::ConfiguredTheme;
use crate::{MessageToMain, paths, utils};

const LETTER_COLORS: &[(u8, u8, u8)] = &[
    (0x5b, 0x8d, 0xef),
    (0xe5, 0x73, 0x4a),
    (0x3f, 0xb2, 0x7f),
    (0xa6, 0x6d, 0xe8),
    (0xe2, 0xa9, 0x3b),
    (0xe0, 0x5c, 0x8a),
    (0x4b, 0xb3, 0xc4),
];

// window-local state that only Settings cares about
#[derive(Default)]
struct Local {
    filter: String,
    show_hidden: bool,
    // app ids of collapsed groups
    collapsed: HashSet<String>,
    // the rows last shown, what row events refer to
    rows: Vec<RowMeta>,
    special_rows: Vec<RowMeta>,
    // every orderable (non-priority) profile id in popup order, hidden ones included
    order: Vec<String>,
    draft: Option<Draft>,
    test_url: String,
    // index into source_ids
    test_source: usize,
    // parallel to the target-options shown in pickers: None = "Show the picker"
    target_ids: Vec<Option<String>>,
    // parallel to the source-options shown in pickers: "" = any app
    source_ids: Vec<String>,
    // rule index -> the invalid text last typed into its pattern, which isn't saved.
    // Indexes shift when rules move, so this is cleared whenever the list changes shape.
    rule_pattern_edits: HashMap<usize, String>,
    is_default_browser: bool,
    default_browser_poll: Option<Timer>,
}

thread_local! {
    static LOCAL: RefCell<Local> = RefCell::default();
}

fn with_local<R>(f: impl FnOnce(&mut Local) -> R) -> R {
    LOCAL.with(|local| f(&mut local.borrow_mut()))
}

// a fresh window starts out unfiltered, without a draft or a test link
fn reset_local() {
    with_local(|local| {
        *local = Local {
            is_default_browser: utils::is_default_web_browser(),
            ..Local::default()
        }
    });
}

// forces std-widgets' own Palette (which otherwise just tracks the OS) so an explicit Light/Dark
// choice also applies to native controls like the menus.
// Auto maps to Unknown, un-forcing it.
fn theme_to_color_scheme(theme: ConfiguredTheme) -> ColorScheme {
    match theme {
        ConfiguredTheme::Auto => ColorScheme::Unknown,
        ConfiguredTheme::Light => ColorScheme::Light,
        ConfiguredTheme::Dark => ColorScheme::Dark,
    }
}

fn theme_to_choice(theme: ConfiguredTheme) -> ThemeChoice {
    match theme {
        ConfiguredTheme::Auto => ThemeChoice::Auto,
        ConfiguredTheme::Light => ThemeChoice::Light,
        ConfiguredTheme::Dark => ThemeChoice::Dark,
    }
}

fn choice_to_theme(choice: ThemeChoice) -> ConfiguredTheme {
    match choice {
        ThemeChoice::Auto => ConfiguredTheme::Auto,
        ThemeChoice::Light => ConfiguredTheme::Light,
        ThemeChoice::Dark => ConfiguredTheme::Dark,
    }
}

// lets debug builds' screenshots start on any page (0-3: browsers, rules, general, about)
fn debug_page() -> Option<usize> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var("BROWSERS_DEBUG_SETTINGS_PAGE")
        .ok()
        .and_then(|p| p.parse().ok())
}

pub fn open(state: &SharedState) {
    #[cfg(target_os = "macos")]
    if native_settings::is_initialized() {
        open_native(state);
        return;
    }

    // state.settings_window gets cleared to None on actual close, so Some here only ever means
    // "currently showing"
    let already_open = state
        .borrow()
        .settings_window
        .as_ref()
        .map(|w| w.clone_strong());
    if app::focus_if_open(already_open) {
        return;
    }

    let win = SettingsWindow::new().expect("failed to create settings window");
    // needs real VecModels here, not the generated defaults - see ui_util::sync_vec_model for why
    win.set_browser_rows(ModelRc::new(VecModel::default()));
    win.set_special_rows(ModelRc::new(VecModel::default()));
    win.set_rules(ModelRc::new(VecModel::default()));
    win.set_target_options(ModelRc::new(VecModel::default()));
    win.set_source_options(ModelRc::new(VecModel::default()));
    // none of the About page changes during a run, so it's set once here rather than on every
    // refresh() call
    win.set_directories(ModelRc::new(VecModel::from(
        crate::gui::about_window::directories_entries(),
    )));
    win.set_app_icon(
        state
            .borrow_mut()
            .icons
            .load(&paths::get_app_icon_path().to_string_lossy()),
    );
    win.set_version_text(crate::gui::about_window::version_text().into());
    win.set_debug_hold_drag(
        cfg!(debug_assertions) && std::env::var_os("BROWSERS_DEBUG_HOLD_DRAG").is_some(),
    );
    if let Some(page) = debug_page() {
        win.set_page(page as i32);
    }

    reset_local();
    state.borrow_mut().settings_window = Some(win.clone_strong());

    app::apply_theme(state);
    wire(state, &win);
    refresh(state);

    win.window().show().ok();

    // otherwise this renders behind the always-on-top main popup - see app::make_window_floating
    app::make_window_floating(&win);

    app::clear_on_close(&win, state, |st| st.settings_window = None);
}

// opening a closed window starts it over, an open one just comes to the front with fresh state
#[cfg(target_os = "macos")]
fn open_native(state: &SharedState) {
    if !native_settings::settings_is_open() {
        reset_local();
    }
    let page =
        debug_page().and_then(|page| ["browsers", "rules", "general", "about"].get(page).copied());
    let json = native_state_json(state);
    native_settings::settings_show(&json, page);
}

fn settings_window(state: &SharedState) -> Option<SettingsWindow> {
    state
        .borrow()
        .settings_window
        .as_ref()
        .map(|w| w.clone_strong())
}

// shows the current state in whichever Settings window is open
pub fn refresh(state: &SharedState) {
    #[cfg(target_os = "macos")]
    if native_settings::is_initialized() {
        refresh_native(state);
        return;
    }
    let Some(win) = settings_window(state) else {
        return;
    };
    let model = build_model(state);
    apply_to_slint(state, &win, &model);
}

#[cfg(target_os = "macos")]
pub fn refresh_native(state: &SharedState) {
    if native_settings::settings_is_open() {
        let json = native_state_json(state);
        native_settings::settings_update(&json);
    }
}

// built before calling Swift, so neither AppState nor Local is borrowed while it runs
#[cfg(target_os = "macos")]
fn native_state_json(state: &SharedState) -> std::ffi::CString {
    let model = build_model(state);
    native_settings::settings_state_json(&model, native_settings::AboutInfo::current())
}

// when the installed apps were last scanned, from the scan's cache file
fn last_scan_time() -> Option<SystemTime> {
    std::fs::metadata(paths::get_cache_root_dir().join("installed_browsers.json"))
        .and_then(|m| m.modified())
        .ok()
}

fn native_popup_available() -> bool {
    #[cfg(target_os = "macos")]
    return crate::macos::native_popup::is_supported();
    #[cfg(not(target_os = "macos"))]
    false
}

// everything Settings shows, also remembering what the rows and pickers stand for, since the
// events only carry their positions
fn build_model(state: &SharedState) -> SettingsModel {
    let scanned_ago = last_scan_time().and_then(|t| t.elapsed().ok());
    let st = state.borrow();
    let visible = &st.browsers;
    let hidden = &st.restorable_app_profiles;
    let settings = &st.ui_settings;
    with_local(|l| {
        let browsers = settings_model::browsers_model(
            visible,
            hidden,
            settings,
            &BrowsersView {
                filter: &l.filter,
                show_hidden: l.show_hidden,
                collapsed: &l.collapsed,
            },
            scanned_ago,
        );
        let rules = settings_model::rules_model(
            visible,
            hidden,
            settings,
            &RulesView {
                draft: l.draft.as_ref(),
                test_url: &l.test_url,
                test_source: l.test_source,
                pattern_edits: &l.rule_pattern_edits,
            },
        );
        l.rows = browsers.rows.iter().map(|r| r.meta.clone()).collect();
        l.special_rows = browsers
            .special_rows
            .iter()
            .map(|r| r.meta.clone())
            .collect();
        l.order = browsers.order.clone();
        l.test_source = rules.test.source_index;
        l.target_ids = rules.targets.iter().map(|t| t.id.clone()).collect();
        l.source_ids = rules.sources.iter().map(|s| s.id.clone()).collect();
        SettingsModel {
            is_default_browser: l.is_default_browser,
            browsers,
            rules,
            general: settings_model::general_model(settings, native_popup_available()),
        }
    })
}

// ------------------------------------------------------------------------------------ Slint

fn letter_color(name: &str) -> Color {
    let hash = name
        .bytes()
        .fold(0usize, |acc, b| acc.wrapping_mul(31).wrapping_add(b as usize));
    let (r, g, b) = LETTER_COLORS[hash % LETTER_COLORS.len()];
    Color::from_rgb_u8(r, g, b)
}

fn loaded(image: Image) -> Option<Image> {
    (image.size().width > 0).then_some(image)
}

fn slint_row(row: &BrowserRow, icons: &mut IconLoader) -> SettingsBrowserRow {
    let icon = loaded(icons.load(&row.icon_path));
    SettingsBrowserRow {
        kind: row.meta.kind.as_int(),
        letter: row.letter.clone().into(),
        letter_color: letter_color(&row.name),
        name: row.name.clone().into(),
        detail: row.detail.clone().into(),
        has_icon: icon.is_some(),
        icon: icon.unwrap_or_default(),
        hidden: row.meta.hidden,
        is_default: row.is_default,
        collapsed: row.collapsed,
        draggable: row.meta.draggable,
    }
}

// the profile's picture, else the app icon
fn target_icon(target: &TargetDisplay, icons: &mut IconLoader) -> Option<Image> {
    loaded(icons.load(&target.profile_icon_path)).or_else(|| loaded(icons.load(&target.icon_path)))
}

fn apply_to_slint(state: &SharedState, win: &SettingsWindow, model: &SettingsModel) {
    let mut icons = std::mem::take(&mut state.borrow_mut().icons);

    let browsers = &model.browsers;
    let rows: Vec<SettingsBrowserRow> = browsers
        .rows
        .iter()
        .map(|row| slint_row(row, &mut icons))
        .collect();
    let special_rows: Vec<SettingsBrowserRow> = browsers
        .special_rows
        .iter()
        .map(|row| slint_row(row, &mut icons))
        .collect();

    let rules = &model.rules;
    let rule_rows: Vec<SettingsRuleRow> = rules
        .rules
        .iter()
        .map(|rule| {
            let icon = target_icon(&rule.target, &mut icons);
            SettingsRuleRow {
                pattern: rule.pattern.clone().into(),
                source_label: rule.source_label.clone().into(),
                target_label: rule.target.label.clone().into(),
                target_has_icon: icon.is_some(),
                target_icon: icon.unwrap_or_default(),
                private: rule.private,
                supports_private: rule.target.supports_private,
            }
        })
        .collect();
    let fallback_icon = target_icon(&rules.fallback.target, &mut icons);

    state.borrow_mut().icons = icons;

    win.set_is_default_browser(model.is_default_browser);

    // ---- Browsers
    win.set_show_hidden(browsers.show_hidden);
    win.set_hidden_count(browsers.hidden_count as i32);
    win.set_reorder_enabled(browsers.reorder_enabled);
    win.set_order_hint(browsers.order_hint().into());
    win.set_banner_count(browsers.banner_names.len() as i32);
    win.set_banner_names(join_names(&browsers.banner_names).into());
    win.set_found_text(browsers.found_text.clone().into());
    sync_vec_model(&win.get_browser_rows(), rows);
    sync_vec_model(&win.get_special_rows(), special_rows);

    // ---- Rules
    let target_options: Vec<SharedString> = rules
        .targets
        .iter()
        .map(|t| t.label.clone().into())
        .collect();
    let source_options: Vec<SharedString> = rules
        .sources
        .iter()
        .map(|s| s.label.clone().into())
        .collect();
    sync_vec_model(&win.get_target_options(), target_options);
    sync_vec_model(&win.get_source_options(), source_options);
    sync_vec_model(&win.get_rules(), rule_rows);

    win.set_has_draft(rules.draft.is_some());
    if let Some(draft) = &rules.draft {
        win.set_draft_source_label(draft.source_label.clone().into());
        win.set_draft_target_label(draft.target_label.clone().into());
        win.set_draft_help(draft.help.clone().into());
        win.set_draft_pattern_invalid(draft.pattern_invalid);
    }

    let test = &rules.test;
    win.set_test_source_label(test.source_label.clone().into());
    win.set_test_result_kind(test.result_kind.as_int());
    win.set_test_result_text(test.result_text.clone().into());
    win.set_matched_rule(test.matched_rule.map_or(-1, |i| i as i32));

    let fallback = &rules.fallback;
    win.set_fallback_target_label(fallback.target.label.clone().into());
    win.set_fallback_target_has_icon(fallback_icon.is_some());
    win.set_fallback_target_icon(fallback_icon.unwrap_or_default());
    win.set_fallback_private(fallback.private);
    win.set_fallback_supports_private(fallback.target.supports_private);

    // ---- General
    let general = &model.general;
    win.set_show_native_popup(general.native_popup_available);
    win.set_show_quit_on_lost_focus(general.quit_on_lost_focus_visible);
    win.set_show_hotkeys(general.show_hotkeys);
    win.set_quit_on_lost_focus(general.quit_on_lost_focus);
    win.set_unwrap_urls(general.unwrap_urls);
    win.set_run_in_background(general.run_in_background);
    win.set_native_popup(general.native_popup);
    win.set_theme(theme_to_choice(general.theme));
    win.global::<StdWidgetPalette>()
        .set_color_scheme(theme_to_color_scheme(general.theme));
}

// ------------------------------------------------------------------------------------ Handlers
// shared by the Slint callbacks (wire) and the native window's events (handle_native_event)

fn send(state: &SharedState, message: MessageToMain) {
    let _ = state.borrow().main_sender.send(message);
}

fn set_default_browser(state: &SharedState) {
    send(state, MessageToMain::SetBrowsersAsDefaultBrowser);
    poll_default_browser(state);
}

// the default browser switch only happens once the user confirms the OS prompt, so the sidebar
// card checks back for a while instead of trusting the click
fn poll_default_browser(state: &SharedState) {
    let weak_state = std::rc::Rc::downgrade(state);
    let timer = Timer::default();
    let mut checks_left = 60;
    timer.start(TimerMode::Repeated, Duration::from_millis(500), move || {
        checks_left -= 1;
        let Some(state) = weak_state.upgrade() else {
            return;
        };
        let is_default = utils::is_default_web_browser();
        let changed =
            with_local(|l| std::mem::replace(&mut l.is_default_browser, is_default) != is_default);
        if changed {
            refresh(&state);
        }
        if is_default {
            state.borrow_mut().show_set_as_default = false;
            app::refresh_main_window_model(&state);
        }
        if is_default || checks_left <= 0 {
            with_local(|l| l.default_browser_poll = None);
        }
    });
    with_local(|l| l.default_browser_poll = Some(timer));
}

// ---- Browsers

fn filter_changed(state: &SharedState, text: &str) {
    with_local(|l| l.filter = text.to_string());
    refresh(state);
}

fn set_show_hidden(state: &SharedState, show: bool) {
    with_local(|l| l.show_hidden = show);
    refresh(state);
}

fn set_group_collapsed(state: &SharedState, app_id: &str, collapsed: bool) {
    with_local(|l| {
        if collapsed {
            l.collapsed.insert(app_id.to_string());
        } else {
            l.collapsed.remove(app_id);
        }
    });
    refresh(state);
}

fn row_meta(list: RowList, row: usize) -> Option<RowMeta> {
    with_local(|l| match list {
        RowList::Rows => l.rows.get(row).cloned(),
        RowList::SpecialRows => l.special_rows.get(row).cloned(),
    })
}

// where a row dragged from `from` lands when released at insertion point `raw`
fn snap_browser_drop(from: usize, raw: usize) -> usize {
    with_local(|l| {
        let kinds: Vec<RowKind> = l.rows.iter().map(|r| r.kind).collect();
        snap_drop_index(&kinds, from, raw)
    })
}

fn move_rows(state: &SharedState, from: usize, to: usize) {
    let target = with_local(|l| drop_target(&l.rows, &l.order, from, to));
    if let Some((ids, before)) = target {
        send(state, MessageToMain::MoveAppProfilesBefore(ids, before));
    }
}

fn toggle_visibility(state: &SharedState, meta: &RowMeta) {
    if meta.hidden {
        for id in &meta.ids {
            send(state, MessageToMain::RestoreAppProfile(id.clone()));
        }
    } else if meta.kind == RowKind::Header {
        send(state, MessageToMain::HideAllProfiles(meta.app_id.clone()));
    } else {
        send(state, MessageToMain::HideAppProfile(meta.ids[0].clone()));
    }
}

fn toggle_default_profile(state: &SharedState, profile: &str) {
    let default_opener = {
        let mut st = state.borrow_mut();
        let opener = &mut st.ui_settings.default_opener;
        if opener.as_ref().is_some_and(|o| o.profile == profile) {
            *opener = None;
        } else {
            *opener = Some(UIProfileAndIncognito {
                profile: profile.to_string(),
                incognito: false,
            });
        }
        opener.clone()
    };
    send(state, MessageToMain::SaveConfigDefaultOpener(default_opener));
    refresh(state);
}

// a "···" menu item of a row of the main list
fn row_menu_picked(state: &SharedState, row: usize, action: RowAction) {
    let (meta, rows, order) =
        with_local(|l| (l.rows.get(row).cloned(), l.rows.clone(), l.order.clone()));
    let Some(meta) = meta else {
        return;
    };
    match action {
        RowAction::MoveToTop | RowAction::MoveToBottom if meta.kind == RowKind::Child => {
            let kinds: Vec<RowKind> = rows.iter().map(|r| r.kind).collect();
            let Some(header) = (0..row).rev().find(|&i| kinds[i] != RowKind::Child) else {
                return;
            };
            let target = if action == RowAction::MoveToTop {
                header + 1
            } else {
                block_end(&kinds, header)
            };
            move_rows(state, row, target);
        }
        RowAction::MoveToTop => {
            let first = order.first().cloned();
            if first.as_ref() != meta.ids.first() {
                send(state, MessageToMain::MoveAppProfilesBefore(meta.ids, first));
            }
        }
        RowAction::MoveToBottom => {
            send(state, MessageToMain::MoveAppProfilesBefore(meta.ids, None))
        }
        RowAction::ToggleDefault => toggle_default_profile(state, &meta.ids[0]),
    }
}

// the special rows' menu only has "Open unmatched links here"
fn special_menu_picked(state: &SharedState, row: usize) {
    if let Some(meta) = row_meta(RowList::SpecialRows, row) {
        toggle_default_profile(state, &meta.ids[0]);
    }
}

fn hide_non_browsers(state: &SharedState) {
    let app_ids: Vec<String> = {
        let st = state.borrow();
        let mut ids: Vec<String> = vec![];
        for b in st.browsers.iter().filter(|b| b.has_priority_ordering()) {
            if !ids.contains(&b.unique_app_id) {
                ids.push(b.unique_app_id.clone());
            }
        }
        ids
    };
    for app_id in app_ids {
        send(state, MessageToMain::HideAllProfiles(app_id));
    }
    dismiss_non_browser_banner(state);
}

fn dismiss_non_browser_banner(state: &SharedState) {
    let settings = {
        let mut st = state.borrow_mut();
        st.ui_settings.visual_settings.non_browser_banner_dismissed = true;
        st.ui_settings.visual_settings.clone()
    };
    send(state, MessageToMain::SaveConfigUISettings(settings));
    refresh(state);
}

// ---- Rules

fn save_rules(state: &SharedState) {
    let rules = state.borrow().ui_settings.rules.clone();
    send(state, MessageToMain::SaveConfigRules(rules));
    refresh(state);
}

// the rule list changes shape: indexes move, so remembered invalid edits would land on the wrong
// rule
fn rules_reshaped(state: &SharedState) {
    with_local(|l| l.rule_pattern_edits.clear());
    save_rules(state);
}

fn edit_rule(
    state: &SharedState,
    index: usize,
    edit: impl FnOnce(&mut crate::gui::app_state::UISettingsRule),
) {
    {
        let mut st = state.borrow_mut();
        let Some(rule) = st.ui_settings.rules.get_mut(index) else {
            tracing::debug!("Ignoring an edit of rule {index}, there's no such rule");
            return;
        };
        edit(rule);
    }
    save_rules(state);
}

fn add_rule(state: &SharedState) {
    with_local(|l| {
        l.draft.get_or_insert_with(Draft::default);
    });
    refresh(state);
}

// an invalid pattern stays on screen (in red) but never gets saved.
// Returns whether it was saved
fn rule_pattern_edited(state: &SharedState, index: usize, text: &str) -> bool {
    let pattern = text.trim().to_string();
    let Some(mut candidate) = state.borrow().ui_settings.rules.get(index).cloned() else {
        return false;
    };
    candidate.url_pattern = pattern.clone();
    if !candidate.has_valid_matcher() {
        with_local(|l| l.rule_pattern_edits.insert(index, text.to_string()));
        refresh(state);
        return false;
    }
    with_local(|l| l.rule_pattern_edits.remove(&index));
    edit_rule(state, index, |rule| rule.url_pattern = pattern);
    true
}

fn rule_source_picked(state: &SharedState, index: usize, source_app: String) {
    edit_rule(state, index, |rule| rule.source_app = source_app);
}

fn rule_target_picked(state: &SharedState, index: usize, target: Option<String>) {
    edit_rule(state, index, |rule| rule.opener = retarget(&rule.opener, target));
}

fn set_rule_private(state: &SharedState, index: usize, private: bool) {
    edit_rule(state, index, |rule| {
        if let Some(o) = rule.opener.as_mut() {
            o.incognito = private;
        }
    });
}

// the copy goes right below
fn duplicate_rule(state: &SharedState, index: usize) {
    {
        let mut st = state.borrow_mut();
        let rules = &mut st.ui_settings.rules;
        let Some(copy) = rules.get(index).cloned() else {
            return;
        };
        rules.insert(index + 1, copy);
    }
    rules_reshaped(state);
}

fn delete_rule(state: &SharedState, index: usize) {
    {
        let mut st = state.borrow_mut();
        let rules = &mut st.ui_settings.rules;
        if index >= rules.len() {
            return;
        }
        rules.remove(index);
    }
    rules_reshaped(state);
}

// `to` is the insertion point in the original list
fn move_rule(state: &SharedState, from: usize, to: usize) {
    let moved = move_item(&mut state.borrow_mut().ui_settings.rules, from, to);
    if moved {
        rules_reshaped(state);
    }
}

// saves the draft as a real rule once it's complete, otherwise just shows its new state
fn update_draft(state: &SharedState, edit: impl FnOnce(&mut Draft)) {
    let completed = with_local(|l| {
        let draft = l.draft.get_or_insert_with(Draft::default);
        edit(draft);
        if draft.is_complete() {
            l.draft.take()
        } else {
            None
        }
    });
    match completed {
        Some(draft) => {
            clear_slint_draft_pattern(state);
            state.borrow_mut().ui_settings.rules.push(draft.as_rule());
            rules_reshaped(state);
        }
        None => refresh(state),
    }
}

// the Slint draft field keeps its own text, the native one clears itself once the draft is gone
fn clear_slint_draft_pattern(state: &SharedState) {
    if let Some(win) = settings_window(state) {
        win.set_draft_pattern(SharedString::new());
    }
}

fn draft_pattern_edited(state: &SharedState, text: &str) {
    update_draft(state, |d| {
        d.pattern = text.to_string();
        d.show_invalid = false;
    });
}

// from now on an invalid pattern shows as invalid
fn draft_pattern_committed(state: &SharedState) {
    if with_local(|l| l.draft.is_some()) {
        update_draft(state, |d| d.show_invalid = true);
    }
}

fn draft_source_picked(state: &SharedState, source_app: String) {
    update_draft(state, |d| d.source_app = source_app);
}

fn draft_target_picked(state: &SharedState, target: Option<String>) {
    update_draft(state, |d| {
        d.opener = Some(target);
        d.show_invalid = true;
    });
}

fn discard_draft(state: &SharedState) {
    with_local(|l| l.draft = None);
    clear_slint_draft_pattern(state);
    refresh(state);
}

fn test_url_edited(state: &SharedState, text: &str) {
    with_local(|l| l.test_url = text.to_string());
    refresh(state);
}

// `source` is an index into the source options
fn test_source_picked(state: &SharedState, source: usize) {
    with_local(|l| {
        if source < l.source_ids.len() {
            l.test_source = source;
        }
    });
    refresh(state);
}

fn save_default_opener(state: &SharedState, edit: impl FnOnce(&mut Option<UIProfileAndIncognito>)) {
    let default_opener = {
        let mut st = state.borrow_mut();
        let opener = &mut st.ui_settings.default_opener;
        edit(opener);
        opener.clone()
    };
    send(state, MessageToMain::SaveConfigDefaultOpener(default_opener));
    refresh(state);
}

fn fallback_target_picked(state: &SharedState, target: Option<String>) {
    save_default_opener(state, |opener| *opener = retarget(opener, target));
}

fn set_fallback_private(state: &SharedState, private: bool) {
    save_default_opener(state, |opener| {
        if let Some(o) = opener.as_mut() {
            o.incognito = private;
        }
    });
}

fn target_id(index: i32) -> Option<Option<String>> {
    let index = usize::try_from(index).ok()?;
    with_local(|l| l.target_ids.get(index).cloned())
}

fn source_id(index: i32) -> Option<String> {
    let index = usize::try_from(index).ok()?;
    with_local(|l| l.source_ids.get(index).cloned())
}

// ---- General

fn save_visual_settings(state: &SharedState) {
    let settings = state.borrow().ui_settings.visual_settings.clone();
    send(state, MessageToMain::SaveConfigUISettings(settings));
    refresh(state);
}

fn set_theme(state: &SharedState, theme: ConfiguredTheme) {
    state.borrow_mut().ui_settings.visual_settings.theme = theme;
    app::apply_theme(state);
    save_visual_settings(state);
}

fn set_show_hotkeys(state: &SharedState, value: bool) {
    state.borrow_mut().ui_settings.visual_settings.show_hotkeys = value;
    app::refresh_main_window_model(state);
    save_visual_settings(state);
}

fn set_quit_on_lost_focus(state: &SharedState, value: bool) {
    state
        .borrow_mut()
        .ui_settings
        .visual_settings
        .quit_on_lost_focus = value;
    save_visual_settings(state);
}

fn set_unwrap_urls(state: &SharedState, value: bool) {
    let settings = {
        let mut st = state.borrow_mut();
        st.ui_settings.behavioral_settings.unwrap_urls = value;
        st.ui_settings.behavioral_settings.clone()
    };
    send(state, MessageToMain::SaveConfigUIBehavioralSettings(settings));
    refresh(state);
}

fn set_run_in_background(state: &SharedState, value: bool) {
    state
        .borrow_mut()
        .ui_settings
        .behavioral_settings
        .run_in_background = value;
    send(state, MessageToMain::SetRunInBackground(value));
    refresh(state);
}

fn set_native_popup(state: &SharedState, value: bool) {
    state.borrow_mut().ui_settings.visual_settings.native_popup = value;
    // the next link shows the other popup (see app::show_popup_for_url), and the one up now goes
    // away rather than lingering next to it
    if state.borrow().popup_visible() {
        app::hide_popup(state);
    }
    save_visual_settings(state);
}

// ------------------------------------------------------------------------------------ Native

// the row an event names, None (logged) when the list changed since the state it was sent from
#[cfg(target_os = "macos")]
fn current_row(list: RowList, row: usize, row_id: &str) -> Option<RowMeta> {
    let meta = row_meta(list, row).filter(|meta| meta.id == row_id);
    if meta.is_none() {
        tracing::info!("Ignoring an event for {list:?} row {row} ({row_id}), the list changed");
    }
    meta
}

// a target_id the native window sent: None when it isn't one of the options last sent,
// Some(None) for "Show the picker"
#[cfg(target_os = "macos")]
fn known_target(target_id: &str) -> Option<Option<String>> {
    let target = (!target_id.is_empty()).then(|| target_id.to_string());
    let known = with_local(|l| l.target_ids.contains(&target));
    if !known {
        tracing::info!("Ignoring the unknown target {target_id:?}");
    }
    known.then_some(target)
}

// the position of a source_id the native window sent among the options last sent
#[cfg(target_os = "macos")]
fn known_source(source_id: &str) -> Option<usize> {
    let index = with_local(|l| l.source_ids.iter().position(|id| id == source_id));
    if index.is_none() {
        tracing::info!("Ignoring the unknown source {source_id:?}");
    }
    index
}

// the native Settings' events (except settings_closed/about_closed, see app), mapped onto the
// same handlers as the Slint window's callbacks
#[cfg(target_os = "macos")]
pub fn handle_native_event(state: &SharedState, event: SettingsEvent) {
    use SettingsEvent as E;
    match event {
        E::SettingsClosed | E::AboutClosed => {}
        E::SetDefaultBrowser => set_default_browser(state),

        E::BrowsersFilterChanged { text } => filter_changed(state, &text),
        E::SetShowHidden { show } => set_show_hidden(state, show),
        E::Rescan => send(state, MessageToMain::Refresh),
        E::HideNonBrowsers => hide_non_browsers(state),
        E::DismissNonBrowserBanner => dismiss_non_browser_banner(state),
        E::ToggleRowVisibility { list, row, row_id } => {
            if let Some(meta) = current_row(list, row, &row_id) {
                toggle_visibility(state, &meta);
            }
        }
        E::SetGroupCollapsed { app_id, collapsed } => {
            set_group_collapsed(state, &app_id, collapsed)
        }
        E::RowMenu {
            list,
            row,
            row_id,
            action,
        } => {
            let Some(meta) = current_row(list, row, &row_id) else {
                return;
            };
            // only what the row's menu offered, e.g. no moving while filtering
            if !settings_model::row_menu(meta.kind, meta.draggable).contains(&action) {
                tracing::info!("Ignoring {action:?} for row {row_id}, its menu doesn't have it");
                return;
            }
            match list {
                RowList::Rows => row_menu_picked(state, row, action),
                RowList::SpecialRows => special_menu_picked(state, row),
            }
        }
        E::MoveBrowserRow { row, row_id, to } => {
            let Some(meta) = current_row(RowList::Rows, row, &row_id) else {
                return;
            };
            if meta.draggable {
                move_rows(state, row, snap_browser_drop(row, to));
            }
        }

        E::AddRule => add_rule(state),
        E::RulePatternEdited { index, pattern } => {
            rule_pattern_edited(state, index, &pattern);
        }
        E::RuleSourcePicked { index, source_id } => {
            if known_source(&source_id).is_some() {
                rule_source_picked(state, index, source_id);
            }
        }
        E::RuleTargetPicked { index, target_id } => {
            if let Some(target) = known_target(&target_id) {
                rule_target_picked(state, index, target);
            }
        }
        E::RulePrivateSet { index, private } => set_rule_private(state, index, private),
        E::RuleDuplicate { index } => duplicate_rule(state, index),
        E::RuleDelete { index } => delete_rule(state, index),
        E::MoveRule { from, to } => move_rule(state, from, to),
        E::DraftPatternEdited { text } => draft_pattern_edited(state, &text),
        E::DraftPatternCommitted => draft_pattern_committed(state),
        E::DraftSourcePicked { source_id } => {
            if known_source(&source_id).is_some() {
                draft_source_picked(state, source_id);
            }
        }
        E::DraftTargetPicked { target_id } => {
            if let Some(target) = known_target(&target_id) {
                draft_target_picked(state, target);
            }
        }
        E::DraftDiscarded => discard_draft(state),
        E::TestUrlEdited { text } => test_url_edited(state, &text),
        E::TestSourcePicked { source_id } => {
            if let Some(index) = known_source(&source_id) {
                test_source_picked(state, index);
            }
        }
        E::FallbackTargetPicked { target_id } => {
            if let Some(target) = known_target(&target_id) {
                fallback_target_picked(state, target);
            }
        }
        E::FallbackPrivateSet { private } => set_fallback_private(state, private),

        E::SetTheme { theme } => set_theme(state, theme.theme()),
        E::SetNativePopup { value } => set_native_popup(state, value),
        E::SetShowHotkeys { value } => set_show_hotkeys(state, value),
        E::SetQuitOnLostFocus { value } => set_quit_on_lost_focus(state, value),
        E::SetUnwrapUrls { value } => set_unwrap_urls(state, value),
        E::SetRunInBackground { value } => set_run_in_background(state, value),
    }
}

// ------------------------------------------------------------------------------------ Slint wiring

fn wire(state: &SharedState, win: &SettingsWindow) {
    // ---- sidebar
    {
        let state = state.clone();
        win.on_set_default_browser(move || set_default_browser(&state));
    }

    // ---- Browsers
    win.on_refresh(app::forward_handler(state, || MessageToMain::Refresh));
    {
        let state = state.clone();
        win.on_filter_changed(move |text| filter_changed(&state, &text));
    }
    {
        let state = state.clone();
        win.on_show_hidden_toggled(move || {
            let show = !with_local(|l| l.show_hidden);
            set_show_hidden(&state, show);
        });
    }
    {
        let state = state.clone();
        win.on_hide_non_browsers(move || hide_non_browsers(&state));
    }
    {
        let state = state.clone();
        win.on_dismiss_non_browser_banner(move || dismiss_non_browser_banner(&state));
    }
    {
        let state = state.clone();
        win.on_browser_visibility_toggled(move |row| {
            if let Some(meta) = usize::try_from(row)
                .ok()
                .and_then(|row| row_meta(RowList::Rows, row))
            {
                toggle_visibility(&state, &meta);
            }
        });
    }
    {
        let state = state.clone();
        win.on_special_visibility_toggled(move |row| {
            if let Some(meta) = usize::try_from(row)
                .ok()
                .and_then(|row| row_meta(RowList::SpecialRows, row))
            {
                toggle_visibility(&state, &meta);
            }
        });
    }
    {
        let state = state.clone();
        win.on_browser_group_toggled(move |row| {
            let Some(meta) = usize::try_from(row)
                .ok()
                .and_then(|row| row_meta(RowList::Rows, row))
            else {
                return;
            };
            let collapsed = with_local(|l| l.collapsed.contains(&meta.app_id));
            set_group_collapsed(&state, &meta.app_id, !collapsed);
        });
    }
    {
        let state = state.clone();
        win.on_browser_menu_picked(move |row, action| {
            if let (Ok(row), Some(action)) =
                (usize::try_from(row), RowAction::from_slint_index(action))
            {
                row_menu_picked(&state, row, action);
            }
        });
    }
    {
        let state = state.clone();
        win.on_special_menu_picked(move |row, _action| {
            if let Ok(row) = usize::try_from(row) {
                special_menu_picked(&state, row);
            }
        });
    }
    win.on_snap_browser_drop(|from, raw| {
        snap_browser_drop(from.max(0) as usize, raw.max(0) as usize) as i32
    });
    {
        let state = state.clone();
        win.on_browser_rows_dropped(move |from, to| {
            if from >= 0 && to >= 0 {
                move_rows(&state, from as usize, to as usize);
            }
        });
    }

    // ---- Rules
    {
        let state = state.clone();
        win.on_add_rule(move || add_rule(&state));
    }
    {
        let state = state.clone();
        win.on_rule_pattern_edited(move |index, text| {
            usize::try_from(index).is_ok_and(|index| rule_pattern_edited(&state, index, &text))
        });
    }
    {
        let state = state.clone();
        win.on_rule_source_picked(move |index, source| {
            if let (Ok(index), Some(source_app)) = (usize::try_from(index), source_id(source)) {
                rule_source_picked(&state, index, source_app);
            }
        });
    }
    {
        let state = state.clone();
        win.on_rule_target_picked(move |index, target| {
            if let (Ok(index), Some(target)) = (usize::try_from(index), target_id(target)) {
                rule_target_picked(&state, index, target);
            }
        });
    }
    {
        let state = state.clone();
        win.on_rule_private_toggled(move |index| {
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            let private = state
                .borrow()
                .ui_settings
                .rules
                .get(index)
                .and_then(|rule| rule.opener.as_ref())
                .map(|o| !o.incognito);
            if let Some(private) = private {
                set_rule_private(&state, index, private);
            }
        });
    }
    {
        let state = state.clone();
        win.on_rule_menu_picked(move |index, action| {
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            if action == 0 {
                duplicate_rule(&state, index);
            } else {
                delete_rule(&state, index);
            }
        });
    }
    {
        let state = state.clone();
        win.on_rules_dropped(move |from, to| {
            move_rule(&state, from.max(0) as usize, to.max(0) as usize);
        });
    }
    {
        let state = state.clone();
        win.on_draft_pattern_edited(move |text| draft_pattern_edited(&state, &text));
    }
    {
        let state = state.clone();
        win.on_draft_pattern_committed(move || draft_pattern_committed(&state));
    }
    {
        let state = state.clone();
        win.on_draft_source_picked(move |source| {
            if let Some(source_app) = source_id(source) {
                draft_source_picked(&state, source_app);
            }
        });
    }
    {
        let state = state.clone();
        win.on_draft_target_picked(move |target| {
            if let Some(target) = target_id(target) {
                draft_target_picked(&state, target);
            }
        });
    }
    {
        let state = state.clone();
        win.on_draft_discarded(move || discard_draft(&state));
    }
    {
        let state = state.clone();
        win.on_test_url_edited(move |text| test_url_edited(&state, &text));
    }
    {
        let state = state.clone();
        win.on_test_source_picked(move |source| {
            if let Ok(source) = usize::try_from(source) {
                test_source_picked(&state, source);
            }
        });
    }
    {
        let state = state.clone();
        win.on_fallback_target_picked(move |target| {
            if let Some(target) = target_id(target) {
                fallback_target_picked(&state, target);
            }
        });
    }
    {
        let state = state.clone();
        win.on_fallback_private_toggled(move || {
            let private = state
                .borrow()
                .ui_settings
                .default_opener
                .as_ref()
                .is_some_and(|o| !o.incognito);
            set_fallback_private(&state, private);
        });
    }

    // ---- General
    {
        let state = state.clone();
        win.on_theme_changed(move |choice| set_theme(&state, choice_to_theme(choice)));
    }
    {
        let state = state.clone();
        win.on_show_hotkeys_changed(move |v| set_show_hotkeys(&state, v));
    }
    {
        let state = state.clone();
        win.on_quit_on_lost_focus_changed(move |v| set_quit_on_lost_focus(&state, v));
    }
    {
        let state = state.clone();
        win.on_unwrap_urls_changed(move |v| set_unwrap_urls(&state, v));
    }
    {
        let state = state.clone();
        win.on_run_in_background_changed(move |v| set_run_in_background(&state, v));
    }
    {
        let state = state.clone();
        win.on_native_popup_changed(move |v| set_native_popup(&state, v));
    }
}
