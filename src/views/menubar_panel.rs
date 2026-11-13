//! The menu bar icon's panel, like Keybase's menu bar widget: recent
//! conversations, unread first, drawn as the sidebar's Previews rows. Picking
//! one brings zBase forward on it; Escape or a press anywhere else closes it.
use crate::domain::route::Route;
use crate::views::typography;
use crate::{
    app::{theme::ThemeVariant, window},
    models::{
        recent_sidebar::RecentSidebarRow, settings_model::TextSize, sidebar_model::SidebarAvatar,
        sidebar_preview::PreviewLive,
    },
    services::status_item,
    views::{
        app_window::AppWindow, shell_border, sidebar, sidebar::SidebarRowHost, sidebar_surface,
        subtle_surface, text_primary, text_secondary, with_theme, with_window_active,
    },
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Bounds, Context, DisplayId, FocusHandle, Focusable, FontWeight, Global,
    InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Pixels, Render,
    StatefulInteractiveElement, Styled, Subscription, WeakEntity, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, div, point,
    px, rgb, size,
};
use std::collections::HashMap;

/// How many conversations the panel lists.
pub const ROWS: usize = 8;
const WIDTH_PX: f32 = 340.;
const PADDING_PX: f32 = 6.;
const ROW_GAP_PX: f32 = 2.;
const FOOTER_PX: f32 = 32.;
const EMPTY_PX: f32 = 44.;
/// Between the menu bar and the panel's top edge.
const BELOW_ICON_PX: f32 = 4.;
/// Least space kept between the panel and the display's edges.
const SCREEN_MARGIN_PX: f32 = 8.;

/// What the panel shows, taken from the main window.
pub struct MenubarPanelState {
    pub rows: Vec<RecentSidebarRow>,
    pub avatars: HashMap<String, SidebarAvatar>,
    pub live: HashMap<String, PreviewLive>,
    pub theme: ThemeVariant,
    pub text_size: TextSize,
}

pub struct MenubarPanel {
    app: WeakEntity<AppWindow>,
    hovered_row: Option<String>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

struct OpenPanel(WindowHandle<MenubarPanel>);

impl Global for OpenPanel {}

/// Opens the panel under the menu bar icon at `icon` on `display`, or closes
/// it when it is open.
pub fn toggle(display: DisplayId, icon: Bounds<Pixels>, cx: &mut App) {
    if close(cx) {
        return;
    }
    let Some(app) = window::main_window_view(cx).and_then(|app| app.upgrade()) else {
        // Nothing to list without the main window; bring zBase back instead.
        window::show_main_window(cx);
        return;
    };
    let state = app.read(cx).menubar_panel_state();
    let height = typography::with_sidebar_size(state.text_size, || panel_height(state.rows.len()));
    let display_width = cx
        .find_display(display)
        .map_or(px(WIDTH_PX), |display| display.bounds().size.width);
    let x = (icon.center().x - px(WIDTH_PX / 2.))
        .min(display_width - px(WIDTH_PX + SCREEN_MARGIN_PX))
        .max(px(SCREEN_MARGIN_PX));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            point(x, icon.bottom() + px(BELOW_ICON_PX)),
            size(px(WIDTH_PX), px(height)),
        ))),
        titlebar: None,
        focus: true,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        display_id: Some(display),
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    };
    let app = app.downgrade();
    let Ok(handle) = cx.open_window(options, |window, cx| {
        let view = cx.new(|cx| {
            let subscriptions = app
                .upgrade()
                .map(|app| cx.observe(&app, |_, _, cx| cx.notify()))
                .into_iter()
                .collect();
            MenubarPanel {
                app,
                hovered_row: None,
                focus_handle: cx.focus_handle(),
                _subscriptions: subscriptions,
            }
        });
        window.focus(&view.focus_handle(cx), cx);
        view
    }) else {
        return;
    };
    cx.set_global(OpenPanel(handle));
    status_item::watch_outside_clicks(true);
}

/// Closes the panel; `false` when it was not open.
pub fn close(cx: &mut App) -> bool {
    if !cx.has_global::<OpenPanel>() {
        return false;
    }
    let OpenPanel(handle) = cx.remove_global::<OpenPanel>();
    status_item::watch_outside_clicks(false);
    let _ = handle.update(cx, |_, window, _| window.remove_window());
    true
}

/// Rows, the gaps between them, the footer and the padding around them.
fn panel_height(rows: usize) -> f32 {
    let list = if rows == 0 {
        EMPTY_PX
    } else {
        rows as f32 * sidebar::preview_row_height_px() + (rows - 1) as f32 * ROW_GAP_PX
    };
    PADDING_PX * 2. + list + ROW_GAP_PX + FOOTER_PX
}

impl Focusable for MenubarPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl SidebarRowHost for MenubarPanel {
    fn sidebar_hide_hover_tooltip(&mut self, _: &mut Context<Self>) {}

    fn sidebar_navigate_to(&mut self, route: Route, _: &mut Window, cx: &mut Context<Self>) {
        cx.defer(move |cx| {
            close(cx);
            window::open_route(route, cx);
        });
    }

    fn sidebar_open_conversation_team(&mut self, route: Route, cx: &mut Context<Self>) {
        cx.defer(move |cx| {
            close(cx);
            window::open_conversation_team(route, cx);
        });
    }

    fn sidebar_set_row_hovered(&mut self, row_id: String, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            if self.hovered_row.as_ref() != Some(&row_id) {
                self.hovered_row = Some(row_id);
                cx.notify();
            }
        } else if self.hovered_row.as_ref() == Some(&row_id) {
            self.hovered_row = None;
            cx.notify();
        }
    }
}

impl Render for MenubarPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(state) = self
            .app
            .upgrade()
            .map(|app| app.read(cx).menubar_panel_state())
        else {
            return div().into_any_element();
        };
        window.set_rem_size(px(16. * state.text_size.scale()));
        typography::with_sidebar_size(state.text_size, || {
            with_theme(state.theme, || {
                with_window_active(true, || {
                    let rows = sidebar::preview_rows(
                        &state.rows,
                        &state.avatars,
                        &state.live,
                        self.hovered_row.as_deref(),
                        cx,
                    );
                    div()
                        .id("menubar-panel")
                        .track_focus(&self.focus_handle)
                        .on_key_down(cx.listener(|_, event: &KeyDownEvent, _, cx| {
                            if event.keystroke.key == "escape" {
                                cx.defer(|cx| {
                                    close(cx);
                                });
                            }
                        }))
                        .size_full()
                        .p(px(PADDING_PX))
                        .flex()
                        .flex_col()
                        .gap(px(ROW_GAP_PX))
                        .rounded(px(10.))
                        .border_1()
                        .border_color(shell_border())
                        .bg(sidebar_surface())
                        .text_color(rgb(text_primary()))
                        .when(rows.is_empty(), |panel| {
                            panel.child(
                                div()
                                    .h(px(EMPTY_PX))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(typography::caption())
                                    .text_color(rgb(text_secondary()))
                                    .child("No conversations yet"),
                            )
                        })
                        .children(rows)
                        .child(
                            div()
                                .id("menubar-panel-open")
                                .h(px(FOOTER_PX))
                                .flex_shrink_0()
                                .px(px(8.))
                                .flex()
                                .items_center()
                                .rounded(px(8.))
                                .border_t_1()
                                .border_color(shell_border())
                                .text_size(typography::label())
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_pointer()
                                .hover(|row| row.bg(subtle_surface()))
                                .on_click(|_, _, cx| {
                                    cx.defer(|cx| {
                                        close(cx);
                                        window::show_main_window(cx);
                                    });
                                })
                                .child("Open zBase"),
                        )
                        .into_any_element()
                })
            })
        })
    }
}
