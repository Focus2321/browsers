// what the Settings window shows, computed once for both the Slint window (gui::settings_window)
// and the native macOS one (macos::native_settings), so the two can't drift apart.
// Plain data and pure functions only, no UI types: each window turns these into its own rows.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::gui::app_state::{UIBrowser, UIProfileAndIncognito, UISettings, UISettingsRule};
use crate::utils::ConfiguredTheme;
use crate::{RuleTestOutcome, profile_order};

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

pub const ORDER_HINT: &str = "Drag to reorder · 1–9 follow this order";
pub const ORDER_HINT_FILTERING: &str = "Clear the filter to reorder";
pub const SHOW_THE_PICKER: &str = "Show the picker";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKind {
    Single,
    Header,
    Child,
}

impl RowKind {
    pub fn as_int(self) -> i32 {
        match self {
            RowKind::Single => 0,
            RowKind::Header => 1,
            RowKind::Child => 2,
        }
    }
}

// what a row of the Browsers list stands for
#[derive(Clone, Debug, PartialEq)]
pub struct RowMeta {
    pub kind: RowKind,
    // unique within its list: the profile's id, or "group:<app id>" for a header.
    // The native window sends it back with row events, to catch a list that changed in between.
    pub id: String,
    // every profile id the row moves/hides together, in popup order.
    // For a header that includes the group's hidden profiles too, even when they aren't shown.
    pub ids: Vec<String>,
    pub app_id: String,
    pub hidden: bool,
    pub draggable: bool,
}

// the two lists of the Browsers page
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowList {
    Rows,
    // apps that only open their own links
    SpecialRows,
}

// an item of a Browsers row's "···" menu
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowAction {
    MoveToTop,
    MoveToBottom,
    ToggleDefault,
}

impl RowAction {
    // the Slint menu's item index, the move items counted even where they're left out
    pub fn from_slint_index(index: i32) -> Option<RowAction> {
        match index {
            0 => Some(RowAction::MoveToTop),
            1 => Some(RowAction::MoveToBottom),
            2 => Some(RowAction::ToggleDefault),
            _ => None,
        }
    }

    pub fn title(self, is_default: bool) -> &'static str {
        match self {
            RowAction::MoveToTop => "Move to top",
            RowAction::MoveToBottom => "Move to bottom",
            RowAction::ToggleDefault if is_default => "Stop opening unmatched links here",
            RowAction::ToggleDefault => "Open unmatched links here",
        }
    }
}

// a header only stands for a group, so it has no default to toggle; rows that can't be dragged
// (while filtering, special apps) can't be moved from the menu either
pub fn row_menu(kind: RowKind, draggable: bool) -> &'static [RowAction] {
    use RowAction::*;
    match (draggable, kind) {
        (true, RowKind::Header) => &[MoveToTop, MoveToBottom],
        (true, _) => &[MoveToTop, MoveToBottom, ToggleDefault],
        (false, RowKind::Header) => &[],
        (false, _) => &[ToggleDefault],
    }
}

// one row of the Browsers list as shown
#[derive(Clone, Debug, PartialEq)]
pub struct BrowserRow {
    pub meta: RowMeta,
    pub name: String,
    pub detail: String,
    // the app icon, or the profile's own picture for a child ("" = none)
    pub icon_path: String,
    pub letter: String,
    pub is_default: bool,
    pub collapsed: bool,
}

impl BrowserRow {
    pub fn menu(&self) -> &'static [RowAction] {
        row_menu(self.meta.kind, self.meta.draggable)
    }
}

// the rule being added on the Rules page, kept out of the saved rules until it's complete
#[derive(Clone, Debug, Default)]
pub struct Draft {
    pub pattern: String,
    pub source_app: String,
    // None until a target is picked; Some(None) is "Show the picker"
    pub opener: Option<Option<String>>,
    // only flag a bad pattern once the user is done typing it
    pub show_invalid: bool,
}

impl Draft {
    pub fn as_rule(&self) -> UISettingsRule {
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

    pub fn is_complete(&self) -> bool {
        self.opener.is_some() && self.as_rule().has_valid_matcher()
    }

    pub fn pattern_is_invalid(&self) -> bool {
        let pattern = self.pattern.trim();
        !pattern.is_empty() && !crate::url_rule::is_valid_pattern(pattern)
    }

    pub fn shows_invalid(&self) -> bool {
        self.show_invalid && self.pattern_is_invalid()
    }
}

// ------------------------------------------------------------------------------------ Browsers

// the Browsers page's window-local view settings
pub struct BrowsersView<'a> {
    // as typed, trimmed and lowercased here
    pub filter: &'a str,
    pub show_hidden: bool,
    // app ids of collapsed groups
    pub collapsed: &'a HashSet<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BrowsersModel {
    // as typed
    pub filter: String,
    pub show_hidden: bool,
    pub hidden_count: usize,
    pub reorder_enabled: bool,
    pub rows: Vec<BrowserRow>,
    pub special_rows: Vec<BrowserRow>,
    // every orderable (non-priority) profile id in popup order, hidden ones included
    pub order: Vec<String>,
    // the apps the "these aren't web browsers" banner names, empty = no banner
    pub banner_names: Vec<String>,
    pub found_text: String,
}

impl BrowsersModel {
    pub fn order_hint(&self) -> &'static str {
        if self.reorder_enabled {
            ORDER_HINT
        } else {
            ORDER_HINT_FILTERING
        }
    }

    // (title, message), None = no banner
    pub fn banner(&self) -> Option<(String, String)> {
        if self.banner_names.is_empty() {
            return None;
        }
        let title = match self.banner_names.len() {
            1 => "1 of these isn't a web browser".to_string(),
            count => format!("{count} of these aren't web browsers"),
        };
        let message = format!(
            "{} can open links, but you probably don't want them in the popup.",
            join_names(&self.banner_names)
        );
        Some((title, message))
    }

    // shown in place of the lists when both are empty
    pub fn empty_text(&self) -> String {
        let filter = self.filter.trim();
        if filter.is_empty() {
            "No apps found. Try Rescan.".to_string()
        } else {
            format!("Nothing matches \"{filter}\"")
        }
    }
}

struct Entry<'a> {
    browser: &'a UIBrowser,
    hidden: bool,
}

pub fn matches_filter(text: &str, filter: &str) -> bool {
    filter.is_empty() || text.to_lowercase().contains(filter)
}

pub fn first_letter(name: &str) -> String {
    name.chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default()
}

// a single app or one app profile as a list row
fn entry_row(
    entry: &Entry,
    kind: RowKind,
    default_profile: Option<&str>,
    draggable: bool,
) -> BrowserRow {
    let b = entry.browser;
    let (name, detail) = match kind {
        RowKind::Child => (b.profile_name.clone(), String::new()),
        _ if b.supports_profiles => (b.browser_name.clone(), b.profile_name.clone()),
        _ => (b.browser_name.clone(), String::new()),
    };
    // a profile without a picture of its own gets a letter instead of repeating the app icon
    let icon_path = match kind {
        RowKind::Child => b.profile_icon_path.clone(),
        _ => b.icon_path.clone(),
    };
    BrowserRow {
        meta: RowMeta {
            kind,
            id: b.unique_id.clone(),
            ids: vec![b.unique_id.clone()],
            app_id: b.unique_app_id.clone(),
            hidden: entry.hidden,
            draggable,
        },
        letter: first_letter(&name),
        name,
        detail,
        icon_path,
        is_default: default_profile == Some(b.unique_id.as_str()),
        collapsed: false,
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

pub fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

// "Slack", "Slack and Zoom", "Slack, Zoom and Figma", "Slack, Zoom, Figma and 2 more"
pub fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [only] => only.clone(),
        [first, second, third, rest @ ..] if !rest.is_empty() => {
            format!("{first}, {second}, {third} and {} more", rest.len())
        }
        [rest @ .., last] => format!("{} and {}", rest.join(", "), last),
    }
}

pub fn relative_time(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    match secs {
        0..60 => "just now".to_string(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86400 => format!("{} ago", plural((secs / 3600) as usize, "hour", "hours")),
        _ => format!("{} ago", plural((secs / 86400) as usize, "day", "days")),
    }
}

// the footer, `scanned_ago` being the time since the installed apps were last scanned
pub fn found_text(app_count: usize, scanned_ago: Option<Duration>) -> String {
    let apps = plural(app_count, "app", "apps");
    match scanned_ago {
        Some(elapsed) => format!("Found {apps} · checked {}", relative_time(elapsed)),
        None => format!("Found {apps}"),
    }
}

// the Browsers page from the visible and hidden profiles
pub fn browsers_model(
    visible: &[UIBrowser],
    hidden: &[UIBrowser],
    settings: &UISettings,
    view: &BrowsersView,
    scanned_ago: Option<Duration>,
) -> BrowsersModel {
    let filter = view.filter.trim().to_lowercase();
    let show_hidden = view.show_hidden;
    let reorder_enabled = filter.is_empty();
    let default_profile = settings.default_opener.as_ref().map(|o| o.profile.as_str());

    let ids = |list: &[UIBrowser]| -> Vec<String> {
        list.iter()
            .filter(|b| !b.has_priority_ordering())
            .map(|b| b.unique_id.clone())
            .collect()
    };
    let order = profile_order::full_order(&ids(visible), &ids(hidden), &settings.profile_order);
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
                default_profile,
                reorder_enabled,
            ));
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
        let is_collapsed = view.collapsed.contains(&first.unique_app_id) && filter.is_empty();
        rows.push(BrowserRow {
            meta: RowMeta {
                kind: RowKind::Header,
                id: format!("group:{}", first.unique_app_id),
                ids: group.iter().map(|e| e.browser.unique_id.clone()).collect(),
                app_id: first.unique_app_id.clone(),
                hidden: hidden_count == group.len(),
                draggable: reorder_enabled,
            },
            name: first.browser_name.clone(),
            detail,
            icon_path: first.icon_path.clone(),
            letter: first_letter(&first.browser_name),
            is_default: false,
            collapsed: is_collapsed,
        });

        if is_collapsed {
            continue;
        }
        for entry in children {
            rows.push(entry_row(
                entry,
                RowKind::Child,
                default_profile,
                reorder_enabled,
            ));
        }
    }

    // apps that only open their own links: always first in the popup, so not draggable
    let special_rows = visible
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
        })
        .map(|entry| entry_row(&entry, RowKind::Single, default_profile, false))
        .collect();

    // the "these aren't web browsers" banner lists every visible app that only opens its own links
    let mut banner_names: Vec<String> = vec![];
    if !settings.visual_settings.non_browser_banner_dismissed {
        for b in visible.iter().filter(|b| b.has_priority_ordering()) {
            if !banner_names.contains(&b.browser_name) {
                banner_names.push(b.browser_name.clone());
            }
        }
    }

    let app_count = visible
        .iter()
        .chain(hidden.iter())
        .map(|b| b.unique_app_id.as_str())
        .collect::<HashSet<_>>()
        .len();

    BrowsersModel {
        filter: view.filter.to_string(),
        show_hidden,
        hidden_count: hidden.len(),
        reorder_enabled,
        rows,
        special_rows,
        order,
        banner_names,
        found_text: found_text(app_count, scanned_ago),
    }
}

// where a row dragged from `from` lands when released at insertion point `raw` (0..=rows).
// A profile stays inside its own group; an app or a whole group only lands between other apps
// and groups, never between someone else's profiles.
pub fn snap_drop_index(kinds: &[RowKind], from: usize, raw: usize) -> usize {
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
pub fn block_end(kinds: &[RowKind], from: usize) -> usize {
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
pub fn drop_target(
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

// ------------------------------------------------------------------------------------ Rules

// "Work · Chrome" for a profile, just "Safari" for an app without profiles
pub fn profile_label(b: &UIBrowser) -> String {
    if b.supports_profiles {
        format!("{} · {}", b.profile_name, b.browser_name)
    } else {
        b.browser_name.clone()
    }
}

// a rule's target as shown in its picker
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TargetDisplay {
    pub label: String,
    // the profile's picture, with the app icon to fall back on ("" = none)
    pub profile_icon_path: String,
    pub icon_path: String,
    // whether it can open privately
    pub supports_private: bool,
}

pub fn target_display(opener: Option<&str>, all: &[&UIBrowser]) -> TargetDisplay {
    let Some(id) = opener else {
        return TargetDisplay {
            label: SHOW_THE_PICKER.to_string(),
            ..Default::default()
        };
    };
    match all.iter().find(|b| b.unique_id == id) {
        Some(b) => TargetDisplay {
            label: profile_label(b),
            profile_icon_path: b.profile_icon_path.clone(),
            icon_path: b.icon_path.clone(),
            supports_private: b.supports_incognito,
        },
        None => TargetDisplay {
            label: "Missing app".to_string(),
            ..Default::default()
        },
    }
}

// one of the "open in" choices
#[derive(Clone, Debug, PartialEq)]
pub struct TargetOption {
    // None = "Show the picker"
    pub id: Option<String>,
    pub label: String,
    pub profile_icon_path: String,
    pub icon_path: String,
}

// one of the "links from" choices
#[derive(Clone, Debug, PartialEq)]
pub struct SourceOption {
    // bundle id, "" = any app
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuleRow {
    pub pattern: String,
    // the user's latest text for the pattern when it was invalid and so not saved
    pub pattern_edit: Option<String>,
    pub source_id: String,
    // "" with any app
    pub source_label: String,
    // "" = show the picker
    pub target_id: String,
    pub target: TargetDisplay,
    pub private: bool,
    // it's the rule the test link hit
    pub matched: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DraftRow {
    pub pattern: String,
    pub pattern_placeholder: &'static str,
    pub pattern_invalid: bool,
    pub source_id: String,
    pub source_label: String,
    // None = not picked yet, Some("") = show the picker
    pub target_id: Option<String>,
    // "" while not picked
    pub target_label: String,
    pub help: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TestResultKind {
    // nothing typed yet
    None,
    Rule,
    Fallback,
    Invalid,
}

impl TestResultKind {
    pub fn as_int(self) -> i32 {
        match self {
            TestResultKind::None => 0,
            TestResultKind::Rule => 1,
            TestResultKind::Fallback => 2,
            TestResultKind::Invalid => 3,
        }
    }
}

// the "Test a link" card
#[derive(Clone, Debug, PartialEq)]
pub struct TestRow {
    pub url: String,
    // index into the sources, kept in range
    pub source_index: usize,
    pub source_id: String,
    pub source_label: String,
    pub result_kind: TestResultKind,
    pub result_text: String,
    pub matched_rule: Option<usize>,
}

// the "Everything else" card
#[derive(Clone, Debug, PartialEq)]
pub struct FallbackRow {
    pub target_id: String,
    pub target: TargetDisplay,
    pub private: bool,
    pub matched: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RulesModel {
    pub rules: Vec<RuleRow>,
    pub sources: Vec<SourceOption>,
    pub targets: Vec<TargetOption>,
    pub draft: Option<DraftRow>,
    pub test: TestRow,
    pub fallback: FallbackRow,
}

// the Rules page's window-local state
pub struct RulesView<'a> {
    pub draft: Option<&'a Draft>,
    pub test_url: &'a str,
    // index into the sources
    pub test_source: usize,
    // rule index -> the invalid text last typed into its pattern
    pub pattern_edits: &'a HashMap<usize, String>,
}

pub fn draft_help(draft: &Draft) -> String {
    let pattern = draft.pattern.trim();
    if draft.shows_invalid() {
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

// "any app", then every found app, then other common link senders, then anything only the saved
// rules know about
pub fn source_options(all: &[&UIBrowser], rules: &[UISettingsRule]) -> Vec<SourceOption> {
    let mut sources = vec![SourceOption {
        id: String::new(),
        label: "Any app".to_string(),
    }];
    let mut add_source = |id: &str, label: &str| {
        if !id.is_empty() && !sources.iter().any(|known| known.id == id) {
            sources.push(SourceOption {
                id: id.to_string(),
                label: label.to_string(),
            });
        }
    };
    let mut found: Vec<&UIBrowser> = all.to_vec();
    found.sort_by_key(|b| b.browser_name.to_lowercase());
    for b in found {
        add_source(&b.bundle_id, &b.browser_name);
    }
    for (label, id) in KNOWN_SOURCE_APPS {
        add_source(id, label);
    }
    for rule in rules {
        add_source(&rule.source_app, &rule.source_app);
    }
    sources
}

// the Rules page from every profile (hidden ones are valid targets too) and the saved rules
pub fn rules_model(
    visible: &[UIBrowser],
    hidden: &[UIBrowser],
    settings: &UISettings,
    view: &RulesView,
) -> RulesModel {
    let all: Vec<&UIBrowser> = visible.iter().chain(hidden.iter()).collect();
    let hidden_ids: HashSet<&str> = hidden.iter().map(|b| b.unique_id.as_str()).collect();

    let mut targets = vec![TargetOption {
        id: None,
        label: SHOW_THE_PICKER.to_string(),
        profile_icon_path: String::new(),
        icon_path: String::new(),
    }];
    for b in &all {
        let mut label = profile_label(b);
        if hidden_ids.contains(b.unique_id.as_str()) {
            label.push_str(" (hidden)");
        }
        targets.push(TargetOption {
            id: Some(b.unique_id.clone()),
            label,
            profile_icon_path: b.profile_icon_path.clone(),
            icon_path: b.icon_path.clone(),
        });
    }

    let sources = source_options(&all, &settings.rules);
    let source_label = |id: &str| -> String {
        if id.is_empty() {
            return String::new();
        }
        sources
            .iter()
            .find(|known| known.id == id)
            .map(|known| known.label.clone())
            .unwrap_or_else(|| id.to_string())
    };

    let rules = &settings.rules;
    let default_opener = settings.default_opener.as_ref();
    let fallback_target = target_display(default_opener.map(|o| o.profile.as_str()), &all);

    // test a link
    let test_source = if view.test_source < sources.len() {
        view.test_source
    } else {
        0
    };
    let (result_kind, result_text, matched_rule) = if view.test_url.trim().is_empty() {
        (TestResultKind::None, String::new(), None)
    } else {
        let source = Some(sources[test_source].id.as_str()).filter(|s| !s.is_empty());
        match crate::test_opening_rules(rules, default_opener.is_some(), view.test_url, source) {
            RuleTestOutcome::InvalidUrl => (
                TestResultKind::Invalid,
                "That isn't a link yet".to_string(),
                None,
            ),
            RuleTestOutcome::MatchedRule(i) => {
                let destination = match rules[i].opener.as_ref() {
                    None => "shows the picker".to_string(),
                    Some(o) => {
                        let label = target_display(Some(&o.profile), &all).label;
                        let private = if o.incognito { " (private)" } else { "" };
                        format!("opens in {label}{private}")
                    }
                };
                (
                    TestResultKind::Rule,
                    format!("Rule {} matches → {destination}", i + 1),
                    Some(i),
                )
            }
            RuleTestOutcome::DefaultProfile => {
                let private = if default_opener.is_some_and(|o| o.incognito) {
                    " (private)"
                } else {
                    ""
                };
                (
                    TestResultKind::Fallback,
                    format!("No rule matches → opens in {}{private}", fallback_target.label),
                    None,
                )
            }
            RuleTestOutcome::Picker => (
                TestResultKind::Fallback,
                "No rule matches → shows the picker".to_string(),
                None,
            ),
        }
    };

    let rule_rows = rules
        .iter()
        .enumerate()
        .map(|(i, rule)| {
            let opener = rule.opener.as_ref();
            RuleRow {
                pattern: rule.url_pattern.clone(),
                pattern_edit: view.pattern_edits.get(&i).cloned(),
                source_id: rule.source_app.clone(),
                source_label: source_label(&rule.source_app),
                target_id: opener.map(|o| o.profile.clone()).unwrap_or_default(),
                target: target_display(opener.map(|o| o.profile.as_str()), &all),
                private: opener.is_some_and(|o| o.incognito),
                matched: matched_rule == Some(i),
            }
        })
        .collect();

    let draft = view.draft.map(|draft| DraftRow {
        pattern: draft.pattern.clone(),
        pattern_placeholder: if draft.source_app.is_empty() {
            "e.g. notion.so"
        } else {
            "any link"
        },
        pattern_invalid: draft.shows_invalid(),
        source_id: draft.source_app.clone(),
        source_label: source_label(&draft.source_app),
        target_id: draft
            .opener
            .as_ref()
            .map(|opener| opener.clone().unwrap_or_default()),
        target_label: draft
            .opener
            .as_ref()
            .map(|opener| target_display(opener.as_deref(), &all).label)
            .unwrap_or_default(),
        help: draft_help(draft),
    });

    let test = TestRow {
        url: view.test_url.to_string(),
        source_index: test_source,
        source_id: sources[test_source].id.clone(),
        source_label: sources[test_source].label.clone(),
        result_kind,
        result_text,
        matched_rule,
    };

    let fallback = FallbackRow {
        target_id: default_opener
            .map(|o| o.profile.clone())
            .unwrap_or_default(),
        target: fallback_target,
        private: default_opener.is_some_and(|o| o.incognito),
        matched: result_kind == TestResultKind::Fallback,
    };

    RulesModel {
        rules: rule_rows,
        sources,
        targets,
        draft,
        test,
        fallback,
    }
}

// moves the item at `from` so it ends up at insertion point `to` (0..=len) of the original list
pub fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from >= items.len() || to > items.len() || to == from || to == from + 1 {
        return false;
    }
    let item = items.remove(from);
    let to = if to > from { to - 1 } else { to };
    items.insert(to, item);
    true
}

// keeps the private flag when switching to another target that can also open privately
pub fn retarget(
    opener: &Option<UIProfileAndIncognito>,
    target: Option<String>,
) -> Option<UIProfileAndIncognito> {
    let incognito = opener.as_ref().is_some_and(|o| o.incognito);
    target.map(|profile| UIProfileAndIncognito { profile, incognito })
}

// ------------------------------------------------------------------------------------ General

#[derive(Clone, Debug, PartialEq)]
pub struct GeneralModel {
    pub theme: ConfiguredTheme,
    // whether the "Native popup" setting means anything on this system
    pub native_popup_available: bool,
    pub native_popup: bool,
    pub show_hotkeys: bool,
    // in the background the popup always hides on a click elsewhere, so the setting only applies
    // to an app that quits after each link
    pub quit_on_lost_focus_visible: bool,
    pub quit_on_lost_focus: bool,
    pub unwrap_urls: bool,
    pub run_in_background: bool,
}

pub fn general_model(settings: &UISettings, native_popup_available: bool) -> GeneralModel {
    let visual = &settings.visual_settings;
    let behavior = &settings.behavioral_settings;
    GeneralModel {
        theme: visual.theme,
        native_popup_available,
        native_popup: visual.native_popup,
        show_hotkeys: visual.show_hotkeys,
        quit_on_lost_focus_visible: cfg!(target_os = "macos") && !behavior.run_in_background,
        quit_on_lost_focus: visual.quit_on_lost_focus,
        unwrap_urls: behavior.unwrap_urls,
        run_in_background: behavior.run_in_background,
    }
}

// everything the Settings window shows
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsModel {
    pub is_default_browser: bool,
    pub browsers: BrowsersModel,
    pub rules: RulesModel,
    pub general: GeneralModel,
}

#[cfg(test)]
pub mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::gui::app_state::{UIBehavioralSettings, UIVisualSettings};
    use RowKind::{Child, Header, Single};

    fn meta(kind: RowKind, ids: &[&str]) -> RowMeta {
        RowMeta {
            kind,
            id: ids[0].to_string(),
            ids: ids.iter().map(|s| s.to_string()).collect(),
            app_id: ids[0].split('#').next().unwrap().to_string(),
            hidden: false,
            draggable: true,
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

    // a profile like the app finder produces: `profile` "" = an app without profiles
    pub fn browser(app: &str, profile: &str, bundle_id: &str) -> UIBrowser {
        let supports_profiles = !profile.is_empty();
        UIBrowser {
            browser_profile_index: 0,
            is_first: false,
            is_last: false,
            restricted_url_matchers: Arc::new(vec![]),
            browser_name: app.to_string(),
            profile_name: profile.to_string(),
            supports_profiles,
            supports_incognito: supports_profiles,
            icon_path: format!("/icons/{app}.png"),
            profile_icon_path: String::new(),
            unique_id: if supports_profiles {
                format!("/Applications/{app}.app#{profile}")
            } else {
                format!("/Applications/{app}.app")
            },
            unique_app_id: format!("/Applications/{app}.app"),
            bundle_id: bundle_id.to_string(),
            filtered_index: 0,
        }
    }

    // an app that only opens its own links, like Slack
    pub fn special(app: &str, profile: &str, bundle_id: &str) -> UIBrowser {
        let matcher = crate::url_rule::to_url_matcher("example.com").to_glob_matcher();
        UIBrowser {
            restricted_url_matchers: Arc::new(vec![matcher]),
            supports_profiles: false,
            supports_incognito: false,
            unique_id: format!("/Applications/{app}.app#{profile}"),
            ..browser(app, "", bundle_id)
        }
    }

    pub fn settings() -> UISettings {
        UISettings {
            default_opener: None,
            rules: vec![],
            visual_settings: UIVisualSettings {
                show_hotkeys: true,
                quit_on_lost_focus: false,
                theme: ConfiguredTheme::Auto,
                non_browser_banner_dismissed: false,
                native_popup: true,
            },
            behavioral_settings: UIBehavioralSettings {
                unwrap_urls: true,
                run_in_background: true,
            },
            profile_order: vec![],
        }
    }

    pub fn rule(source: &str, pattern: &str, target: Option<&str>) -> UISettingsRule {
        UISettingsRule {
            source_app: source.to_string(),
            url_pattern: pattern.to_string(),
            opener: target.map(|profile| UIProfileAndIncognito {
                profile: profile.to_string(),
                incognito: false,
            }),
        }
    }

    fn browsers(
        visible: &[UIBrowser],
        hidden: &[UIBrowser],
        settings: &UISettings,
        filter: &str,
        show_hidden: bool,
        collapsed: &[&str],
    ) -> BrowsersModel {
        let collapsed: HashSet<String> = collapsed.iter().map(|s| s.to_string()).collect();
        let view = BrowsersView {
            filter,
            show_hidden,
            collapsed: &collapsed,
        };
        browsers_model(visible, hidden, settings, &view, None)
    }

    fn rules(
        visible: &[UIBrowser],
        hidden: &[UIBrowser],
        settings: &UISettings,
        draft: Option<&Draft>,
        test_url: &str,
    ) -> RulesModel {
        let edits = HashMap::new();
        let view = RulesView {
            draft,
            test_url,
            test_source: 0,
            pattern_edits: &edits,
        };
        rules_model(visible, hidden, settings, &view)
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

    #[test]
    fn found_text_mentions_the_scan_when_known() {
        assert_eq!(found_text(1, None), "Found 1 app");
        assert_eq!(
            found_text(9, Some(Duration::from_secs(120))),
            "Found 9 apps · checked 2 min ago"
        );
    }

    #[test]
    fn row_menus_follow_the_row() {
        use RowAction::*;
        assert_eq!(row_menu(Single, true), [MoveToTop, MoveToBottom, ToggleDefault]);
        assert_eq!(row_menu(Child, true), [MoveToTop, MoveToBottom, ToggleDefault]);
        assert_eq!(row_menu(Header, true), [MoveToTop, MoveToBottom]);
        assert_eq!(row_menu(Single, false), [ToggleDefault]);
        assert!(row_menu(Header, false).is_empty());
        assert_eq!(ToggleDefault.title(true), "Stop opening unmatched links here");
        assert_eq!(ToggleDefault.title(false), "Open unmatched links here");
        assert_eq!(RowAction::from_slint_index(2), Some(ToggleDefault));
        assert_eq!(RowAction::from_slint_index(3), None);
    }

    #[test]
    fn browsers_are_grouped_by_app() {
        let visible = vec![
            browser("Safari", "", "com.apple.Safari"),
            browser("Chrome", "Work", "com.google.Chrome"),
            browser("Chrome", "Home", "com.google.Chrome"),
        ];
        let hidden = vec![browser("Chrome", "School", "com.google.Chrome")];
        let mut settings = settings();
        settings.default_opener = Some(UIProfileAndIncognito {
            profile: visible[1].unique_id.clone(),
            incognito: false,
        });
        let model = browsers(&visible, &hidden, &settings, "", false, &[]);

        let summary: Vec<(RowKind, &str, &str, &str)> = model
            .rows
            .iter()
            .map(|r| {
                (
                    r.meta.kind,
                    r.meta.id.as_str(),
                    r.name.as_str(),
                    r.detail.as_str(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (Single, "/Applications/Safari.app", "Safari", ""),
                (
                    Header,
                    "group:/Applications/Chrome.app",
                    "Chrome",
                    "3 profiles · 1 hidden"
                ),
                (Child, "/Applications/Chrome.app#Work", "Work", ""),
                (Child, "/Applications/Chrome.app#Home", "Home", ""),
            ]
        );
        // the header moves and hides the hidden profile too
        assert_eq!(model.rows[1].meta.ids.len(), 3);
        assert!(model.rows[2].is_default);
        // a child without a picture of its own gets a letter
        assert_eq!(model.rows[2].icon_path, "");
        assert_eq!(model.rows[2].letter, "W");
        assert_eq!(model.rows[1].icon_path, "/icons/Chrome.png");
        assert_eq!(model.hidden_count, 1);
        assert!(model.reorder_enabled);
        assert_eq!(model.order_hint(), ORDER_HINT);
        assert_eq!(model.found_text, "Found 2 apps");
        assert!(model.banner().is_none());
    }

    #[test]
    fn show_hidden_and_collapsed_groups() {
        let visible = vec![browser("Chrome", "Work", ""), browser("Chrome", "Home", "")];
        let hidden = vec![browser("Firefox", "", "")];
        let shown = browsers(&visible, &hidden, &settings(), "", true, &[]);
        let firefox = shown.rows.last().unwrap();
        assert_eq!(firefox.name, "Firefox");
        assert!(firefox.meta.hidden);

        let collapsed = browsers(
            &visible,
            &hidden,
            &settings(),
            "",
            false,
            &["/Applications/Chrome.app"],
        );
        assert_eq!(collapsed.rows.len(), 1);
        assert!(collapsed.rows[0].collapsed);
    }

    #[test]
    fn filtering_turns_off_reordering() {
        let visible = vec![
            browser("Safari", "", ""),
            browser("Chrome", "Work", ""),
            browser("Chrome", "Home", ""),
        ];
        let model = browsers(&visible, &[], &settings(), " WORK ", false, &[]);
        let names: Vec<&str> = model.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Chrome", "Work"]);
        assert!(!model.reorder_enabled);
        assert!(model.rows.iter().all(|r| !r.meta.draggable));
        assert_eq!(model.order_hint(), ORDER_HINT_FILTERING);
        // a header can't be collapsed while filtering
        let model = browsers(
            &visible,
            &[],
            &settings(),
            "chrome",
            false,
            &["/Applications/Chrome.app"],
        );
        assert!(!model.rows[0].collapsed);

        let nothing = browsers(&visible, &[], &settings(), "zzz ", false, &[]);
        assert!(nothing.rows.is_empty());
        assert_eq!(nothing.empty_text(), "Nothing matches \"zzz\"");
        let none = browsers(&[], &[], &settings(), "", false, &[]);
        assert_eq!(none.empty_text(), "No apps found. Try Rescan.");
    }

    #[test]
    fn special_apps_get_their_own_list_and_the_banner() {
        let visible = vec![
            special("Slack", "T1", "com.tinyspeck.slackmacgap"),
            special("Zoom", "Z", "us.zoom.xos"),
            browser("Safari", "", ""),
        ];
        let model = browsers(&visible, &[], &settings(), "", false, &[]);
        assert_eq!(model.rows.len(), 1);
        assert_eq!(model.special_rows.len(), 2);
        assert!(model.special_rows.iter().all(|r| !r.meta.draggable));
        assert_eq!(model.special_rows[0].menu(), [RowAction::ToggleDefault]);
        assert_eq!(
            model.banner(),
            Some((
                "2 of these aren't web browsers".to_string(),
                "Slack and Zoom can open links, but you probably don't want them in the popup."
                    .to_string()
            ))
        );

        let mut dismissed = settings();
        dismissed.visual_settings.non_browser_banner_dismissed = true;
        let model = browsers(&visible, &[], &dismissed, "", false, &[]);
        assert!(model.banner().is_none());
    }

    #[test]
    fn rules_page_options_and_labels() {
        let visible = vec![
            browser("Safari", "", "com.apple.Safari"),
            browser("Chrome", "Work", "com.google.Chrome"),
        ];
        let hidden = vec![browser("Firefox", "Home", "org.mozilla.firefox")];
        let mut settings = settings();
        settings.rules = vec![
            rule("com.example.only-in-a-rule", "github.com/acme/**", None),
            rule("", "notion.so", Some("/Applications/Gone.app")),
        ];
        let model = rules(&visible, &hidden, &settings, None, "");

        let targets: Vec<&str> = model.targets.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(
            targets,
            [
                "Show the picker",
                "Safari",
                "Work · Chrome",
                "Home · Firefox (hidden)"
            ]
        );
        assert_eq!(model.targets[0].id, None);
        let sources: Vec<&str> = model.sources.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            sources[..4],
            [
                "",
                "com.google.Chrome",
                "org.mozilla.firefox",
                "com.apple.Safari"
            ]
        );
        assert_eq!(*sources.last().unwrap(), "com.example.only-in-a-rule");

        assert_eq!(model.rules[0].source_label, "com.example.only-in-a-rule");
        assert_eq!(model.rules[0].target.label, "Show the picker");
        assert_eq!(model.rules[0].target_id, "");
        assert_eq!(model.rules[1].source_label, "");
        assert_eq!(model.rules[1].target.label, "Missing app");
        assert_eq!(model.rules[1].target_id, "/Applications/Gone.app");
        assert_eq!(model.test.result_kind, TestResultKind::None);
        assert_eq!(model.test.source_label, "Any app");
        assert!(model.draft.is_none());
    }

    #[test]
    fn test_link_results() {
        let visible = vec![browser("Chrome", "Work", "com.google.Chrome")];
        let work = visible[0].unique_id.clone();
        let mut settings = settings();
        settings.rules = vec![rule("", "github.com/acme/**", Some(&work))];
        settings.rules[0].opener.as_mut().unwrap().incognito = true;

        let model = rules(&visible, &[], &settings, None, "https://github.com/acme/api");
        assert_eq!(model.test.result_kind, TestResultKind::Rule);
        assert_eq!(
            model.test.result_text,
            "Rule 1 matches → opens in Work · Chrome (private)"
        );
        assert!(model.rules[0].matched);
        assert!(!model.fallback.matched);

        let model = rules(&visible, &[], &settings, None, "example.com");
        assert_eq!(model.test.result_kind, TestResultKind::Fallback);
        assert_eq!(model.test.result_text, "No rule matches → shows the picker");
        assert!(model.fallback.matched);

        settings.default_opener = Some(UIProfileAndIncognito {
            profile: work.clone(),
            incognito: false,
        });
        let model = rules(&visible, &[], &settings, None, "example.com");
        assert_eq!(
            model.test.result_text,
            "No rule matches → opens in Work · Chrome"
        );
        assert_eq!(model.fallback.target.label, "Work · Chrome");
        assert!(model.fallback.target.supports_private);

        let model = rules(&visible, &[], &settings, None, "not a link");
        assert_eq!(model.test.result_kind, TestResultKind::Invalid);
        assert_eq!(model.test.result_text, "That isn't a link yet");
    }

    #[test]
    fn test_source_out_of_range_falls_back_to_any_app() {
        let edits = HashMap::new();
        let view = RulesView {
            draft: None,
            test_url: "",
            test_source: 99,
            pattern_edits: &edits,
        };
        let model = rules_model(&[], &[], &settings(), &view);
        assert_eq!(model.test.source_index, 0);
        assert_eq!(model.test.source_id, "");
    }

    #[test]
    fn draft_row_follows_the_draft() {
        let visible = vec![browser("Safari", "", "com.apple.Safari")];
        let draft = Draft::default();
        let row = rules(&visible, &[], &settings(), Some(&draft), "")
            .draft
            .unwrap();
        assert_eq!(row.target_id, None);
        assert_eq!(row.target_label, "");
        assert_eq!(row.pattern_placeholder, "e.g. notion.so");
        assert_eq!(row.source_label, "");

        let draft = Draft {
            pattern: "notion.so/[".to_string(),
            source_app: "com.apple.Safari".to_string(),
            opener: Some(None),
            show_invalid: true,
        };
        let row = rules(&visible, &[], &settings(), Some(&draft), "")
            .draft
            .unwrap();
        assert_eq!(row.target_id, Some(String::new()));
        assert_eq!(row.target_label, "Show the picker");
        assert_eq!(row.pattern_placeholder, "any link");
        assert_eq!(row.source_label, "Safari");
        assert!(row.pattern_invalid);
        assert!(row.help.starts_with("That isn't a site or link pattern"));
    }

    #[test]
    fn rejected_pattern_edits_show_on_their_rule() {
        let mut settings = settings();
        settings.rules = vec![rule("", "github.com", None), rule("", "notion.so", None)];
        let edits = HashMap::from([(1, "notion.so/[".to_string())]);
        let view = RulesView {
            draft: None,
            test_url: "",
            test_source: 0,
            pattern_edits: &edits,
        };
        let model = rules_model(&[], &[], &settings, &view);
        assert_eq!(model.rules[0].pattern_edit, None);
        assert_eq!(model.rules[1].pattern, "notion.so");
        assert_eq!(model.rules[1].pattern_edit.as_deref(), Some("notion.so/["));
    }

    #[test]
    fn close_when_clicking_elsewhere_only_without_the_background_mode() {
        let mut settings = settings();
        assert!(!general_model(&settings, true).quit_on_lost_focus_visible);
        settings.behavioral_settings.run_in_background = false;
        assert_eq!(
            general_model(&settings, true).quit_on_lost_focus_visible,
            cfg!(target_os = "macos")
        );
    }
}
