//! Window-level platform behavior: the macOS menu bar (app/bootstrap.rs,
//! app/menu_state.rs), the Dock badge (app/dock_badge.rs), remembered window
//! bounds (app/window.rs, services/window_preferences*) and the Settings
//! window (views/settings_window.rs).

use std::sync::{Arc, Mutex};

use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, Theme, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
    menu::{
        CheckMenuItem, CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem,
        SubmenuBuilder,
    },
};

use super::Intent;
use crate::{
    models::settings_model::{SidebarMode, SidebarStyle, ThemeMode},
    services::{
        window_preferences::WindowPreferences, window_preferences_writer::WindowPreferencesWriter,
    },
};

pub const MAIN_WINDOW: &str = "main";
pub const SETTINGS_WINDOW: &str = "settings";
/// views/mod.rs `WINDOW_MIN_WIDTH_PX` / `WINDOW_MIN_HEIGHT_PX`.
pub const WINDOW_MIN_WIDTH_PX: f64 = 800.;
pub const WINDOW_MIN_HEIGHT_PX: f64 = 680.;
/// app/window.rs `WINDOW_DEFAULT_HEIGHT_PX`.
const WINDOW_DEFAULT_HEIGHT_PX: f64 = 880.;
/// views/settings_window.rs.
const SETTINGS_WIDTH_PX: f64 = 640.;
const SETTINGS_HEIGHT_PX: f64 = 780.;

// Menu item ids, and the titles app/bootstrap.rs gives them.
const CHECK_FOR_UPDATES: &str = "check-for-updates";
const SETTINGS: &str = "settings";
const QUIT: &str = "quit";
const SORT_BY_RECENT: &str = "sort-by-recent";
const GROUP_BY_TEAM: &str = "group-by-team";
pub const VIEW_MENU: &str = "View";
pub const SORT_BY_RECENT_ITEM: &str = "Sort by Recent";
pub const GROUP_BY_TEAM_ITEM: &str = "Group by Team";

/// Native state the engine keeps in step with the models.
#[derive(Default)]
pub struct Native {
    sort_by_recent: Option<CheckMenuItem<tauri::Wry>>,
    group_by_team: Option<CheckMenuItem<tauri::Wry>>,
    /// The order the View menu's check mark shows (`view_menu_mode`).
    view_menu_mode: Option<SidebarMode>,
    /// The count the Dock icon's badge shows (`dock_badge_count`).
    dock_badge_count: Option<u32>,
    theme_mode: Option<ThemeMode>,
    writer: Option<Arc<WindowBounds>>,
}

/// `AppWindow`'s window-preference fields, shared with the window event handler.
struct WindowBounds {
    writer: WindowPreferencesWriter,
    preferred: Mutex<Option<WindowPreferences>>,
    /// From the right pane: the width before it widened the window, and
    /// whether a resize in flight is the app's own (`PaneWindowSizing`).
    pane: Mutex<(Option<f32>, bool)>,
}

impl std::fmt::Debug for Native {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Native")
            .field("view_menu_mode", &self.view_menu_mode)
            .field("dock_badge_count", &self.dock_badge_count)
            .finish_non_exhaustive()
    }
}

/// Sends a shell intent to the engine from a native callback (menu, window events).
fn send(app: &AppHandle, intent: Intent) {
    if let Some(engine) = app.try_state::<crate::Engine>() {
        let _ = engine.0.send(crate::engine::Intent::Shell(intent));
    }
}

impl Native {
    /// Keeps the window-bounds handler in step with the right pane's sizing.
    pub fn set_pane_sizing(&self, base: Option<f32>, automatic: bool) {
        if let Some(bounds) = &self.writer {
            *bounds.pane.lock().unwrap() = (base, automatic);
        }
    }

    /// Menu bar, window bounds and window listeners, as `bootstrap::run` and
    /// `open_main_window` set them up.
    pub fn install(&mut self, app: &AppHandle, sidebar_style: SidebarStyle) {
        if let Err(error) = self.install_menu(app) {
            tracing::warn!("could not build the menu bar: {error}");
        }
        let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
            return;
        };
        let _ = window.set_min_size(Some(LogicalSize::new(
            WINDOW_MIN_WIDTH_PX,
            WINDOW_MIN_HEIGHT_PX,
        )));
        let bounds = Arc::new(WindowBounds {
            writer: WindowPreferencesWriter::default(),
            preferred: Mutex::new(None),
            pane: Mutex::new((None, false)),
        });
        restore_bounds(&window, sidebar_style, &bounds);
        let _ = window.show();
        let _ = window.set_focus();
        let handler_bounds = Arc::clone(&bounds);
        let handler_window = window.clone();
        window.on_window_event(move |event| match event {
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                remember_bounds(&handler_window, &handler_bounds);
            }
            WindowEvent::CloseRequested { .. } => {
                // `on_window_should_close`: finish the pending write first.
                let _ = handler_bounds.writer.flush().blocking_recv();
            }
            _ => {}
        });
        self.writer = Some(bounds);
    }

    fn install_menu(&mut self, app: &AppHandle) -> tauri::Result<()> {
        let sort_by_recent = CheckMenuItemBuilder::with_id(SORT_BY_RECENT, SORT_BY_RECENT_ITEM)
            .accelerator("Alt+CmdOrCtrl+1")
            .build(app)?;
        let group_by_team = CheckMenuItemBuilder::with_id(GROUP_BY_TEAM, GROUP_BY_TEAM_ITEM)
            .accelerator("Alt+CmdOrCtrl+2")
            .build(app)?;
        let app_menu = SubmenuBuilder::new(app, "zBase")
            .item(&MenuItemBuilder::with_id(CHECK_FOR_UPDATES, "Check for Updates…").build(app)?)
            .item(
                &MenuItemBuilder::with_id(SETTINGS, "Settings…")
                    .accelerator("CmdOrCtrl+,")
                    .build(app)?,
            )
            .separator()
            .item(&PredefinedMenuItem::services(app, None)?)
            .separator()
            .item(&PredefinedMenuItem::hide(app, None)?)
            .item(&PredefinedMenuItem::hide_others(app, None)?)
            .item(&PredefinedMenuItem::show_all(app, None)?)
            .separator()
            .item(
                &MenuItemBuilder::with_id(QUIT, "Quit zBase")
                    .accelerator("CmdOrCtrl+Q")
                    .build(app)?,
            )
            .build()?;
        // A web view only gets copy, paste and undo through these items; gpui's
        // text fields bind them in key_bindings.rs instead.
        let edit_menu = SubmenuBuilder::new(app, "Edit")
            .undo()
            .redo()
            .separator()
            .cut()
            .copy()
            .paste()
            .select_all()
            .build()?;
        let view_menu = SubmenuBuilder::new(app, VIEW_MENU)
            .item(&sort_by_recent)
            .item(&group_by_team)
            .build()?;
        let window_menu = SubmenuBuilder::new(app, "Window")
            .minimize()
            .maximize()
            .separator()
            .close_window()
            .build()?;
        let menu = MenuBuilder::new(app)
            .items(&[&app_menu, &edit_menu, &view_menu, &window_menu])
            .build()?;
        app.set_menu(menu)?;
        app.on_menu_event(|app, event| {
            let intent = match event.id().as_ref() {
                CHECK_FOR_UPDATES => Intent::CheckForUpdates,
                SETTINGS => Intent::OpenPreferences,
                QUIT => Intent::Quit,
                SORT_BY_RECENT => Intent::SetSidebarMode {
                    mode: "Recent".into(),
                },
                GROUP_BY_TEAM => Intent::SetSidebarMode {
                    mode: "Grouped".into(),
                },
                _ => return,
            };
            send(app, intent);
        });
        self.sort_by_recent = Some(sort_by_recent);
        self.group_by_team = Some(group_by_team);
        Ok(())
    }

    /// `sync_view_menu`: moves the View menu's check mark to the current order.
    /// Check items toggle themselves when chosen, so this also runs every tick.
    pub fn sync_view_menu(&mut self, mode: SidebarMode) {
        let (Some(recent), Some(grouped)) = (&self.sort_by_recent, &self.group_by_team) else {
            return;
        };
        let recent_on = mode == SidebarMode::Recent;
        if self.view_menu_mode == Some(mode)
            && recent.is_checked().ok() == Some(recent_on)
            && grouped.is_checked().ok() == Some(!recent_on)
        {
            return;
        }
        self.view_menu_mode = Some(mode);
        let _ = recent.set_checked(recent_on);
        let _ = grouped.set_checked(!recent_on);
    }

    /// `sync_dock_badge`.
    pub fn sync_dock_badge(&mut self, app: &AppHandle, count: u32) {
        if self.dock_badge_count == Some(count) {
            return;
        }
        let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
            return;
        };
        self.dock_badge_count = Some(count);
        let _ = window.set_badge_count((count > 0).then_some(i64::from(count)));
    }

    /// The native window appearance (title bar, traffic lights, scroll bars)
    /// follows Settings › Theme, so `prefers-color-scheme` matches it too.
    pub fn sync_theme(&mut self, app: &AppHandle, mode: &ThemeMode) {
        if self.theme_mode.as_ref() == Some(mode) {
            return;
        }
        self.theme_mode = Some(mode.clone());
        let theme = match mode {
            ThemeMode::Light => Some(Theme::Light),
            ThemeMode::Dark => Some(Theme::Dark),
            ThemeMode::System => None,
        };
        for window in app.webview_windows().values() {
            let _ = window.set_theme(theme);
        }
    }

    /// Waits for the pending window-bounds write, as `quit_app_action` does.
    pub fn flush_window_preferences(&self) {
        if let Some(bounds) = &self.writer {
            let _ = bounds.writer.flush().blocking_recv();
        }
    }
}

/// `open_main_window`: the saved bounds, kept on a connected display, or a
/// centered default wide enough for the sidebar style.
fn restore_bounds(window: &WebviewWindow, style: SidebarStyle, bounds: &WindowBounds) {
    let default_width = WINDOW_MIN_WIDTH_PX
        + f64::from(crate::models::sidebar_model::sidebar_width_for_style(style))
        - f64::from(crate::models::sidebar_model::SIDEBAR_WIDTH_PX);
    let mut displays = window
        .available_monitors()
        .unwrap_or_default()
        .into_iter()
        .collect::<Vec<_>>();
    let primary = window.primary_monitor().ok().flatten();
    if let Some(primary) = primary.as_ref() {
        displays.sort_by_key(|display| display.position() != primary.position());
    }
    let displays = displays
        .iter()
        .map(|display| {
            let scale = display.scale_factor();
            let position = display.position().to_logical::<f64>(scale);
            let size = display.size().to_logical::<f64>(scale);
            WindowPreferences {
                x: position.x as f32,
                y: position.y as f32,
                width: size.width as f32,
                height: size.height as f32,
            }
        })
        .collect::<Vec<_>>();
    let default = displays.first().map_or(
        WindowPreferences {
            x: 0.,
            y: 0.,
            width: default_width as f32,
            height: WINDOW_DEFAULT_HEIGHT_PX as f32,
        },
        |display| WindowPreferences {
            x: display.x + (display.width - default_width as f32).max(0.) / 2.,
            y: display.y + (display.height - WINDOW_DEFAULT_HEIGHT_PX as f32).max(0.) / 2.,
            width: default_width as f32,
            height: WINDOW_DEFAULT_HEIGHT_PX as f32,
        },
    );
    let preferred = WindowPreferences::load().unwrap_or(default).constrained(
        &displays,
        (WINDOW_MIN_WIDTH_PX as f32, WINDOW_MIN_HEIGHT_PX as f32),
    );
    let _ = window.set_size(LogicalSize::new(
        f64::from(preferred.width),
        f64::from(preferred.height),
    ));
    let _ = window.set_position(LogicalPosition::new(
        f64::from(preferred.x),
        f64::from(preferred.y),
    ));
    *bounds.preferred.lock().unwrap() = Some(preferred);
}

/// `remember_window_preferences`: windowed bounds only, never below the minimum.
fn remember_bounds(window: &WebviewWindow, bounds: &WindowBounds) {
    if window.is_fullscreen().unwrap_or(false)
        || window.is_maximized().unwrap_or(false)
        || window.is_minimized().unwrap_or(false)
    {
        return;
    }
    let Ok(scale) = window.scale_factor() else {
        return;
    };
    let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size()) else {
        return;
    };
    let position = position.to_logical::<f64>(scale);
    let size = size.to_logical::<f64>(scale);
    // A pane's temporary space is not the user's preferred size, and the
    // app's own resizes do not change it.
    let (pane_base, automatic) = *bounds.pane.lock().unwrap();
    if automatic {
        return;
    }
    let width = pane_base.map_or(size.width, f64::from);
    let observed = WindowPreferences {
        x: position.x as f32,
        y: position.y as f32,
        width: (width.max(WINDOW_MIN_WIDTH_PX)) as f32,
        height: (size.height.max(WINDOW_MIN_HEIGHT_PX)) as f32,
    };
    let mut preferred = bounds.preferred.lock().unwrap();
    if *preferred == Some(observed) {
        return;
    }
    *preferred = Some(observed);
    bounds.writer.update(observed);
}

/// `open_settings_window`: brings Settings forward, or opens it centered over
/// the main window, 48px below its top, as a fixed-size window.
pub fn open_settings_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let mut builder = WebviewWindowBuilder::new(
        app,
        SETTINGS_WINDOW,
        WebviewUrl::App("index.html?window=settings".into()),
    )
    .title("Settings")
    .inner_size(SETTINGS_WIDTH_PX, SETTINGS_HEIGHT_PX)
    .resizable(false)
    .minimizable(false)
    .maximizable(false);
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true)
            .traffic_light_position(LogicalPosition::new(14., 13.));
    }
    if let Some(main) = app.get_webview_window(MAIN_WINDOW)
        && let (Ok(scale), Ok(position), Ok(size)) = (
            main.scale_factor(),
            main.outer_position(),
            main.outer_size(),
        )
    {
        let position = position.to_logical::<f64>(scale);
        let size = size.to_logical::<f64>(scale);
        builder = builder.position(
            position.x + size.width / 2. - SETTINGS_WIDTH_PX / 2.,
            position.y + 48.,
        );
    } else {
        builder = builder.center();
    }
    match builder.build() {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(error) => tracing::warn!("failed to open settings window: {error}"),
    }
}

pub fn close_window(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.close();
    }
}
