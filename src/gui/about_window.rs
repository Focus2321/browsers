use slint::{ComponentHandle, ModelRc, VecModel};

use crate::gui::app::{self, SharedState};
use crate::gui::generated::{AboutWindow, DirEntry};
#[cfg(target_os = "macos")]
use crate::macos::native_settings;
use crate::paths;

const VERSION: &str = env!("CARGO_PKG_VERSION");

// the About copy that isn't laid out in ui/about_window.slint (the native About gets all of it
// from Rust)
pub const APP_NAME: &str = "Browsers";
pub const COPYRIGHT: &str = "© 2022-2024 Browsers.software team.";
pub const WEBSITE_URL: &str = "https://browsers.software";

pub fn version_text() -> String {
    format!("Version {VERSION}")
}

// (label, path) pairs
pub fn directories() -> Vec<(String, String)> {
    // .join("") adds a trailing "/", indicating for the user that it's a directory
    let config = paths::get_config_root_dir().join("");
    let cache = paths::get_cache_root_dir().join("");
    let logs = paths::get_logs_root_dir().join("");
    let resources = paths::get_resources_basedir().join("");

    vec![
        ("Config".to_string(), config.to_string_lossy().to_string()),
        ("Cache".to_string(), cache.to_string_lossy().to_string()),
        ("Logs".to_string(), logs.to_string_lossy().to_string()),
        ("Resources".to_string(), resources.to_string_lossy().to_string()),
    ]
}

pub fn directories_entries() -> Vec<DirEntry> {
    directories()
        .into_iter()
        .map(|(label, path)| DirEntry {
            label: label.into(),
            path: path.into(),
        })
        .collect()
}

pub fn open(state: &SharedState) {
    // the native About shows (or brings to front) itself, see native_settings
    #[cfg(target_os = "macos")]
    if native_settings::is_initialized() {
        native_settings::about_show(&native_state_json(state));
        return;
    }

    // state.about_window gets cleared to None on actual close, so Some here only ever means
    // "currently showing"
    let already_open = state
        .borrow()
        .about_window
        .as_ref()
        .map(|w| w.clone_strong());
    if app::focus_if_open(already_open) {
        return;
    }

    let win = AboutWindow::new().expect("failed to create about window");

    // reuse the app's shared icon cache rather than a throwaway loader - it's already populated
    // for the main window's icons, and IconLoader caches by path anyway
    win.set_app_icon(
        state
            .borrow_mut()
            .icons
            .load(&paths::get_app_icon_path().to_string_lossy()),
    );
    win.set_version_text(version_text().into());
    win.set_directories(ModelRc::new(VecModel::from(directories_entries())));

    state.borrow_mut().about_window = Some(win.clone_strong());
    app::apply_theme(state);
    win.window().show().ok();

    // otherwise this renders behind the always-on-top main popup - see app::make_window_floating
    app::make_window_floating(&win);

    app::clear_on_close(&win, state, |st| st.about_window = None);
}

#[cfg(target_os = "macos")]
fn native_state_json(state: &SharedState) -> std::ffi::CString {
    let theme = state.borrow().ui_settings.visual_settings.theme;
    native_settings::about_state_json(theme, native_settings::AboutInfo::current())
}

// sends the native About its current state (the theme is the only thing that changes), if it's
// open
#[cfg(target_os = "macos")]
pub fn refresh_native(state: &SharedState) {
    if native_settings::about_is_open() {
        native_settings::about_update(&native_state_json(state));
    }
}
