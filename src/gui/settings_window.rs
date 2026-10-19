use std::cell::RefCell;
use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use slint::{Color, ComponentHandle, Image, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::gui::app::{self, SharedState};
use crate::gui::app_state::{UIBrowser, UIProfileAndIncognito, UISettingsRule};
use crate::gui::generated::{
    Palette as StdWidgetPalette, SettingsBrowserRow, SettingsRuleRow, SettingsWindow, ThemeChoice,
};
use crate::gui::icon_loader::IconLoader;
use crate::gui::theme::ColorScheme;
use crate::gui::ui_util::sync_vec_model;
use crate::utils::ConfiguredTheme;
use crate::{MessageToMain, RuleTestOutcome, paths, profile_order, utils};

const VERSION: &str = env!("CARGO_PKG_VERSION");

// apps that commonly send links, offered as a rule's source app even when they aren't link
// openers themselves (so they never show up among the found apps)
#[cfg(target_os = "macos")]
const KNOWN_SOURCE_APPS: &[(&str, &str)] = &[
    ("Mail", "com.apple.mail"),
    ("Messages", "com.apple.MobileSMS"),
    ("Notes", "com.apple.Notes"),
    ("Calendar", "com.apple.iCal"),
    ("Terminal", "com.apple.Terminal"),
    ("Outlook", "com.microsoft.Outlook"),
    ("Microsoft Teams", "com.microsoft.teams2"),
    ("Discord", "com.hnc.Discord"),
];
#[cfg(not(target_os = "macos"))]
const KNOWN_SOURCE_APPS: &[(&str, &str)] = &[];

const LETTER_COLORS: &[(u8, u8, u8)] = &[
    (0x5b, 0x8d, 0xef),
    (0xe5, 0x73, 0x4a),
    (0x3f, 0xb2, 0x7f),
    (0xa6, 0x6d, 0xe8),
    (0xe2, 0xa9, 0x3b),
    (0xe0, 0x5c, 0x8a),
    (0x4b, 0xb3, 0xc4),
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum RowKind {
    Single,
    Header,
    Child,
}

impl RowKind {
    fn as_int(self) -> i32 {
        match self {
            RowKind::Single => 0,
            RowKind::Header => 1,
            RowKind::Child => 2,
        }
    }
}

// what a row of the Browsers list stands for
#[derive(Clone, Debug)]
struct RowMeta {
    kind: RowKind,
    // every profile id the row moves/hides together, in popup order.
    // For a header that includes the group's hidden profiles too, even when they aren't shown.
    ids: Vec<String>,
    app_id: String,
    hidden: bool,
}

// the rule being added on the Rules page, kept out of the saved rules until it's complete
#[derive(Clone, Debug, Default)]
struct Draft {
    pattern: String,
    source_app: String,
    // None until a target is picked; Some(None) is "Show the picker"
    opener: Option<Option<String>>,
    // only flag a bad pattern once the user is done typing it
    show_invalid: bool,
}

impl Draft {
    fn as_rule(&self) -> UISettingsRule {
        UISettingsRule {
            source_app: self.source_app.clone(),
            url_pattern: self.pattern.trim().to_string(),
            opener: self
                .opener
                .clone()
                .flatten()
                .map(|profile| UIProfileAndIncognito {
                    profile,
                    incognito: false,
                }),
        }
    }

    fn is_complete(&self) -> bool {
        self.opener.is_some() && self.as_rule().has_valid_matcher()
    }

    fn pattern_is_invalid(&self) -> bool {
        let pattern = self.pattern.trim();
        !pattern.is_empty() && !crate::url_rule::is_valid_pattern(pattern)
    }
}

// window-local state that only Settings cares about
#[derive(Default)]
struct Local {
    filter: String,
    show_hidden: bool,
    // app ids of collapsed groups
    collapsed: HashSet<String>,
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
    source_labels: Vec<String>,
    default_browser_poll: Option<Timer>,
}

thread_local! {
    static LOCAL: RefCell<Local> = RefCell::default();
}

fn with_local<R>(f: impl FnOnce(&mut Local) -> R) -> R {
    LOCAL.with(|local| f(&mut local.borrow_mut()))
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

pub fn open(state: &SharedState) {
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
    win.set_version_text(format!("Version {}", VERSION).into());
    win.set_show_quit_on_lost_focus(cfg!(target_os = "macos"));
    #[cfg(target_os = "macos")]
    win.set_show_native_popup(crate::macos::native_popup::is_supported());
    win.set_is_default_browser(utils::is_default_web_browser());
    win.set_debug_hold_drag(
        cfg!(debug_assertions) && std::env::var_os("BROWSERS_DEBUG_HOLD_DRAG").is_some(),
    );
    if cfg!(debug_assertions) {
        // lets screenshots start on any page
        if let Some(page) = std::env::var("BROWSERS_DEBUG_SETTINGS_PAGE")
            .ok()
            .and_then(|p| p.parse().ok())
        {
            win.set_page(page);
        }
    }

    with_local(|local| *local = Local::default());
    state.borrow_mut().settings_window = Some(win.clone_strong());

    app::apply_theme(state);
    wire(state, &win);
    refresh(state);

    win.window().show().ok();

    // otherwise this renders behind the always-on-top main popup - see app::make_window_floating
    app::make_window_floating(&win);

    app::clear_on_close(&win, state, |st| st.settings_window = None);
}

fn settings_window(state: &SharedState) -> Option<SettingsWindow> {
    state
        .borrow()
        .settings_window
        .as_ref()
        .map(|w| w.clone_strong())
}

pub fn refresh(state: &SharedState) {
    let Some(win) = settings_window(state) else {
        return;
    };
    refresh_browsers(state, &win);
    refresh_rules(state, &win);
    refresh_general(state, &win);
}

// ------------------------------------------------------------------------------------ Browsers

struct Entry<'a> {
    browser: &'a UIBrowser,
    hidden: bool,
}

fn matches_filter(text: &str, filter: &str) -> bool {
    filter.is_empty() || text.to_lowercase().contains(filter)
}

fn letter_color(name: &str) -> Color {
    let hash = name
        .bytes()
        .fold(0usize, |acc, b| acc.wrapping_mul(31).wrapping_add(b as usize));
    let (r, g, b) = LETTER_COLORS[hash % LETTER_COLORS.len()];
    Color::from_rgb_u8(r, g, b)
}

fn first_letter(name: &str) -> String {
    name.chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default()
}

fn loaded(image: Image) -> Option<Image> {
    (image.size().width > 0).then_some(image)
}

// a single app or one app profile as a list row
fn entry_row(
    entry: &Entry,
    kind: RowKind,
    icons: &mut IconLoader,
    default_profile: Option<&str>,
    draggable: bool,
) -> SettingsBrowserRow {
    let b = entry.browser;
    let (name, detail) = match kind {
        RowKind::Child => (b.profile_name.clone(), String::new()),
        _ if b.supports_profiles => (b.browser_name.clone(), b.profile_name.clone()),
        _ => (b.browser_name.clone(), String::new()),
    };
    let icon = match kind {
        // a profile without a picture of its own gets a letter instead of repeating the app icon
        RowKind::Child => loaded(icons.load(&b.profile_icon_path)),
        _ => loaded(icons.load(&b.icon_path)),
    };
    SettingsBrowserRow {
        kind: kind.as_int(),
        letter: first_letter(&name).into(),
        letter_color: letter_color(&name),
        name: name.into(),
        detail: detail.into(),
        has_icon: icon.is_some(),
        icon: icon.unwrap_or_default(),
        hidden: entry.hidden,
        is_default: default_profile == Some(b.unique_id.as_str()),
        collapsed: false,
        draggable,
    }
}

// splits the full order into one group per app: an app's profiles are pulled together at the
// spot of its first one (see profile_order::group_by_app), even if the saved order split them up.
fn group_entries<'a>(entries: Vec<Entry<'a>>) -> Vec<Vec<Entry<'a>>> {
    let entries = profile_order::group_by_app(entries, |e| e.browser.unique_app_id.to_string());
    let mut groups: Vec<Vec<Entry>> = vec![];
    for entry in entries {
        match groups.last_mut() {
            Some(group)
                if entry.browser.supports_profiles
                    && group[0].browser.supports_profiles
                    && group[0].browser.unique_app_id == entry.browser.unique_app_id =>
            {
                group.push(entry)
            }
            _ => groups.push(vec![entry]),
        }
    }
    groups
}

fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

// "Slack", "Slack and Zoom", "Slack, Zoom and Figma", "Slack, Zoom, Figma and 2 more"
fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [only] => only.clone(),
        [first, second, third, rest @ ..] if !rest.is_empty() => {
            format!("{first}, {second}, {third} and {} more", rest.len())
        }
        [rest @ .., last] => format!("{} and {}", rest.join(", "), last),
    }
}

fn relative_time(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    match secs {
        0..60 => "just now".to_string(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86400 => format!("{} ago", plural((secs / 3600) as usize, "hour", "hours")),
        _ => format!("{} ago", plural((secs / 86400) as usize, "day", "days")),
    }
}

// when the installed apps were last scanned, from the scan's cache file
fn last_scan_time() -> Option<SystemTime> {
    std::fs::metadata(paths::get_cache_root_dir().join("installed_browsers.json"))
        .and_then(|m| m.modified())
        .ok()
}

fn refresh_browsers(state: &SharedState, win: &SettingsWindow) {
    let mut st = state.borrow_mut();
    let mut icons = std::mem::take(&mut st.icons);

    let (filter, show_hidden, collapsed) = with_local(|l| {
        (
            l.filter.trim().to_lowercase(),
            l.show_hidden,
            l.collapsed.clone(),
        )
    });
    let reorder_enabled = filter.is_empty();
    let default_profile = st
        .ui_settings
        .default_opener
        .as_ref()
        .map(|o| o.profile.clone());
    let default_profile = default_profile.as_deref();

    let visible = &st.browsers;
    let hidden = &st.restorable_app_profiles;
    let ids = |list: &[UIBrowser]| -> Vec<String> {
        list.iter()
            .filter(|b| !b.has_priority_ordering())
            .map(|b| b.unique_id.clone())
            .collect()
    };
    let order =
        profile_order::full_order(&ids(visible), &ids(hidden), &st.ui_settings.profile_order);
    let find = |id: &str| -> Option<Entry> {
        visible
            .iter()
            .find(|b| b.unique_id == id)
            .map(|browser| Entry {
                browser,
                hidden: false,
            })
            .or_else(|| {
                hidden
                    .iter()
                    .find(|b| b.unique_id == id)
                    .map(|browser| Entry {
                        browser,
                        hidden: true,
                    })
            })
    };

    let mut rows = vec![];
    let mut metas = vec![];
    for group in group_entries(order.iter().filter_map(|id| find(id)).collect()) {
        let first = group[0].browser;
        let shown = |e: &&Entry| show_hidden || !e.hidden;

        if group.len() == 1 {
            let entry = &group[0];
            let text = format!("{} {}", first.browser_name, first.profile_name);
            if !shown(&entry) || !matches_filter(&text, &filter) {
                continue;
            }
            rows.push(entry_row(
                entry,
                RowKind::Single,
                &mut icons,
                default_profile,
                reorder_enabled,
            ));
            metas.push(RowMeta {
                kind: RowKind::Single,
                ids: vec![first.unique_id.clone()],
                app_id: first.unique_app_id.clone(),
                hidden: entry.hidden,
            });
            continue;
        }

        let app_matches = matches_filter(&first.browser_name, &filter);
        let children: Vec<&Entry> = group
            .iter()
            .filter(shown)
            .filter(|e| app_matches || matches_filter(&e.browser.profile_name, &filter))
            .collect();
        if children.is_empty() {
            continue;
        }

        let hidden_count = group.iter().filter(|e| e.hidden).count();
        let mut detail = plural(group.len(), "profile", "profiles");
        if hidden_count > 0 && !show_hidden {
            detail = format!("{detail} · {hidden_count} hidden");
        }
        let is_collapsed = collapsed.contains(&first.unique_app_id) && filter.is_empty();
        let icon = loaded(icons.load(&first.icon_path));
        rows.push(SettingsBrowserRow {
            kind: RowKind::Header.as_int(),
            name: first.browser_name.clone().into(),
            detail: detail.into(),
            has_icon: icon.is_some(),
            icon: icon.unwrap_or_default(),
            letter: first_letter(&first.browser_name).into(),
            letter_color: letter_color(&first.browser_name),
            hidden: hidden_count == group.len(),
            is_default: false,
            collapsed: is_collapsed,
            draggable: reorder_enabled,
        });
        metas.push(RowMeta {
            kind: RowKind::Header,
            ids: group.iter().map(|e| e.browser.unique_id.clone()).collect(),
            app_id: first.unique_app_id.clone(),
            hidden: hidden_count == group.len(),
        });

        if is_collapsed {
            continue;
        }
        for entry in children {
            rows.push(entry_row(
                entry,
                RowKind::Child,
                &mut icons,
                default_profile,
                reorder_enabled,
            ));
            metas.push(RowMeta {
                kind: RowKind::Child,
                ids: vec![entry.browser.unique_id.clone()],
                app_id: entry.browser.unique_app_id.clone(),
                hidden: entry.hidden,
            });
        }
    }

    // apps that only open their own links: always first in the popup, so not draggable
    let mut special_rows = vec![];
    let mut special_metas = vec![];
    let special = visible
        .iter()
        .map(|browser| Entry {
            browser,
            hidden: false,
        })
        .chain(hidden.iter().map(|browser| Entry {
            browser,
            hidden: true,
        }))
        .filter(|e| e.browser.has_priority_ordering())
        .filter(|e| show_hidden || !e.hidden)
        .filter(|e| {
            let text = format!("{} {}", e.browser.browser_name, e.browser.profile_name);
            matches_filter(&text, &filter)
        });
    for entry in special {
        special_rows.push(entry_row(
            &entry,
            RowKind::Single,
            &mut icons,
            default_profile,
            false,
        ));
        special_metas.push(RowMeta {
            kind: RowKind::Single,
            ids: vec![entry.browser.unique_id.clone()],
            app_id: entry.browser.unique_app_id.clone(),
            hidden: entry.hidden,
        });
    }

    // the "these aren't web browsers" banner lists every visible app that only opens its own links
    let mut banner_names: Vec<String> = vec![];
    for b in visible.iter().filter(|b| b.has_priority_ordering()) {
        if !banner_names.contains(&b.browser_name) {
            banner_names.push(b.browser_name.clone());
        }
    }
    if st.ui_settings.visual_settings.non_browser_banner_dismissed {
        banner_names.clear();
    }

    let app_count = visible
        .iter()
        .chain(hidden.iter())
        .map(|b| b.unique_app_id.as_str())
        .collect::<HashSet<_>>()
        .len();
    let found_text = match last_scan_time().and_then(|t| t.elapsed().ok()) {
        Some(elapsed) => format!(
            "Found {} · checked {}",
            plural(app_count, "app", "apps"),
            relative_time(elapsed)
        ),
        None => format!("Found {}", plural(app_count, "app", "apps")),
    };
    let hidden_count = hidden.len();

    st.icons = icons;
    drop(st);

    with_local(|l| {
        l.rows = metas;
        l.special_rows = special_metas;
        l.order = order;
    });

    win.set_show_hidden(show_hidden);
    win.set_hidden_count(hidden_count as i32);
    win.set_reorder_enabled(reorder_enabled);
    win.set_order_hint(
        if reorder_enabled {
            "Drag to reorder · 1–9 follow this order"
        } else {
            "Clear the filter to reorder"
        }
        .into(),
    );
    win.set_banner_count(banner_names.len() as i32);
    win.set_banner_names(join_names(&banner_names).into());
    win.set_found_text(found_text.into());
    sync_vec_model(&win.get_browser_rows(), rows);
    sync_vec_model(&win.get_special_rows(), special_rows);
}

// where a row dragged from `from` lands when released at insertion point `raw` (0..=rows).
// A profile stays inside its own group; an app or a whole group only lands between other apps
// and groups, never between someone else's profiles.
fn snap_drop_index(kinds: &[RowKind], from: usize, raw: usize) -> usize {
    let n = kinds.len();
    let raw = raw.min(n);
    if from >= n {
        return raw;
    }
    if kinds[from] == RowKind::Child {
        let first = (0..from)
            .rev()
            .find(|&i| kinds[i] != RowKind::Child)
            .map_or(0, |header| header + 1);
        let end = (from..n).find(|&i| kinds[i] != RowKind::Child).unwrap_or(n);
        return raw.clamp(first, end);
    }
    (0..=n)
        .filter(|&i| i == n || kinds[i] != RowKind::Child)
        .min_by_key(|&i| i.abs_diff(raw))
        .unwrap_or(raw)
}

// the end (exclusive) of the block of rows starting at `from`: a header and its shown profiles,
// or just the row itself
fn block_end(kinds: &[RowKind], from: usize) -> usize {
    if kinds.get(from) == Some(&RowKind::Header) {
        (from + 1..kinds.len())
            .find(|&i| kinds[i] != RowKind::Child)
            .unwrap_or(kinds.len())
    } else {
        from + 1
    }
}

// turns dropping row `from` at insertion point `to` into "move these ids before this id" (None =
// to the very end), or None when the drop doesn't move anything.
// `order` is every orderable id in popup order, hidden ones included.
fn drop_target(
    rows: &[RowMeta],
    order: &[String],
    from: usize,
    to: usize,
) -> Option<(Vec<String>, Option<String>)> {
    let kinds: Vec<RowKind> = rows.iter().map(|r| r.kind).collect();
    let row = rows.get(from)?;
    if (from..=block_end(&kinds, from)).contains(&to) {
        return None;
    }

    if row.kind == RowKind::Child {
        let header = (0..from).rev().find(|&i| kinds[i] != RowKind::Child)?;
        let group_end = block_end(&kinds, header);
        if to < group_end {
            return Some((row.ids.clone(), rows.get(to)?.ids.first().cloned()));
        }
        // to the end of the group: right after its last profile, which may be a hidden one
        let last = rows[header].ids.last()?;
        let successor = order
            .iter()
            .position(|id| id == last)
            .and_then(|i| order.get(i + 1))
            .cloned();
        return Some((row.ids.clone(), successor));
    }

    let before = rows.get(to).and_then(|r| r.ids.first().cloned());
    Some((row.ids.clone(), before))
}

fn send(state: &SharedState, message: MessageToMain) {
    let _ = state.borrow().main_sender.send(message);
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

fn row_menu_picked(state: &SharedState, row: usize, action: i32) {
    let (meta, rows, order) =
        with_local(|l| (l.rows.get(row).cloned(), l.rows.clone(), l.order.clone()));
    let Some(meta) = meta else {
        return;
    };
    match action {
        0 | 1 if meta.kind == RowKind::Child => {
            let kinds: Vec<RowKind> = rows.iter().map(|r| r.kind).collect();
            let Some(header) = (0..row).rev().find(|&i| kinds[i] != RowKind::Child) else {
                return;
            };
            let target = if action == 0 {
                header + 1
            } else {
                block_end(&kinds, header)
            };
            move_rows(state, row, target);
        }
        0 => {
            let first = order.first().cloned();
            if first.as_ref() != meta.ids.first() {
                send(state, MessageToMain::MoveAppProfilesBefore(meta.ids, first));
            }
        }
        1 => send(state, MessageToMain::MoveAppProfilesBefore(meta.ids, None)),
        _ => toggle_default_profile(state, &meta.ids[0]),
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

// ------------------------------------------------------------------------------------ Rules

// "Work · Chrome" for a profile, just "Safari" for an app without profiles
fn profile_label(b: &UIBrowser) -> String {
    if b.supports_profiles {
        format!("{} · {}", b.profile_name, b.browser_name)
    } else {
        b.browser_name.clone()
    }
}

// a rule's target as shown in its picker: label, icon, whether it can open privately
fn target_display(
    opener: Option<&str>,
    all: &[&UIBrowser],
    icons: &mut IconLoader,
) -> (String, Option<Image>, bool) {
    let Some(id) = opener else {
        return ("Show the picker".to_string(), None, false);
    };
    match all.iter().find(|b| b.unique_id == id) {
        Some(b) => {
            let icon = loaded(icons.load(&b.profile_icon_path))
                .or_else(|| loaded(icons.load(&b.icon_path)));
            (profile_label(b), icon, b.supports_incognito)
        }
        None => ("Missing app".to_string(), None, false),
    }
}

// moves the item at `from` so it ends up at insertion point `to` (0..=len) of the original list
fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from >= items.len() || to > items.len() || to == from || to == from + 1 {
        return false;
    }
    let item = items.remove(from);
    let to = if to > from { to - 1 } else { to };
    items.insert(to, item);
    true
}

fn draft_help(draft: &Draft) -> String {
    let pattern = draft.pattern.trim();
    if draft.show_invalid && draft.pattern_is_invalid() {
        return "That isn't a site or link pattern - try notion.so or github.com/acme/**"
            .to_string();
    }
    let what = if let Some(prefix) = pattern.strip_suffix("/**") {
        format!("Matches every page under {prefix}.")
    } else if pattern.contains('/') {
        format!("Matches {pattern} exactly - end it with /** to include the pages below.")
    } else if !pattern.is_empty() {
        format!("Matches {pattern} and every page on it.")
    } else if !draft.source_app.is_empty() {
        "Matches every link from this app.".to_string()
    } else {
        "Matches notion.so and every page on it.".to_string()
    };
    format!("{what} Saved once it has a pattern and a target.")
}

fn refresh_rules(state: &SharedState, win: &SettingsWindow) {
    let mut st = state.borrow_mut();
    let mut icons = std::mem::take(&mut st.icons);

    // every profile is a valid target, hidden ones included
    let all: Vec<&UIBrowser> = st
        .browsers
        .iter()
        .chain(st.restorable_app_profiles.iter())
        .collect();
    let hidden_ids: HashSet<&str> = st
        .restorable_app_profiles
        .iter()
        .map(|b| b.unique_id.as_str())
        .collect();
    let mut target_ids: Vec<Option<String>> = vec![None];
    let mut target_options: Vec<SharedString> = vec!["Show the picker".into()];
    for b in &all {
        target_ids.push(Some(b.unique_id.clone()));
        let mut label = profile_label(b);
        if hidden_ids.contains(b.unique_id.as_str()) {
            label.push_str(" (hidden)");
        }
        target_options.push(label.into());
    }

    // "any app", then every found app, then other common link senders, then anything only the
    // saved rules know about
    let mut sources: Vec<(String, String)> = vec![(String::new(), "Any app".to_string())];
    let mut add_source = |id: &str, label: &str| {
        if !id.is_empty() && !sources.iter().any(|(known, _)| known == id) {
            sources.push((id.to_string(), label.to_string()));
        }
    };
    let mut found: Vec<&UIBrowser> = all.clone();
    found.sort_by_key(|b| b.browser_name.to_lowercase());
    for b in found {
        add_source(&b.bundle_id, &b.browser_name);
    }
    for (label, id) in KNOWN_SOURCE_APPS {
        add_source(id, label);
    }
    for rule in &st.ui_settings.rules {
        add_source(&rule.source_app, &rule.source_app);
    }
    let source_label = |id: &str| -> String {
        sources
            .iter()
            .find(|(known, _)| known == id)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| id.to_string())
    };

    let rules = st.ui_settings.rules.clone();
    let rule_rows: Vec<SettingsRuleRow> = rules
        .iter()
        .map(|rule| {
            let opener = rule.opener.as_ref().map(|o| o.profile.as_str());
            let (label, icon, supports_private) = target_display(opener, &all, &mut icons);
            SettingsRuleRow {
                pattern: rule.url_pattern.clone().into(),
                source_label: if rule.source_app.is_empty() {
                    SharedString::new()
                } else {
                    source_label(&rule.source_app).into()
                },
                target_label: label.into(),
                target_has_icon: icon.is_some(),
                target_icon: icon.unwrap_or_default(),
                private: rule.opener.as_ref().is_some_and(|o| o.incognito),
                supports_private,
            }
        })
        .collect();

    let default_opener = st.ui_settings.default_opener.clone();
    let (fallback_label, fallback_icon, fallback_supports_private) = target_display(
        default_opener.as_ref().map(|o| o.profile.as_str()),
        &all,
        &mut icons,
    );

    let (draft, test_url, test_source) = with_local(|l| {
        if l.test_source >= sources.len() {
            l.test_source = 0;
        }
        (l.draft.clone(), l.test_url.clone(), l.test_source)
    });
    let draft_target_label = draft
        .as_ref()
        .and_then(|d| d.opener.clone())
        .map(|opener| target_display(opener.as_deref(), &all, &mut icons).0)
        .unwrap_or_default();

    // test a link
    let (result_kind, result_text, matched_rule) = if test_url.trim().is_empty() {
        (0, String::new(), -1)
    } else {
        let source = Some(sources[test_source].0.as_str()).filter(|s| !s.is_empty());
        match crate::test_opening_rules(&rules, default_opener.is_some(), &test_url, source) {
            RuleTestOutcome::InvalidUrl => (3, "That isn't a link yet".to_string(), -1),
            RuleTestOutcome::MatchedRule(i) => {
                let opener = rules[i].opener.as_ref();
                let destination = match opener {
                    None => "shows the picker".to_string(),
                    Some(o) => {
                        let label = target_display(Some(&o.profile), &all, &mut icons).0;
                        let private = if o.incognito { " (private)" } else { "" };
                        format!("opens in {label}{private}")
                    }
                };
                (1, format!("Rule {} matches → {destination}", i + 1), i as i32)
            }
            RuleTestOutcome::DefaultProfile => {
                let private = if default_opener.as_ref().is_some_and(|o| o.incognito) {
                    " (private)"
                } else {
                    ""
                };
                (
                    2,
                    format!("No rule matches → opens in {fallback_label}{private}"),
                    -1,
                )
            }
            RuleTestOutcome::Picker => (2, "No rule matches → shows the picker".to_string(), -1),
        }
    };

    st.icons = icons;
    drop(st);

    let test_source_label = sources[test_source].1.clone();
    let source_options: Vec<SharedString> = sources.iter().map(|(_, l)| l.into()).collect();
    with_local(|l| {
        l.target_ids = target_ids;
        l.source_ids = sources.iter().map(|(id, _)| id.clone()).collect();
        l.source_labels = sources.iter().map(|(_, label)| label.clone()).collect();
    });

    sync_vec_model(&win.get_target_options(), target_options);
    sync_vec_model(&win.get_source_options(), source_options);
    sync_vec_model(&win.get_rules(), rule_rows);

    win.set_has_draft(draft.is_some());
    if let Some(draft) = &draft {
        win.set_draft_source_label(
            if draft.source_app.is_empty() {
                String::new()
            } else {
                source_label(&draft.source_app)
            }
            .into(),
        );
        win.set_draft_target_label(draft_target_label.into());
        win.set_draft_help(draft_help(draft).into());
        win.set_draft_pattern_invalid(draft.show_invalid && draft.pattern_is_invalid());
    }

    win.set_test_source_label(test_source_label.into());
    win.set_test_result_kind(result_kind);
    win.set_test_result_text(result_text.into());
    win.set_matched_rule(matched_rule);

    win.set_fallback_target_label(fallback_label.into());
    win.set_fallback_target_has_icon(fallback_icon.is_some());
    win.set_fallback_target_icon(fallback_icon.unwrap_or_default());
    win.set_fallback_private(default_opener.as_ref().is_some_and(|o| o.incognito));
    win.set_fallback_supports_private(fallback_supports_private);
}

fn save_rules(state: &SharedState) {
    let rules = state.borrow().ui_settings.rules.clone();
    send(state, MessageToMain::SaveConfigRules(rules));
    refresh(state);
}

fn edit_rule(state: &SharedState, index: i32, edit: impl FnOnce(&mut UISettingsRule)) {
    {
        let mut st = state.borrow_mut();
        let Some(rule) = st.ui_settings.rules.get_mut(index as usize) else {
            return;
        };
        edit(rule);
    }
    save_rules(state);
}

fn target_id(index: i32) -> Option<Option<String>> {
    with_local(|l| l.target_ids.get(index as usize).cloned())
}

fn source_id(index: i32) -> Option<String> {
    with_local(|l| l.source_ids.get(index as usize).cloned())
}

// keeps the private flag when switching to another target that can also open privately
fn retarget(
    opener: &Option<UIProfileAndIncognito>,
    target: Option<String>,
) -> Option<UIProfileAndIncognito> {
    let incognito = opener.as_ref().is_some_and(|o| o.incognito);
    target.map(|profile| UIProfileAndIncognito { profile, incognito })
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
            if let Some(win) = settings_window(state) {
                win.set_draft_pattern(SharedString::new());
            }
            state.borrow_mut().ui_settings.rules.push(draft.as_rule());
            save_rules(state);
        }
        None => refresh(state),
    }
}

// ------------------------------------------------------------------------------------ General

fn refresh_general(state: &SharedState, win: &SettingsWindow) {
    let settings = state.borrow().ui_settings.clone();
    win.set_show_hotkeys(settings.visual_settings.show_hotkeys);
    win.set_quit_on_lost_focus(settings.visual_settings.quit_on_lost_focus);
    win.set_unwrap_urls(settings.behavioral_settings.unwrap_urls);
    win.set_run_in_background(settings.behavioral_settings.run_in_background);
    win.set_native_popup(settings.visual_settings.native_popup);
    win.set_theme(theme_to_choice(settings.visual_settings.theme));
    win.global::<StdWidgetPalette>()
        .set_color_scheme(theme_to_color_scheme(settings.visual_settings.theme));
}

fn save_visual_settings(state: &SharedState) {
    let settings = state.borrow().ui_settings.visual_settings.clone();
    send(state, MessageToMain::SaveConfigUISettings(settings));
    refresh(state);
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
        if let Some(win) = settings_window(&state) {
            win.set_is_default_browser(is_default);
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

fn wire(state: &SharedState, win: &SettingsWindow) {
    // ---- sidebar
    {
        let state = state.clone();
        win.on_set_default_browser(move || {
            send(&state, MessageToMain::SetBrowsersAsDefaultBrowser);
            poll_default_browser(&state);
        });
    }

    // ---- Browsers
    win.on_refresh(app::forward_handler(state, || MessageToMain::Refresh));
    {
        let state = state.clone();
        win.on_filter_changed(move |text| {
            with_local(|l| l.filter = text.to_string());
            refresh(&state);
        });
    }
    {
        let state = state.clone();
        win.on_show_hidden_toggled(move || {
            with_local(|l| l.show_hidden = !l.show_hidden);
            refresh(&state);
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
            if let Some(meta) = with_local(|l| l.rows.get(row as usize).cloned()) {
                toggle_visibility(&state, &meta);
            }
        });
    }
    {
        let state = state.clone();
        win.on_special_visibility_toggled(move |row| {
            if let Some(meta) = with_local(|l| l.special_rows.get(row as usize).cloned()) {
                toggle_visibility(&state, &meta);
            }
        });
    }
    {
        let state = state.clone();
        win.on_browser_group_toggled(move |row| {
            with_local(|l| {
                if let Some(meta) = l.rows.get(row as usize) {
                    let app_id = meta.app_id.clone();
                    if !l.collapsed.remove(&app_id) {
                        l.collapsed.insert(app_id);
                    }
                }
            });
            refresh(&state);
        });
    }
    {
        let state = state.clone();
        win.on_browser_menu_picked(move |row, action| {
            row_menu_picked(&state, row as usize, action)
        });
    }
    {
        let state = state.clone();
        win.on_special_menu_picked(move |row, _action| {
            if let Some(meta) = with_local(|l| l.special_rows.get(row as usize).cloned()) {
                toggle_default_profile(&state, &meta.ids[0]);
            }
        });
    }
    win.on_snap_browser_drop(|from, raw| {
        with_local(|l| {
            let kinds: Vec<RowKind> = l.rows.iter().map(|r| r.kind).collect();
            snap_drop_index(&kinds, from.max(0) as usize, raw.max(0) as usize) as i32
        })
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
        win.on_add_rule(move || {
            with_local(|l| {
                l.draft.get_or_insert_with(Draft::default);
            });
            refresh(&state);
        });
    }
    {
        let state = state.clone();
        win.on_rule_pattern_edited(move |index, text| {
            let pattern = text.trim().to_string();
            let mut candidate = match state.borrow().ui_settings.rules.get(index as usize) {
                Some(rule) => rule.clone(),
                None => return false,
            };
            candidate.url_pattern = pattern.clone();
            // an invalid pattern stays on screen (in red) but never gets saved
            if !candidate.has_valid_matcher() {
                return false;
            }
            edit_rule(&state, index, |rule| rule.url_pattern = pattern);
            true
        });
    }
    {
        let state = state.clone();
        win.on_rule_source_picked(move |index, source| {
            if let Some(source_app) = source_id(source) {
                edit_rule(&state, index, |rule| rule.source_app = source_app);
            }
        });
    }
    {
        let state = state.clone();
        win.on_rule_target_picked(move |index, target| {
            if let Some(target) = target_id(target) {
                edit_rule(&state, index, |rule| {
                    rule.opener = retarget(&rule.opener, target)
                });
            }
        });
    }
    {
        let state = state.clone();
        win.on_rule_private_toggled(move |index| {
            edit_rule(&state, index, |rule| {
                if let Some(o) = rule.opener.as_mut() {
                    o.incognito = !o.incognito;
                }
            });
        });
    }
    {
        let state = state.clone();
        win.on_rule_menu_picked(move |index, action| {
            {
                let mut st = state.borrow_mut();
                let rules = &mut st.ui_settings.rules;
                let index = index as usize;
                if index >= rules.len() {
                    return;
                }
                if action == 0 {
                    let copy = rules[index].clone();
                    rules.insert(index + 1, copy);
                } else {
                    rules.remove(index);
                }
            }
            save_rules(&state);
        });
    }
    {
        let state = state.clone();
        win.on_rules_dropped(move |from, to| {
            let moved = {
                let mut st = state.borrow_mut();
                move_item(
                    &mut st.ui_settings.rules,
                    from.max(0) as usize,
                    to.max(0) as usize,
                )
            };
            if moved {
                save_rules(&state);
            }
        });
    }
    {
        let state = state.clone();
        win.on_draft_pattern_edited(move |text| {
            update_draft(&state, |d| {
                d.pattern = text.to_string();
                d.show_invalid = false;
            });
        });
    }
    {
        let state = state.clone();
        win.on_draft_pattern_committed(move || {
            if with_local(|l| l.draft.is_some()) {
                update_draft(&state, |d| d.show_invalid = true);
            }
        });
    }
    {
        let state = state.clone();
        win.on_draft_source_picked(move |source| {
            if let Some(source_app) = source_id(source) {
                update_draft(&state, |d| d.source_app = source_app);
            }
        });
    }
    {
        let state = state.clone();
        win.on_draft_target_picked(move |target| {
            if let Some(target) = target_id(target) {
                update_draft(&state, |d| {
                    d.opener = Some(target);
                    d.show_invalid = true;
                });
            }
        });
    }
    {
        let state = state.clone();
        let weak_win = win.as_weak();
        win.on_draft_discarded(move || {
            with_local(|l| l.draft = None);
            if let Some(win) = weak_win.upgrade() {
                win.set_draft_pattern(SharedString::new());
            }
            refresh(&state);
        });
    }
    {
        let state = state.clone();
        win.on_test_url_edited(move |text| {
            with_local(|l| l.test_url = text.to_string());
            refresh(&state);
        });
    }
    {
        let state = state.clone();
        win.on_test_source_picked(move |source| {
            with_local(|l| {
                if (source as usize) < l.source_ids.len() {
                    l.test_source = source as usize;
                }
            });
            refresh(&state);
        });
    }
    {
        let state = state.clone();
        win.on_fallback_target_picked(move |target| {
            let Some(target) = target_id(target) else {
                return;
            };
            let default_opener = {
                let mut st = state.borrow_mut();
                let opener = &mut st.ui_settings.default_opener;
                *opener = retarget(opener, target);
                opener.clone()
            };
            send(&state, MessageToMain::SaveConfigDefaultOpener(default_opener));
            refresh(&state);
        });
    }
    {
        let state = state.clone();
        win.on_fallback_private_toggled(move || {
            let default_opener = {
                let mut st = state.borrow_mut();
                if let Some(o) = st.ui_settings.default_opener.as_mut() {
                    o.incognito = !o.incognito;
                }
                st.ui_settings.default_opener.clone()
            };
            send(&state, MessageToMain::SaveConfigDefaultOpener(default_opener));
            refresh(&state);
        });
    }

    // ---- General
    {
        let state = state.clone();
        win.on_theme_changed(move |choice| {
            state.borrow_mut().ui_settings.visual_settings.theme = choice_to_theme(choice);
            app::apply_theme(&state);
            save_visual_settings(&state);
        });
    }
    {
        let state = state.clone();
        win.on_show_hotkeys_changed(move |v| {
            state.borrow_mut().ui_settings.visual_settings.show_hotkeys = v;
            app::refresh_main_window_model(&state);
            save_visual_settings(&state);
        });
    }
    {
        let state = state.clone();
        win.on_quit_on_lost_focus_changed(move |v| {
            state
                .borrow_mut()
                .ui_settings
                .visual_settings
                .quit_on_lost_focus = v;
            save_visual_settings(&state);
        });
    }
    {
        let state = state.clone();
        win.on_unwrap_urls_changed(move |v| {
            state
                .borrow_mut()
                .ui_settings
                .behavioral_settings
                .unwrap_urls = v;
            let settings = state.borrow().ui_settings.behavioral_settings.clone();
            send(&state, MessageToMain::SaveConfigUIBehavioralSettings(settings));
            refresh(&state);
        });
    }
    {
        let state = state.clone();
        win.on_run_in_background_changed(move |v| {
            state
                .borrow_mut()
                .ui_settings
                .behavioral_settings
                .run_in_background = v;
            send(&state, MessageToMain::SetRunInBackground(v));
            refresh(&state);
        });
    }
    {
        let state = state.clone();
        win.on_native_popup_changed(move |v| {
            state.borrow_mut().ui_settings.visual_settings.native_popup = v;
            // the next link shows the other popup (see app::show_popup_for_url), and the one up
            // now goes away rather than lingering next to it
            if state.borrow().popup_visible() {
                app::hide_popup(&state);
            }
            save_visual_settings(&state);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use RowKind::{Child, Header, Single};

    fn meta(kind: RowKind, ids: &[&str]) -> RowMeta {
        RowMeta {
            kind,
            ids: ids.iter().map(|s| s.to_string()).collect(),
            app_id: ids[0].split('#').next().unwrap().to_string(),
            hidden: false,
        }
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    // safari | chrome: work, home, (hidden school) | firefox
    fn rows() -> (Vec<RowMeta>, Vec<String>) {
        let rows = vec![
            meta(Single, &["safari"]),
            meta(Header, &["chrome#work", "chrome#home", "chrome#school"]),
            meta(Child, &["chrome#work"]),
            meta(Child, &["chrome#home"]),
            meta(Single, &["firefox"]),
        ];
        let order = ids(&[
            "safari",
            "chrome#work",
            "chrome#home",
            "chrome#school",
            "firefox",
        ]);
        (rows, order)
    }

    #[test]
    fn snap_keeps_a_profile_inside_its_group() {
        let kinds = [Single, Header, Child, Child, Single];
        assert_eq!(snap_drop_index(&kinds, 3, 0), 2);
        assert_eq!(snap_drop_index(&kinds, 3, 2), 2);
        assert_eq!(snap_drop_index(&kinds, 2, 5), 4);
    }

    #[test]
    fn snap_keeps_apps_out_of_other_groups() {
        let kinds = [Single, Header, Child, Child, Single];
        // between chrome's profiles snaps to the nearest group edge
        assert_eq!(snap_drop_index(&kinds, 4, 2), 1);
        assert_eq!(snap_drop_index(&kinds, 0, 3), 4);
        assert_eq!(snap_drop_index(&kinds, 0, 5), 5);
        assert_eq!(snap_drop_index(&kinds, 0, 99), 5);
    }

    #[test]
    fn dropping_a_group_moves_all_its_profiles() {
        let (rows, order) = rows();
        let target = drop_target(&rows, &order, 1, 0);
        assert_eq!(
            target,
            Some((
                ids(&["chrome#work", "chrome#home", "chrome#school"]),
                Some("safari".to_string())
            ))
        );
    }

    #[test]
    fn dropping_a_row_next_to_itself_does_nothing() {
        let (rows, order) = rows();
        assert_eq!(drop_target(&rows, &order, 0, 0), None);
        assert_eq!(drop_target(&rows, &order, 0, 1), None);
        // a group's block ends after its last shown profile
        assert_eq!(drop_target(&rows, &order, 1, 4), None);
        assert_eq!(drop_target(&rows, &order, 2, 3), None);
    }

    #[test]
    fn dropping_at_the_end_moves_to_the_very_end() {
        let (rows, order) = rows();
        assert_eq!(drop_target(&rows, &order, 0, 5), Some((ids(&["safari"]), None)));
    }

    #[test]
    fn dropping_a_profile_at_its_group_end_goes_after_hidden_ones() {
        let (rows, order) = rows();
        assert_eq!(
            drop_target(&rows, &order, 2, 4),
            Some((ids(&["chrome#work"]), Some("firefox".to_string())))
        );
        assert_eq!(
            drop_target(&rows, &order, 3, 2),
            Some((ids(&["chrome#home"]), Some("chrome#work".to_string())))
        );
    }

    #[test]
    fn move_item_reorders_rules() {
        let mut items = vec!["a", "b", "c", "d"];
        assert!(move_item(&mut items, 0, 3));
        assert_eq!(items, vec!["b", "c", "a", "d"]);
        assert!(move_item(&mut items, 3, 0));
        assert_eq!(items, vec!["d", "b", "c", "a"]);
        assert!(move_item(&mut items, 1, 4));
        assert_eq!(items, vec!["d", "c", "a", "b"]);
    }

    #[test]
    fn move_item_next_to_itself_is_a_no_op() {
        let mut items = vec!["a", "b", "c"];
        assert!(!move_item(&mut items, 1, 1));
        assert!(!move_item(&mut items, 1, 2));
        assert!(!move_item(&mut items, 5, 0));
        assert_eq!(items, vec!["a", "b", "c"]);
    }

    #[test]
    fn draft_needs_a_valid_pattern_and_a_target() {
        let mut draft = Draft::default();
        assert!(!draft.is_complete());
        draft.pattern = "notion.so".to_string();
        assert!(!draft.is_complete());
        // "Show the picker" counts as a target
        draft.opener = Some(None);
        assert!(draft.is_complete());
        draft.pattern = "notion.so/[".to_string();
        assert!(!draft.is_complete());
        assert!(draft.pattern_is_invalid());
    }

    #[test]
    fn draft_with_only_a_source_app_is_complete_once_targeted() {
        let draft = Draft {
            source_app: "com.apple.mail".to_string(),
            opener: Some(Some("chrome#work".to_string())),
            ..Default::default()
        };
        assert!(draft.is_complete());
        assert_eq!(draft.as_rule().opener.unwrap().profile, "chrome#work");
    }

    #[test]
    fn draft_help_describes_what_the_pattern_matches() {
        let help = |pattern: &str| {
            let draft = Draft {
                pattern: pattern.to_string(),
                ..Default::default()
            };
            draft_help(&draft)
        };
        assert!(help("notion.so").starts_with("Matches notion.so and every page on it."));
        assert!(
            help("github.com/acme/**").starts_with("Matches every page under github.com/acme.")
        );
        assert!(help("github.com/acme").starts_with("Matches github.com/acme exactly"));
    }

    #[test]
    fn relative_time_reads_naturally() {
        assert_eq!(relative_time(Duration::from_secs(5)), "just now");
        assert_eq!(relative_time(Duration::from_secs(120)), "2 min ago");
        assert_eq!(relative_time(Duration::from_secs(3600)), "1 hour ago");
        assert_eq!(relative_time(Duration::from_secs(3 * 86400)), "3 days ago");
    }

    #[test]
    fn join_names_lists_apps() {
        assert_eq!(join_names(&ids(&["Slack"])), "Slack");
        assert_eq!(join_names(&ids(&["Slack", "Zoom"])), "Slack and Zoom");
        assert_eq!(
            join_names(&ids(&["Slack", "Zoom", "Figma"])),
            "Slack, Zoom and Figma"
        );
        assert_eq!(
            join_names(&ids(&["Slack", "Zoom", "Figma", "Linear", "Notion"])),
            "Slack, Zoom, Figma and 2 more"
        );
    }
}
