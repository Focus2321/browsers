use std::cell::RefCell;
use std::process::exit;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use copypasta::{ClipboardContext, ClipboardProvider};
use slint::{ComponentHandle, Model, ModelRc, VecModel, Weak};

use crate::gui::app_state::{
    UIBrowser, UIProfileAndIncognito, UISettings, UISettingsRule, get_filtered_browsers,
};
use crate::gui::generated::{AboutWindow, MainWindow, SettingsWindow};
use crate::gui::icon_loader::IconLoader;
use crate::gui::{about_window, main_window, screen, settings_window, theme};
#[cfg(target_os = "macos")]
use crate::macos::{native_popup, native_settings};
use crate::{MessageToMain, UrlOpenInfo};

pub type SharedState = Rc<RefCell<AppState>>;

// which link picker shows the link.
// Native is the SwiftUI one on macOS 13+ (see macos::native_popup), the Slint MainWindow then
// still exists for Settings/About/theme code but is never shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupBackend {
    Slint,
    #[cfg(target_os = "macos")]
    Native,
}

pub struct AppState {
    pub main_sender: Sender<MessageToMain>,
    pub main_window: MainWindow,
    pub settings_window: Option<SettingsWindow>,
    pub about_window: Option<AboutWindow>,
    pub icons: IconLoader,

    pub url: String,
    pub browsers: Vec<UIBrowser>,
    pub filtered_browsers: Vec<UIBrowser>,
    pub restorable_app_profiles: Vec<UIBrowser>,
    pub show_set_as_default: bool,
    pub ui_settings: UISettings,
    // the app the link came from, for the header's "from Slack" (empty if unknown)
    pub source_app_name: String,
    // decided again each time the popup is shown, see choose_popup_backend
    pub popup_backend: PopupBackend,

    // macOS launch without a URL on the command line - the popup waits for the GetURL event
    // (see run), and this is cleared once a URL (or the timeout) shows it
    pub awaiting_url: bool,
}

impl AppState {
    // derived from the two window Options rather than tracked separately, so it can never drift
    // out of sync with them
    pub fn quit_on_lost_focus_applies(&self) -> bool {
        self.ui_settings.visual_settings.quit_on_lost_focus
            && !self.settings_open()
            && !self.about_open()
    }

    // the native window (see macos::native_settings) or the Slint one
    pub fn settings_open(&self) -> bool {
        #[cfg(target_os = "macos")]
        if native_settings::settings_is_open() {
            return true;
        }
        self.settings_window.is_some()
    }

    pub fn about_open(&self) -> bool {
        #[cfg(target_os = "macos")]
        if native_settings::about_is_open() {
            return true;
        }
        self.about_window.is_some()
    }

    // macOS only: after use the popup is hidden and the app keeps running, so the next link
    // shows it without a cold launch.
    // Elsewhere the app always quits instead.
    pub fn runs_in_background(&self) -> bool {
        cfg!(target_os = "macos") && self.ui_settings.behavioral_settings.run_in_background
    }

    pub fn popup_visible(&self) -> bool {
        #[cfg(target_os = "macos")]
        if native_popup::is_visible() {
            return true;
        }
        self.main_window.window().is_visible()
    }

    // the native popup when it's available (see native_popup::init) and the setting is on
    fn wanted_popup_backend(&self) -> PopupBackend {
        #[cfg(target_os = "macos")]
        if self.ui_settings.visual_settings.native_popup && native_popup::is_initialized() {
            return PopupBackend::Native;
        }
        PopupBackend::Slint
    }

    fn native_popup_active(&self) -> bool {
        self.popup_backend != PopupBackend::Slint
    }

    // no window on screen at all - the popup's hidden (or not shown yet), and Settings/About are
    // closed
    fn nothing_visible(&self) -> bool {
        !self.popup_visible() && !self.settings_open() && !self.about_open()
    }
}

// how long a macOS launch without a URL waits for the GetURL event before showing the (URL-less)
// popup anyway - it usually arrives within a few ms of the event loop starting
const AWAIT_URL_TIMEOUT: Duration = Duration::from_millis(300);

thread_local! {
    // for logging how long the first appearance of the popup took, see record_launch_time
    static LAUNCHED_AT: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}

// call first thing in main, on the main thread
pub fn record_launch_time() {
    LAUNCHED_AT.with(|launched_at| launched_at.set(Some(Instant::now())));
}

pub fn new(
    main_sender: Sender<MessageToMain>,
    url: String,
    browsers: Vec<UIBrowser>,
    restorable_app_profiles: Vec<UIBrowser>,
    show_set_as_default: bool,
    ui_settings: UISettings,
) -> SharedState {
    let main_window = MainWindow::new().expect("failed to create main window");
    // needs a real VecModel here, not whatever the generated default is - see
    // ui_util::sync_vec_model for why
    main_window.set_browsers(ModelRc::new(VecModel::default()));
    main_window.set_restorable_app_profiles(ModelRc::new(VecModel::default()));
    let filtered_browsers = get_filtered_browsers(&url, &browsers);

    #[cfg(target_os = "macos")]
    native_popup::init(on_native_popup_event);
    // decided once here like the popup, but independent of the "Native popup" setting
    #[cfg(target_os = "macos")]
    native_settings::init(on_native_settings_event);

    let state = Rc::new(RefCell::new(AppState {
        main_sender,
        main_window,
        settings_window: None,
        about_window: None,
        icons: IconLoader::new(),
        url,
        browsers,
        filtered_browsers,
        restorable_app_profiles,
        show_set_as_default,
        ui_settings,
        source_app_name: String::new(),
        popup_backend: PopupBackend::Slint,
        awaiting_url: false,
    }));
    // the native popup's events find the state through this
    APP_STATE.with(|s| *s.borrow_mut() = Some(state.clone()));
    let popup_backend = state.borrow().wanted_popup_backend();
    state.borrow_mut().popup_backend = popup_backend;
    tracing::info!("Popup: {popup_backend:?}");

    apply_theme(&state);
    refresh_main_window_model(&state);
    reposition_main_window(&state);
    wire_main_window(&state);

    state
}

pub fn apply_theme(state: &SharedState) {
    {
        let st = state.borrow();
        let system_is_dark = theme::detect_system_is_dark(&st.main_window);
        let palette = theme::resolve_palette(&st.ui_settings.visual_settings, system_is_dark);
        theme::apply_to_window(&palette, &st.main_window);
        if let Some(w) = &st.settings_window {
            theme::apply_to_window(&palette, w);
        }
        if let Some(w) = &st.about_window {
            theme::apply_to_window(&palette, w);
        }
    }
    // it gets the theme as its "appearance"
    #[cfg(target_os = "macos")]
    if state.borrow().native_popup_active() {
        update_native_popup(state);
    }
    #[cfg(target_os = "macos")]
    {
        settings_window::refresh_native(state);
        about_window::refresh_native(state);
    }
}

// puts the current links/browsers/settings into whichever popup is in use
pub fn refresh_main_window_model(state: &SharedState) {
    // the native popup draws (and filters) its rows from the same state
    #[cfg(target_os = "macos")]
    if state.borrow().native_popup_active() {
        update_native_popup(state);
        return;
    }

    let mut st = state.borrow_mut();
    let show_hotkeys = st.ui_settings.visual_settings.show_hotkeys;
    let filter_text = st.main_window.get_filter_text();

    // take the loader out so it can be borrowed mutably while iterating filtered_browsers by
    // reference, instead of cloning the whole browser list just to satisfy the borrow checker
    let mut icons = std::mem::take(&mut st.icons);
    let items: Vec<_> = st
        .filtered_browsers
        .iter()
        .filter(|b| main_window::matches_filter(b, &filter_text))
        .enumerate()
        .map(|(position, b)| main_window::to_browser_item(b, position, &mut icons, show_hotkeys))
        .collect();
    st.icons = icons;
    let restorable: Vec<_> = st
        .restorable_app_profiles
        .iter()
        .map(main_window::to_restorable_profile)
        .collect();

    let win = st.main_window.clone_strong();
    let show_set_as_default = st.show_set_as_default;
    let url = st.url.clone();
    let source_app_name = st.source_app_name.clone();
    drop(st);

    let browser_count = items.len() as i32;
    crate::gui::ui_util::sync_vec_model(&win.get_browsers(), items);
    crate::gui::ui_util::sync_vec_model(&win.get_restorable_app_profiles(), restorable);
    win.set_show_set_as_default(show_set_as_default);
    let (url_domain, url_path) = main_window::split_url_for_header(&url);
    win.set_url_domain(url_domain.into());
    win.set_url_path(url_path.into());
    win.set_source_app_name(source_app_name.into());
    win.set_icon_size(main_window::get_icon_size());

    // keep the focus index in range as the (filtered) list changes size - after a hide/restore/move,
    // or the url changing
    win.set_focused_index(win.get_focused_index().clamp(0, (browser_count - 1).max(0)));
}

// builds a zero-arg callback handler that just fires a message at main - shared by every
// no-argument callback (refresh, set-browsers-default, ...) that both the main and settings
// windows expose and wire up identically
pub fn forward_handler(
    state: &SharedState,
    to_message: impl Fn() -> MessageToMain + 'static,
) -> impl Fn() + 'static {
    let state = state.clone();
    move || send_to_main(&state, to_message())
}

// builds a one-arg (string id) callback handler that just forwards the id into a message at main -
// shared by every id-forwarding callback (restore-profile, hide-profile, hide-app, ...) that the
// main and settings windows expose and wire up identically
pub fn forward_id_handler(
    state: &SharedState,
    to_message: impl Fn(String) -> MessageToMain + 'static,
) -> impl Fn(slint::SharedString) + 'static {
    let state = state.clone();
    move |id| send_to_main(&state, to_message(id.to_string()))
}

fn send_to_main(state: &SharedState, message: MessageToMain) {
    let _ = state.borrow().main_sender.send(message);
}

// focuses (and shows) an already-open singleton window instead of creating a second instance;
// returns true if it did so, so the caller can bail out of window creation. shared by
// settings_window::open and about_window::open.
pub fn focus_if_open<W: ComponentHandle>(win: Option<W>) -> bool {
    if let Some(w) = win {
        w.window().show().ok();
        true
    } else {
        false
    }
}

// registers a close handler that hides the window and clears the SharedState field holding its
// last strong ref - that's what actually drops the window once its Rc refcount hits zero. shared
// by settings_window::open and about_window::open.
pub fn clear_on_close<W: ComponentHandle + 'static>(
    win: &W,
    state: &SharedState,
    clear: impl Fn(&mut AppState) + 'static,
) {
    let state = state.clone();
    win.window().on_close_requested(move || {
        clear(&mut state.borrow_mut());
        window_closed(&state);
        slint::CloseRequestResponse::HideWindow
    });
}

// after Settings or About (Slint or native) closed
fn window_closed(state: &SharedState) {
    if state.borrow().nothing_visible() {
        on_last_window_closed(state);
    } else {
        // once this window is gone (so it can't take the focus back)
        let state = state.clone();
        slint::Timer::single_shot(Duration::ZERO, move || refocus_popup(&state));
    }
}

// the event loop keeps running without windows on macOS (see run), so it's decided here whether
// closing Settings/About while the popup is hidden quits, like it did before
fn on_last_window_closed(state: &SharedState) {
    if !state.borrow().runs_in_background() {
        exit(crate::QUIT_EXIT_CODE);
    }
    #[cfg(target_os = "macos")]
    crate::macos::macos_native::hide_app();
}

// clears per-appearance state left over from the previous time the popup was shown (the
// incognito modifier could still be held down when it was hidden, for one)
fn reset_popup_state(win: &MainWindow) {
    win.invoke_reset_state();
}

// shows the popup at the mouse cursor for the given URL, from a clean state
pub fn show_popup_for_url(state: &SharedState, url: String) {
    {
        let mut st = state.borrow_mut();
        st.awaiting_url = false;
        st.url = url;
        st.filtered_browsers = get_filtered_browsers(&st.url, &st.browsers);
    }
    match choose_popup_backend(state) {
        PopupBackend::Slint => show_slint_popup(state),
        #[cfg(target_os = "macos")]
        PopupBackend::Native => show_native_popup(state),
    }

    match LAUNCHED_AT.with(|launched_at| launched_at.take()) {
        Some(launched_at) => tracing::debug!(
            "Popup shown, {} ms after launch",
            launched_at.elapsed().as_millis()
        ),
        None => tracing::debug!("Popup shown"),
    }
}

// decided for every appearance, so turning the "Native popup" setting on or off applies from the
// next link on
fn choose_popup_backend(state: &SharedState) -> PopupBackend {
    let mut st = state.borrow_mut();
    let backend = st.wanted_popup_backend();
    if backend != st.popup_backend {
        tracing::info!("Popup: {backend:?}");
        st.popup_backend = backend;
    }
    backend
}

fn show_slint_popup(state: &SharedState) {
    refresh_main_window_model(state);
    let win = state.borrow().main_window.clone_strong();
    reset_popup_state(&win);
    reposition_main_window(state);
    win.window().show().ok();
    #[cfg(target_os = "macos")]
    crate::macos::macos_native::activate_app();
    win.invoke_grab_focus();
}

// Swift positions it at the cursor and resets its filter/selection itself
#[cfg(target_os = "macos")]
fn show_native_popup(state: &SharedState) {
    // built first, so nothing is borrowed while Swift runs (it may call right back)
    let json = native_popup::PopupState::from_app_state(&state.borrow()).to_json();
    crate::macos::macos_native::activate_app();
    native_popup::show(&json);
}

// new content for the native popup, without moving it or resetting its filter/selection
#[cfg(target_os = "macos")]
fn update_native_popup(state: &SharedState) {
    let json = native_popup::PopupState::from_app_state(&state.borrow()).to_json();
    native_popup::update(&json);
}

// the app was launched again while running - shows the popup like a launch without a URL would,
// unless it's already showing a URL
pub fn reopen(state: &SharedState) {
    if state.borrow().popup_visible() {
        refocus_popup(state);
        return;
    }
    state.borrow_mut().source_app_name.clear();
    show_popup_for_url(state, String::new());
}

// hides the popup without quitting (see AppState::runs_in_background), giving focus back to the
// previous app unless Settings/About are still open
pub fn hide_popup(state: &SharedState) {
    // both, in case the "Native popup" setting changed while one of them was showing
    let win = state.borrow().main_window.clone_strong();
    if win.window().is_visible() {
        win.window().hide().ok();
    }
    #[cfg(target_os = "macos")]
    native_popup::hide();
    tracing::debug!("Popup hidden");
    #[cfg(target_os = "macos")]
    if state.borrow().nothing_visible() {
        crate::macos::macos_native::hide_app();
    }
}

// the popup was closed without opening a link (Escape, or focus lost with quit_on_lost_focus)
pub fn dismiss_popup(state: &SharedState) {
    if state.borrow().runs_in_background() {
        hide_popup(state);
    } else {
        exit(crate::QUIT_EXIT_CODE);
    }
}

// another app took focus (a click elsewhere, Cmd+Tab, ...).
// The popup floats above every window, so leaving it up would strand it on screen: while running
// in the background it's always hidden (the next link shows it again instantly), also when
// Settings is open. Otherwise closing means quitting, which "Close when clicking elsewhere" opts
// into.
pub fn on_app_lost_focus(state: &SharedState) {
    let (popup_visible, runs_in_background, quit_applies) = {
        let st = state.borrow();
        (
            st.popup_visible(),
            st.runs_in_background(),
            st.quit_on_lost_focus_applies(),
        )
    };
    // hiding the app ourselves (see hide_popup) resigns active too, with the popup already gone
    if !popup_visible {
        return;
    }
    if runs_in_background {
        hide_popup(state);
    } else if quit_applies {
        dismiss_popup(state);
    }
}

// makes the popup the key window again, so Escape and the arrow keys reach it - closing
// Settings/About doesn't hand the keyboard back to it on its own
fn refocus_popup(state: &SharedState) {
    #[cfg(target_os = "macos")]
    if native_popup::is_visible() {
        crate::macos::macos_native::activate_app();
        native_popup::focus();
        return;
    }
    let win = state.borrow().main_window.clone_strong();
    if !win.window().is_visible() {
        return;
    }
    #[cfg(target_os = "macos")]
    crate::macos::macos_native::activate_app();
    win.window().show().ok();
    win.invoke_grab_focus();
}

// sets the window's pinned/actual size to fit the rows currently shown (after type-to-filter),
// returning that size so callers that also need to reposition (see reposition_main_window) don't
// recompute it
fn apply_main_window_size(win: &MainWindow) -> (f32, f32) {
    let row_height = win
        .global::<crate::gui::generated::Layout>()
        .get_browser_row_height();
    let size = main_window::recalculate_window_size(
        win.get_browsers().row_count(),
        row_height,
        win.get_chrome_height(),
        win.get_popup_width(),
    );

    // keeps min-size == max-size in main_window.slint, so winit's own resizable-from-constraints
    // logic never re-derives "resizable" from the window's natural (much wider) content bounds
    win.set_pinned_width(size.0);
    win.set_pinned_height(size.1);
    win.window()
        .set_size(slint::LogicalSize::new(size.0, size.1));
    size
}

// resizes and moves the window to the cursor - only for a genuinely new appearance (initial
// launch, or a new URL opened while already running), not a same-URL refresh of an already-visible
// window
pub fn reposition_main_window(state: &SharedState) {
    // the native popup places and sizes itself
    if state.borrow().native_popup_active() {
        return;
    }
    let win = state.borrow().main_window.clone_strong();

    let (mouse, work_area) = screen::mouse_position_and_work_area();
    let size = apply_main_window_size(&win);
    let position = main_window::calculate_window_position(mouse, work_area, size);

    // the mouse position is fractional, and a window at a fractional position got a backing
    // surface one pixel wider than its size, which winit then kept correcting back and forth
    win.window().set_position(slint::LogicalPosition::new(
        position.x.round(),
        position.y.round(),
    ));
}

// resizes for the current browser count without moving it - for refreshing an already-visible
// window's content (Refresh, restoring a hidden profile)
pub fn resize_main_window(state: &SharedState) {
    if state.borrow().native_popup_active() {
        return;
    }
    let win = state.borrow().main_window.clone_strong();
    apply_main_window_size(&win);
}

fn wire_main_window(state: &SharedState) {
    let win = state.borrow().main_window.clone_strong();

    {
        let state = state.clone();
        win.on_open_browser(move |profile_index, private| {
            open_browser(&state, profile_index as i64, private);
        });
    }
    {
        let state = state.clone();
        win.on_activate_focused(move || {
            let idx = state.borrow().main_window.get_focused_index();
            open_filtered(&state, idx);
        });
    }
    {
        let state = state.clone();
        win.on_open_hotkey(move |hotkey_index| open_filtered(&state, hotkey_index));
    }
    {
        let state = state.clone();
        win.on_copy_link(move || copy_link(&state));
    }
    {
        let state = state.clone();
        win.on_filter_changed(move || {
            refresh_main_window_model(&state);
            let win = state.borrow().main_window.clone_strong();
            // the best match is the first row, same as when the popup opens
            win.set_focused_index(0);
            resize_main_window(&state);
        });
    }
    win.on_edit_filter(|current, key| main_window::edit_filter(&current, &key).into());
    {
        let state = state.clone();
        win.on_always_open_here(move |profile_id, profile_index| {
            always_open_here(&state, profile_id.to_string(), profile_index as i64);
        });
    }
    win.on_refresh(forward_handler(state, || MessageToMain::Refresh));
    win.on_set_browsers_default(forward_handler(state, || {
        MessageToMain::SetBrowsersAsDefaultBrowser
    }));
    win.on_hide_profile(forward_id_handler(state, MessageToMain::HideAppProfile));
    win.on_hide_app(forward_id_handler(state, MessageToMain::HideAllProfiles));
    win.on_restore_profile(forward_id_handler(state, MessageToMain::RestoreAppProfile));
    {
        let state = state.clone();
        win.on_show_settings(move || {
            settings_window::open(&state);
        });
    }
    {
        let state = state.clone();
        win.on_show_about(move || {
            about_window::open(&state);
        });
    }
    {
        let state = state.clone();
        win.on_dismiss(move || dismiss_popup(&state));
    }
    // the menu's Quit always quits, also while running in the background
    win.on_quit(move || {
        exit(crate::QUIT_EXIT_CODE);
    });
}

// the native Settings and About windows' events.
// Closing runs what closing the Slint windows does (see clear_on_close), the rest goes to the
// handlers the Slint Settings window's callbacks use.
#[cfg(target_os = "macos")]
fn on_native_settings_event(event: native_settings::SettingsEvent) {
    use native_settings::SettingsEvent;

    // cloned out, so APP_STATE itself isn't borrowed while the event is handled
    let Some(state) = APP_STATE.with(|s| s.borrow().clone()) else {
        return;
    };
    match event {
        SettingsEvent::SettingsClosed | SettingsEvent::AboutClosed => window_closed(&state),
        event => settings_window::handle_native_event(&state, event),
    }
}

// the native popup's events, mapped to the same actions as the Slint popup's callbacks in
// wire_main_window.
// Swift calls this on the main thread, from inside the AppKit event loop winit runs.
#[cfg(target_os = "macos")]
fn on_native_popup_event(event: native_popup::PopupEvent) {
    use native_popup::PopupEvent;

    // cloned out, so APP_STATE itself isn't borrowed while the event is handled
    let Some(state) = APP_STATE.with(|s| s.borrow().clone()) else {
        return;
    };
    match event {
        PopupEvent::Open {
            profile_index,
            private,
        } => open_browser(&state, profile_index, private),
        PopupEvent::CopyLink => copy_link(&state),
        PopupEvent::AlwaysOpenHere {
            unique_id,
            profile_index,
        } => always_open_here(&state, unique_id, profile_index),
        PopupEvent::HideProfile { unique_id } => {
            send_to_main(&state, MessageToMain::HideAppProfile(unique_id))
        }
        PopupEvent::HideApp { unique_app_id } => {
            send_to_main(&state, MessageToMain::HideAllProfiles(unique_app_id))
        }
        PopupEvent::RestoreProfile { unique_id } => {
            send_to_main(&state, MessageToMain::RestoreAppProfile(unique_id))
        }
        PopupEvent::Refresh => send_to_main(&state, MessageToMain::Refresh),
        PopupEvent::SetDefault => send_to_main(&state, MessageToMain::SetBrowsersAsDefaultBrowser),
        PopupEvent::Settings => settings_window::open(&state),
        PopupEvent::About => about_window::open(&state),
        // always quits, also while running in the background
        PopupEvent::Quit => exit(crate::QUIT_EXIT_CODE),
        PopupEvent::Dismiss => dismiss_popup(&state),
    }
}

fn copy_link(state: &SharedState) {
    let url = state.borrow().url.clone();
    if let Ok(mut ctx) = ClipboardContext::new() {
        let _ = ctx.set_contents(url);
    }
}

// opens the browser at the given position in the list as shown in the window (after the url and
// type-to-filter narrowing)
fn open_filtered(state: &SharedState, index: i32) {
    let win = state.borrow().main_window.clone_strong();
    let profile_index = usize::try_from(index)
        .ok()
        .and_then(|i| win.get_browsers().row_data(i))
        .map(|item| item.browser_profile_index);
    if let Some(profile_index) = profile_index {
        open_browser(state, profile_index as i64, win.get_incognito_mode());
    }
}

fn open_browser(state: &SharedState, browser_profile_index: i64, private: bool) {
    let Ok(index) = usize::try_from(browser_profile_index) else {
        return;
    };
    let st = state.borrow();
    // rows without a private mode are faded out while ⇧ is held - opening one of those in a
    // regular window instead would quietly break the "private" the user asked for
    let supports_private = st.browsers.get(index).is_some_and(|b| b.supports_incognito);
    if private && !supports_private {
        return;
    }
    let _ = st
        .main_sender
        .send(MessageToMain::OpenLink(index, private, st.url.clone()));
}

// "Always open <domain> here": saves an opening rule for the link's domain pointing at this
// profile, then opens the link in it
fn always_open_here(state: &SharedState, profile_id: String, browser_profile_index: i64) {
    let domain = {
        let st = state.borrow();
        main_window::split_url_for_header(&st.url).0
    };
    if domain.is_empty() {
        return;
    }

    {
        let mut st = state.borrow_mut();
        // keep the in-memory copy Settings shows in sync with what gets saved
        let rules = &mut st.ui_settings.rules;
        let opener = Some(UIProfileAndIncognito {
            profile: profile_id.clone(),
            incognito: false,
        });
        match rules
            .iter_mut()
            .find(|r| r.source_app.is_empty() && r.url_pattern == domain)
        {
            Some(rule) => rule.opener = opener,
            None => rules.push(UISettingsRule {
                source_app: String::new(),
                url_pattern: domain.clone(),
                opener,
            }),
        }
        let _ = st
            .main_sender
            .send(MessageToMain::AddRuleForDomain { domain, profile_id });
    }
    if state.borrow().settings_open() {
        settings_window::refresh(state);
    }

    open_browser(state, browser_profile_index, false);
}

thread_local! {
    // set in new().
    // Lets UiHandle stay Send without unsafe impl - SharedState itself never crosses threads,
    // only this thread-local handle to it does.
    // The native popup's events (see on_native_popup_event) also come in through it.
    static APP_STATE: RefCell<Option<SharedState>> = const { RefCell::new(None) };
}

// handle the background "main" thread uses to push updates onto the Slint UI thread
#[derive(Clone)]
pub struct UiHandle {
    main_weak: Weak<MainWindow>,
}

impl UiHandle {
    pub fn new(state: &SharedState) -> Self {
        UiHandle {
            main_weak: state.borrow().main_window.as_weak(),
        }
    }

    fn with_state(&self, f: impl FnOnce(&SharedState) + Send + 'static) {
        let _ = self.main_weak.upgrade_in_event_loop(move |_| {
            APP_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    f(state);
                }
            });
        });
    }

    // visible + hidden profiles and the saved order in one go, so the popup and Settings
    // always update together after a hide/restore/move/rescan
    pub fn profiles_received(
        &self,
        browsers: Vec<UIBrowser>,
        hidden: Vec<UIBrowser>,
        profile_order: Vec<String>,
    ) {
        self.with_state(move |state| {
            {
                let mut st = state.borrow_mut();
                st.browsers = browsers;
                st.filtered_browsers = get_filtered_browsers(&st.url, &st.browsers);
                st.restorable_app_profiles = hidden;
                st.ui_settings.profile_order = profile_order;
            }
            refresh_main_window_model(state);
            resize_main_window(state);
            settings_window::refresh(state);
        });
    }

    pub fn cleaned_url_opened(&self, info: UrlOpenInfo) {
        self.with_state(move |state| {
            let source_app_name = main_window::source_app_name(&info.source_bundle_id);
            state.borrow_mut().source_app_name = source_app_name;
            show_popup_for_url(state, info.url);
        });
    }

    // a URL was dealt with without the popup - opened by a matching rule, or ignored
    pub fn url_handled_without_popup(&self) {
        self.with_state(|state| {
            state.borrow_mut().awaiting_url = false;
            let (runs_in_background, nothing_visible) = {
                let st = state.borrow();
                (st.runs_in_background(), st.nothing_visible())
            };
            if !runs_in_background && nothing_visible {
                exit(crate::QUIT_EXIT_CODE);
            }
        });
    }

    pub fn open_link_completed(&self) {
        self.with_state(|state| {
            if state.borrow().runs_in_background() {
                hide_popup(state);
            } else {
                exit(crate::QUIT_EXIT_CODE);
            }
        });
    }
}

// with `await_url` the popup isn't shown until a URL arrives (see AWAIT_URL_TIMEOUT), so it
// doesn't flash up empty first, and never shows at all when an opening rule handles the URL
pub fn run(state: &SharedState, await_url: bool) {
    let win = state.borrow().main_window.clone_strong();
    if await_url {
        state.borrow_mut().awaiting_url = true;
        let state = state.clone();
        slint::Timer::single_shot(AWAIT_URL_TIMEOUT, move || {
            if state.borrow().awaiting_url {
                tracing::debug!("No URL arrived, showing the popup without one");
                let url = state.borrow().url.clone();
                show_popup_for_url(&state, url);
            }
        });
    } else {
        let url = state.borrow().url.clone();
        show_popup_for_url(state, url);
    }

    {
        let state = state.clone();
        on_native_window(
            &win,
            "could not obtain the native window - initial theme/resize setup was skipped",
            move |winit_window| {
                // re-apply the theme now that the native window actually exists - the eager call
                // in new() runs before the OS dark/light watcher is wired up and can read a stale
                // Palette.color-scheme (saw it launch light even with dark mode + "Match System"
                // on)
                apply_theme(&state);

                // resize-border-width: 0 only removes the cursor hint, doesn't stop OS resizing -
                // dragging this borderless popup hit a real Slint bug (slint-ui/slint#3990),
                // easier to just turn off resizing than fight it
                winit_window.set_resizable(false);
            },
        );
    }

    make_window_floating(&win);

    #[cfg(target_os = "windows")]
    hide_main_window_from_taskbar(&win);

    // on macOS the app can keep running with all windows hidden (see
    // AppState::runs_in_background), quitting is always an explicit exit() there
    #[cfg(target_os = "macos")]
    slint::run_event_loop_until_quit().ok();
    #[cfg(not(target_os = "macos"))]
    slint::run_event_loop().ok();
}

// waits for the native window to exist (it doesn't until run_event_loop() starts, so this
// spawn_local's an async wait instead of polling with a timer) and then calls `f` with it -
// shared by every call site below that needs a real winit window handle. Logs `warn_msg` if the
// handle never becomes available.
fn on_native_window<T, F>(win: &T, warn_msg: &'static str, f: F)
where
    T: ComponentHandle + 'static,
    F: FnOnce(Arc<slint::winit_030::winit::window::Window>) + 'static,
{
    use slint::winit_030::WinitWindowAccessor;

    let win_weak = win.as_weak();
    let _ = slint::spawn_local(async move {
        let Some(win) = win_weak.upgrade() else {
            return;
        };
        match win.window().winit_window().await {
            Ok(winit_window) => f(winit_window),
            // expected with a non-winit backend, e.g. SLINT_BACKEND=headless for the MCP server
            Err(err)
                if err
                    .to_string()
                    .contains("not backed by a Winit window adapter") =>
            {
                tracing::debug!("{warn_msg}: {err}")
            }
            Err(err) => tracing::warn!("{warn_msg}: {err}"),
        }
    });
}

// always-on-top isn't documented for macOS in Slint and doesn't cover per-Space visibility or the
// Windows taskbar either, so this goes through winit directly instead.
//
// Called for the main window, Settings, and About (see call sites in settings_window.rs/
// about_window.rs) - Settings/About need the same floating level as the main window, otherwise
// they render *behind* it whenever both are visible: NSWindowLevel stacking is absolute, not
// affected by which window is active, so a normal-level window can never appear in front of a
// floating-level one. No-op on non-macOS platforms - see macos_native::make_window_floating for
// what this actually sets and why.
#[cfg(not(target_os = "macos"))]
pub fn make_window_floating<T: ComponentHandle + 'static>(_win: &T) {}

#[cfg(target_os = "macos")]
pub fn make_window_floating<T: ComponentHandle + 'static>(win: &T) {
    use slint::winit_030::winit;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    on_native_window(
        win,
        "could not obtain the native window - it will not float above other windows",
        |winit_window| {
            if let Ok(handle) = winit_window.window_handle()
                && let RawWindowHandle::AppKit(appkit) = handle.as_raw()
            {
                crate::macos::macos_native::make_window_floating(appkit.ns_view.as_ptr());
            }
        },
    );
}

// best-effort hides the window from the Windows taskbar - unverified, never actually run on
// Windows from this (macOS) session, see TODO.md
#[cfg(target_os = "windows")]
fn hide_main_window_from_taskbar(win: &MainWindow) {
    use slint::winit_030::winit;
    use winit::platform::windows::WindowExtWindows;

    on_native_window(
        win,
        "could not obtain the native window - it will still show in the taskbar",
        |winit_window| winit_window.set_skip_taskbar(true),
    );
}
