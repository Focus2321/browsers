use slint::{Color, Image};
use url::Url;

use crate::gui::app_state::UIBrowser;
use crate::gui::generated::{BrowserItem, RestorableProfile};
use crate::gui::icon_loader::IconLoader;
use crate::gui::screen::{Point, Rect};

// most rows the popup shows before the list starts scrolling
const MAX_VISIBLE_ROWS: usize = 10;

// icon styles are conventionally different on platforms,
// e.g most macos icons are actually with a lot of padding
pub fn get_icon_size() -> f32 {
    if cfg!(target_os = "macos") {
        28.0
    } else {
        22.0
    }
}

fn calculate_visible_browser_count(browsers_total: usize) -> usize {
    // at least 1 row, so an empty filter result still has room for its "No matches" line
    browsers_total.clamp(1, MAX_VISIBLE_ROWS)
}

// window size for the given number of (filtered) rows, so the height always fits whole rows.
// row_height and chrome_height (header, action bar, paddings) come from main_window.slint, the
// single source of truth for the popup's layout
pub fn recalculate_window_size(
    browser_count: usize,
    row_height: f32,
    chrome_height: f32,
    width: f32,
) -> (f32, f32) {
    let row_count = calculate_visible_browser_count(browser_count) as f32;
    (width, chrome_height + row_count * row_height)
}

pub fn calculate_window_position(
    mouse: Point,
    screen_rect: Rect,
    window_size: (f32, f32),
) -> Point {
    let (window_width, window_height) = window_size;
    let mut x = mouse.x;
    let mut y = mouse.y;

    // if x is less than starting point, start from min starting rect
    if x < screen_rect.x0 {
        x = screen_rect.x0;
    }
    // if it doesn't fit, put it as far as it does fit
    if x + window_width > screen_rect.x1 {
        x = screen_rect.x1 - window_width;
    }
    // if y is less than starting point, start from min starting rect
    if y < screen_rect.y0 {
        y = screen_rect.y0;
    }
    if y + window_height > screen_rect.y1 {
        y = screen_rect.y1 - window_height;
    }

    Point { x, y }
}

// shared by the main window's full item (real icons + hotkey) and the Settings window's
// lightweight one (blank icons, no hotkey) - see settings_window::to_lightweight_browser_item
pub fn browser_item(
    b: &UIBrowser,
    image: Image,
    profile_image: Image,
    hotkey: String,
    show_hotkey: bool,
) -> BrowserItem {
    BrowserItem {
        image,
        profile_image,
        browser_name: b.browser_name.clone().into(),
        profile_name: b.profile_name.clone().into(),
        full_name: b.get_full_name().into(),
        show_incognito: b.supports_incognito,
        hotkey: hotkey.into(),
        show_hotkey,
        unique_id: b.unique_id.clone().into(),
        unique_app_id: b.unique_app_id.clone().into(),
        browser_profile_index: b.browser_profile_index as i32,
        filtered_index: b.filtered_index as i32,
        is_first: b.is_first,
        is_last: b.is_last,
        has_priority_ordering: b.has_priority_ordering(),
        supports_profiles: b.supports_profiles,
        profile_initial: profile_initial(&b.profile_name).into(),
        avatar_color: avatar_color(&b.unique_id),
    }
}

// `position` is the row's place in the list as shown, which is what the 1-9 hotkeys and the
// keyboard selection follow (it differs from filtered_index once type-to-filter narrows the list)
pub fn to_browser_item(
    b: &UIBrowser,
    position: usize,
    icons: &mut IconLoader,
    show_hotkeys: bool,
) -> BrowserItem {
    let show_hotkey = show_hotkeys && position < 9;
    let hotkey = if show_hotkey {
        (position + 1).to_string()
    } else {
        String::new()
    };

    let mut item = browser_item(
        b,
        icons.load(&b.icon_path),
        icons.load(&b.profile_icon_path),
        hotkey,
        show_hotkey,
    );
    item.filtered_index = position as i32;
    item
}

pub fn to_restorable_profile(b: &UIBrowser) -> RestorableProfile {
    RestorableProfile {
        unique_id: b.unique_id.clone().into(),
        full_name: b.get_full_name().into(),
    }
}

// first letter of the profile name, for the colored circle shown when a profile has no picture.
// shared with the native popup, see macos::native_popup
pub fn profile_initial(profile_name: &str) -> String {
    profile_name
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default()
}

// picks a stable color per profile, so the same profile keeps its color across launches (and in
// both popups)
fn avatar_rgb(unique_id: &str) -> (u8, u8, u8) {
    const COLORS: [(u8, u8, u8); 8] = [
        (0x7c, 0x5c, 0xe6), // violet
        (0xd9, 0x66, 0x3a), // orange
        (0x3e, 0x9d, 0x68), // green
        (0x2f, 0x7f, 0xd8), // blue
        (0xc9, 0x4a, 0x8a), // pink
        (0x1f, 0x9a, 0x9a), // teal
        (0xc2, 0x4b, 0x4b), // red
        (0xa0, 0x7a, 0x2c), // ochre
    ];
    // FNV-1a - std's DefaultHasher isn't guaranteed to be stable between Rust releases
    let hash = unique_id
        .bytes()
        .fold(0x811c_9dc5_u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193));
    COLORS[hash as usize % COLORS.len()]
}

fn avatar_color(unique_id: &str) -> Color {
    let (r, g, b) = avatar_rgb(unique_id);
    Color::from_rgb_u8(r, g, b)
}

// the same color as "#rrggbb", for the native popup
pub fn avatar_color_hex(unique_id: &str) -> String {
    let (r, g, b) = avatar_rgb(unique_id);
    format!("#{r:02x}{g:02x}{b:02x}")
}

// type-to-filter: keeps the browsers whose profile or app name contains the query, ignoring case
pub fn matches_filter(b: &UIBrowser, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let profile_name = if b.supports_profiles {
        b.profile_name.as_str()
    } else {
        ""
    };
    [b.browser_name.as_str(), profile_name]
        .iter()
        .any(|name| name.to_lowercase().contains(&query))
}

// applies one key press to the filter text - see edit-filter in main_window.slint.
// digits are left alone, those stay the 1-9 hotkeys.
pub fn edit_filter(current: &str, key: &str) -> String {
    const BACKSPACE: &str = "\u{8}";
    const DELETE: &str = "\u{7f}";
    if key == BACKSPACE || key == DELETE {
        let mut text = current.to_string();
        text.pop();
        return text;
    }
    // Slint encodes arrows, function keys and other special keys as control characters or
    // private-use codepoints (U+F700..U+F8FF), neither of which is something the user typed
    let is_typed_text = !key.is_empty()
        && key.chars().all(|c| {
            !c.is_control() && !('\u{f700}'..='\u{f8ff}').contains(&c) && !c.is_ascii_digit()
        });
    if is_typed_text {
        format!("{current}{key}")
    } else {
        current.to_string()
    }
}

// splits a link into the header's two lines: "github.com" and "/slint-ui/slint/pull/123".
// anything that doesn't parse as a url with a host goes on the second line as-is
pub fn split_url_for_header(url: &str) -> (String, String) {
    match Url::parse(url) {
        Ok(parsed) if parsed.host_str().is_some() => {
            let domain = parsed.host_str().unwrap_or_default().to_string();
            let mut rest = parsed.path().to_string();
            if rest == "/" {
                rest.clear();
            }
            if let Some(query) = parsed.query() {
                rest.push('?');
                rest.push_str(query);
            }
            if let Some(fragment) = parsed.fragment() {
                rest.push('#');
                rest.push_str(fragment);
            }
            (domain, rest)
        }
        _ => (String::new(), url.to_string()),
    }
}

// display name ("Slack") of the app with the given bundle id ("com.tinyspeck.slackmacgap"), for
// the header's "from Slack"
#[cfg(target_os = "macos")]
pub fn source_app_name(bundle_id: &str) -> String {
    if bundle_id.is_empty() {
        return String::new();
    }
    crate::macos::macos_native::get_bundle_url(bundle_id)
        .and_then(|path| {
            std::path::Path::new(&path.to_string())
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_default()
}

#[cfg(not(target_os = "macos"))]
pub fn source_app_name(_bundle_id: &str) -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_height_fits_rows() {
        assert_eq!(recalculate_window_size(3, 40.0, 94.0, 300.0), (300.0, 214.0));
        // empty list still keeps one row for the "No matches" line
        assert_eq!(recalculate_window_size(0, 40.0, 94.0, 300.0), (300.0, 134.0));
        // scrolls after 10 rows
        assert_eq!(recalculate_window_size(25, 40.0, 94.0, 300.0), (300.0, 494.0));
    }

    #[test]
    fn test_edit_filter() {
        assert_eq!(edit_filter("wo", "r"), "wor");
        assert_eq!(edit_filter("wo", "R"), "woR");
        assert_eq!(edit_filter("wo", "\u{8}"), "w");
        assert_eq!(edit_filter("", "\u{8}"), "");
        assert_eq!(edit_filter("wo", "1"), "wo");
        assert_eq!(edit_filter("wo", "\u{f700}"), "wo");
        assert_eq!(edit_filter("wo", "\u{1b}"), "wo");
        assert_eq!(edit_filter("wo", ""), "wo");
        assert_eq!(edit_filter("my", " "), "my ");
    }

    #[test]
    fn test_split_url_for_header() {
        assert_eq!(
            split_url_for_header("https://github.com/slint-ui/slint/pull/123?a=b#c"),
            (
                "github.com".to_string(),
                "/slint-ui/slint/pull/123?a=b#c".to_string()
            )
        );
        assert_eq!(
            split_url_for_header("https://example.com/"),
            ("example.com".to_string(), String::new())
        );
        assert_eq!(split_url_for_header(""), (String::new(), String::new()));
    }

    #[test]
    fn test_avatar_color_hex_matches_avatar_color() {
        let id = "/Applications/Google Chrome.app#Profile 1";
        let hex = avatar_color_hex(id);
        let color = avatar_color(id);
        assert_eq!(
            hex,
            format!("#{:02x}{:02x}{:02x}", color.red(), color.green(), color.blue())
        );
        assert_eq!(hex.len(), 7);
    }

    #[test]
    fn test_profile_initial() {
        assert_eq!(profile_initial("work"), "W");
        assert_eq!(profile_initial("  (Default)"), "D");
        assert_eq!(profile_initial(""), "");
    }
}
