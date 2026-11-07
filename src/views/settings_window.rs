//! The Settings window (⌘, or zBase > Settings…): a separate fixed-size macOS
//! window, so changing a preference never replaces the conversation. The main
//! window owns the settings; this window renders them and writes changes back.
use crate::{
    app::{commands, theme::resolve_theme},
    views::{
        app_window::AppWindow, is_dark_theme, preferences::PreferencesView, shell_border,
        text_primary, with_theme,
    },
};
use gpui::{
    AnyWindowHandle, App, AppContext, Bounds, Context, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Pixels, Render, Styled, Subscription,
    TitlebarOptions, WeakEntity, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle,
    WindowOptions, div, point, px, rgb, size,
};

pub const SETTINGS_WINDOW_TITLE: &str = "Settings";
const WIDTH_PX: f32 = 640.;
const HEIGHT_PX: f32 = 780.;
/// Height of the drawn title strip; the traffic lights are centered in it.
const TITLE_STRIP_PX: f32 = 40.;

pub struct SettingsWindow {
    app: WeakEntity<AppWindow>,
    main_window: AnyWindowHandle,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl SettingsWindow {
    /// Opens the Settings window centered over `main_bounds`, observing `app`
    /// so the controls always show the current settings.
    pub fn open(
        app: WeakEntity<AppWindow>,
        main_window: AnyWindowHandle,
        main_bounds: Bounds<Pixels>,
        cx: &mut App,
    ) -> Option<WindowHandle<Self>> {
        let window_size = size(px(WIDTH_PX), px(HEIGHT_PX));
        let origin = point(
            main_bounds.center().x - window_size.width / 2.,
            main_bounds.origin.y + px(48.),
        );
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(origin, window_size))),
            titlebar: Some(TitlebarOptions {
                title: Some(SETTINGS_WINDOW_TITLE.into()),
                appears_transparent: cfg!(target_os = "macos"),
                traffic_light_position: cfg!(target_os = "macos").then_some(point(px(14.), px(13.))),
            }),
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Opaque,
            ..Default::default()
        };
        let handle = cx.open_window(options, |window, cx| {
            let view = cx.new(|cx| {
                let mut subscriptions = Vec::new();
                if let Some(app) = app.upgrade() {
                    subscriptions.push(cx.observe(&app, |_, _, cx| cx.notify()));
                }
                subscriptions.push(cx.observe_window_appearance(window, |_, _, cx| cx.notify()));
                Self {
                    app,
                    main_window,
                    focus_handle: cx.focus_handle(),
                    _subscriptions: subscriptions,
                }
            });
            window.focus(&view.focus_handle(cx));
            view
        })
        .inspect_err(|error| tracing::warn!("failed to open settings window: {error}"))
        .ok()?;
        // Make it key even when it was requested from a menu while another app
        // was frontmost.
        cx.activate(true);
        let _ = handle.update(cx, |_, window, _| window.activate_window());
        Some(handle)
    }

    fn close(&mut self, _: &commands::CloseWindow, window: &mut Window, _: &mut Context<Self>) {
        window.remove_window();
    }

    /// Quitting goes through the main window so it can flush its own state first.
    fn quit(&mut self, _: &commands::QuitApp, _: &mut Window, cx: &mut Context<Self>) {
        let _ = self.main_window.update(cx, |_, window, cx| {
            window.dispatch_action(Box::new(commands::QuitApp), cx);
        });
    }

    fn check_for_updates(
        &mut self,
        _: &commands::CheckForUpdates,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.main_window.update(cx, |_, window, cx| {
            window.activate_window();
            window.dispatch_action(Box::new(commands::CheckForUpdates), cx);
        });
    }

    /// ⌘, while Settings is already frontmost: nothing to open.
    fn reopen(&mut self, _: &commands::OpenPreferences, window: &mut Window, _: &mut Context<Self>) {
        window.activate_window();
    }
}

impl Focusable for SettingsWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Window fill behind the grouped rows: a step darker than the groups in light,
/// a step below them in dark, like System Settings.
fn window_bg() -> u32 {
    if is_dark_theme() { 0x1e1e1e } else { 0xf2f4f7 }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (settings, context) = self
            .app
            .upgrade()
            .map(|app| {
                let app = app.read(cx);
                (app.settings_model().clone(), app.preferences_context())
            })
            .unwrap_or_default();
        let theme = resolve_theme(&settings.theme_mode, window.appearance());
        with_theme(theme, || {
            div()
                .size_full()
                .flex()
                .flex_col()
                .bg(rgb(window_bg()))
                .text_color(rgb(text_primary()))
                .key_context("SettingsWindow")
                .track_focus(&self.focus_handle)
                .on_action(cx.listener(Self::close))
                .on_action(cx.listener(Self::quit))
                .on_action(cx.listener(Self::reopen))
                .on_action(cx.listener(Self::check_for_updates))
                .child(
                    div()
                        .h(px(TITLE_STRIP_PX))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_b_1()
                        .border_color(shell_border())
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(SETTINGS_WINDOW_TITLE),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.))
                        .pt(px(8.))
                        .flex()
                        .flex_col()
                        .child(PreferencesView.render(&settings, &context, &self.app)),
                )
        })
    }
}
