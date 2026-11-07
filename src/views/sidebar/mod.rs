mod preview_row;
mod recent;
mod team_group;

use preview_row::{PREVIEW_AVATAR_PX, PreviewRow, title_segments};
use team_group::TeamGroup;

use crate::{
    domain::{ids::SidebarSectionId, route::Route},
    models::{
        app_model::Connectivity,
        settings_model::{SidebarMode, SidebarStyle},
        sidebar_model::{SidebarAvatar, SidebarModel, SidebarRow, SidebarSection, team_avatar_key},
    },
    views::{
        PREVIEWS_SIDEBAR_WIDTH_PX, accent, accent_strong,
        avatar::Avatar,
        chevron_down_icon, chevron_right_icon, hash_icon, icon_asset, is_dark_theme,
        popup_menu::{PopupMenuEntry, popup_menu},
        sidebar_bg, sidebar_hover_bg, sidebar_selected_bg, sidebar_selected_fg,
        sidebar_selected_secondary, subtle_surface, success_soft, text_primary,
        text_secondary, tint,
        tooltip::shortcut_label,
        warning,
        truncated_line::{Segment, TruncatedLine},
    },
};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, AppContext, ClickEvent, Context, Corner, DragMoveEvent, FontWeight, Hsla,
    InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, UniformListScrollHandle, Window, anchored, deferred, div,
    point, px, rgb, svg,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
struct DraggedSection(SidebarSectionId);

struct DragPreview {
    title: String,
}

impl Render for DragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1p5()
            .rounded_md()
            .bg(subtle_surface())
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(text_secondary()))
            .child(self.title.clone())
    }
}

use crate::views::tooltip::TextTooltip as SidebarTooltip;

pub(super) const ROW_AVATAR_SIZE_PX: f32 = 20.;
pub(super) const ROW_TEXT_SIZE_PX: f32 = 13.;
const COMPACT_ROW_HEIGHT_PX: f32 = 30.;
const TOOLBAR_BUTTON_PX: f32 = 28.;
const TOOLBAR_ICON_PX: f32 = 16.;
/// Space above a section header, and above the first one in the list.
const SECTION_GAP_PX: f32 = 16.;
const FIRST_SECTION_GAP_PX: f32 = 8.;
const SECTION_HEADER_PX: f32 = 24.;

/// A menu that pops up from the sidebar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarMenu {
    /// Recent order or grouped by team, from the list's label.
    Order,
    /// Your profile and Settings, from your name in the footer.
    Identity,
}

/// How a row shows that it is the open conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowSelection {
    None,
    /// The one selected row: accent fill.
    Full,
    /// Another listing of the open conversation (its Unread shortcut): a thin
    /// accent bar only, so the sidebar never shows two selections.
    Marker,
}

impl RowSelection {
    pub fn of(row: &SidebarRow, current: &Route, owns_selection: bool) -> Self {
        match (&row.route == current, owns_selection) {
            (false, _) => Self::None,
            (true, true) => Self::Full,
            (true, false) => Self::Marker,
        }
    }

    pub fn is_full(self) -> bool {
        self == Self::Full
    }
}

/// Row style, with what the Previews style needs to know about its width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowLook {
    Compact,
    /// `compact_time`: the sidebar is narrower than Previews' preferred width,
    /// so times shorten ("10:30", "Thu") before names truncate.
    Previews { compact_time: bool },
}

impl RowLook {
    fn new(style: SidebarStyle, width_px: f32) -> Self {
        match style {
            SidebarStyle::Compact => Self::Compact,
            SidebarStyle::Previews => Self::Previews {
                compact_time: width_px < PREVIEWS_SIDEBAR_WIDTH_PX - 8.,
            },
        }
    }
}

/// Text and fill colors of one sidebar row, from its selection and hover state.
pub(super) struct RowColors {
    pub primary: Hsla,
    pub secondary: Hsla,
    pub background: Option<Hsla>,
    /// What the row sits on: rings around avatars and presence dots use it.
    pub surface: Hsla,
}

impl RowColors {
    /// A muted conversation reads in secondary text unless it is selected.
    pub fn new(selected: bool, hovered: bool, muted: bool) -> Self {
        if selected {
            let background: Hsla = rgb(sidebar_selected_bg()).into();
            return Self {
                primary: rgb(sidebar_selected_fg()).into(),
                secondary: sidebar_selected_secondary(),
                background: Some(background),
                surface: background,
            };
        }
        let background = hovered.then(|| rgb(sidebar_hover_bg()).into());
        Self {
            primary: rgb(if muted { text_secondary() } else { text_primary() }).into(),
            secondary: rgb(text_secondary()).into(),
            background,
            surface: background.unwrap_or_else(|| rgb(sidebar_bg()).into()),
        }
    }
}

/// Unread state of a row. Mentions earn an "@" badge, every unread message in a
/// direct conversation a count; other unread activity is a quiet dot. Muted
/// conversations keep only their mentions.
pub(super) struct RowActivity {
    unread: u32,
    badge: u32,
    mention: bool,
    muted: bool,
}

impl RowActivity {
    pub fn of(row: &SidebarRow) -> Self {
        let direct = matches!(row.route, Route::DirectMessage { .. });
        Self {
            unread: row.unread_count,
            badge: if direct {
                row.unread_count.max(row.mention_count)
            } else {
                row.mention_count
            },
            mention: !direct,
            muted: row.muted,
        }
    }

    pub fn is_unread(&self) -> bool {
        self.badge > 0 || (self.unread > 0 && !self.muted)
    }

    /// Trailing badge or dot. The selected row shows neither: it is being read.
    pub fn indicator(&self, selected: bool) -> Option<AnyElement> {
        if selected {
            return None;
        }
        if self.badge > 0 {
            return Some(
                div()
                    .flex_shrink_0()
                    .h(px(16.))
                    .min_w(px(16.))
                    .px(px(5.))
                    .rounded_full()
                    .bg(rgb(accent_strong()))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(11.))
                    .line_height(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff))
                    .child(badge_label(self.badge, self.mention))
                    .into_any_element(),
            );
        }
        (self.unread > 0 && !self.muted).then(|| {
            div()
                .flex_shrink_0()
                .mx(px(5.))
                .w(px(6.))
                .h(px(6.))
                .rounded_full()
                .bg(rgb(text_secondary()))
                .into_any_element()
        })
    }
}

/// Badge text: "@" for one mention, "@3" for more; a plain count otherwise.
fn badge_label(count: u32, mention: bool) -> String {
    let count = if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };
    match (mention, count.as_str()) {
        (true, "1") => "@".to_string(),
        (true, _) => format!("@{count}"),
        (false, _) => count,
    }
}

/// One line in the Compact style: a 20px mark, the name, and its badge.
/// Shared by Recent, Unread shortcuts and grouped rows.
pub(super) struct CompactRow<'a> {
    pub id: String,
    pub row: &'a SidebarRow,
    pub title: Vec<Segment>,
    pub leading: AnyElement,
    pub colors: &'a RowColors,
    pub selection: RowSelection,
}

impl CompactRow<'_> {
    pub fn render<H: SidebarHost>(self, cx: &mut Context<H>) -> AnyElement {
        let Self {
            id,
            row,
            title,
            leading,
            colors,
            selection,
        } = self;
        let activity = RowActivity::of(row);
        let hover_id = id.clone();
        let route = row.route.clone();
        div()
            .id(SharedString::from(id))
            .relative()
            .h(px(COMPACT_ROW_HEIGHT_PX))
            .w_full()
            .flex_shrink_0()
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(6.))
            .cursor_pointer()
            .when_some(colors.background, |row, background| row.bg(background))
            .when(selection == RowSelection::Marker, |row| {
                row.child(
                    div()
                        .absolute()
                        .left(px(0.))
                        .top(px((COMPACT_ROW_HEIGHT_PX - 16.) / 2.))
                        .w(px(2.))
                        .h(px(16.))
                        .rounded_full()
                        // Neutral behind other windows, like the selection fill.
                        .bg(rgb(if crate::views::window_is_active() {
                            accent()
                        } else {
                            text_secondary()
                        })),
                )
            })
            .tooltip(SidebarTooltip::builder(row.full_label()))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.sidebar_set_hovered_row(hovered.then(|| hover_id.clone()), cx);
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.sidebar_hide_hover_tooltip(cx);
                this.sidebar_navigate_to(route.clone(), window, cx);
            }))
            .child(
                div()
                    .w(px(ROW_AVATAR_SIZE_PX))
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .child(leading),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_size(px(ROW_TEXT_SIZE_PX))
                    .child(TruncatedLine::new(title)),
            )
            // The open conversation is being read, wherever it is listed.
            .when_some(activity.indicator(selection != RowSelection::None), |row, indicator| {
                row.child(indicator)
            })
            .into_any_element()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolbarAction {
    Back,
    Forward,
    Search,
    NewChat,
    Updates,
}

/// What the toolbar above the list reflects about the window: whether history
/// can go back or forward, whether a utility page (Search) is showing, and
/// which sidebar menu is open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SidebarChrome {
    pub update_available: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub search_open: bool,
    pub menu: Option<SidebarMenu>,
}

struct ToolbarButton {
    id: &'static str,
    tooltip: String,
    shortcut: Option<String>,
    icon: &'static str,
    action: ToolbarAction,
    /// The page it opens is showing.
    on: bool,
    enabled: bool,
}

impl ToolbarButton {
    fn render<H: SidebarHost>(self, width: f32, height: f32, cx: &mut Context<H>) -> AnyElement {
        let Self {
            id,
            tooltip,
            shortcut,
            icon,
            action,
            on,
            enabled,
        } = self;
        // Behind other windows the toggled fill fades, like native toolbar items.
        let on_fill = tint(
            text_primary(),
            if crate::views::window_is_active() { 0.11 } else { 0.06 },
        );
        let icon_color = match (enabled, on) {
            (false, _) => tint(text_secondary(), 0.45),
            (true, true) => rgb(text_primary()).into(),
            (true, false) => rgb(text_secondary()).into(),
        };
        div()
            .id(id)
            .w(px(width))
            .h(px(height))
            .flex_shrink_0()
            .rounded(px(6.))
            .flex()
            .items_center()
            .justify_center()
            .when(on, |button| button.bg(on_fill))
            .when(enabled && !on, |button| {
                button
                    .cursor_pointer()
                    .hover(|style| style.bg(tint(text_primary(), 0.07)))
            })
            .when(enabled && on, |button| button.cursor_pointer())
            .map(|button| match shortcut {
                Some(shortcut) => button.tooltip(SidebarTooltip::with_shortcut(tooltip, shortcut)),
                None => button.tooltip(SidebarTooltip::single_line(tooltip)),
            })
            .when(enabled, |button| {
                button.on_click(cx.listener(move |this, _, window, cx| {
                    this.sidebar_hide_hover_tooltip(cx);
                    match action {
                        ToolbarAction::Back => this.sidebar_navigate_history(false, window, cx),
                        ToolbarAction::Forward => this.sidebar_navigate_history(true, window, cx),
                        ToolbarAction::Search => this.sidebar_toggle_quick_switcher(cx),
                        ToolbarAction::NewChat => this.sidebar_open_new_chat(window, cx),
                        ToolbarAction::Updates => this.sidebar_show_updates(window, cx),
                    }
                }))
            })
            .child(
                svg()
                    .path(icon)
                    .size(px(TOOLBAR_ICON_PX))
                    .flex_shrink_0()
                    .text_color(icon_color),
            )
            .into_any_element()
    }
}

#[derive(Default)]
pub struct Sidebar;

pub trait SidebarHost: Sized + 'static {
    fn sidebar_show_updates(&mut self, window: &mut Window, cx: &mut Context<Self>);
    fn sidebar_set_mode(&mut self, mode: SidebarMode, cx: &mut Context<Self>);
    fn sidebar_toggle_quick_switcher(&mut self, cx: &mut Context<Self>);
    fn sidebar_open_new_chat(&mut self, window: &mut Window, cx: &mut Context<Self>);
    fn sidebar_open_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>);
    /// Opens the signed-in user's own profile.
    fn sidebar_open_own_profile(&mut self, window: &mut Window, cx: &mut Context<Self>);
    /// Opens `menu`, or closes it when it is already open.
    fn sidebar_toggle_menu(&mut self, menu: SidebarMenu, window: &mut Window, cx: &mut Context<Self>);
    /// Closes any open sidebar menu; `dismissed` when a press outside closed it.
    fn sidebar_close_menu(&mut self, dismissed: bool, cx: &mut Context<Self>);
    fn sidebar_show_hover_tooltip(
        &mut self,
        text: String,
        anchor_x: f32,
        anchor_y: f32,
        cx: &mut Context<Self>,
    );
    fn sidebar_hide_hover_tooltip(&mut self, cx: &mut Context<Self>);
    fn sidebar_toggle_section(&mut self, section_id: SidebarSectionId, cx: &mut Context<Self>);
    fn sidebar_reorder_section(
        &mut self,
        dragged_id: SidebarSectionId,
        target_id: SidebarSectionId,
        cx: &mut Context<Self>,
    );
    fn sidebar_navigate_to(&mut self, route: Route, window: &mut Window, cx: &mut Context<Self>);
    fn sidebar_navigate_history(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>);
    fn sidebar_set_hovered_row(&mut self, label: Option<String>, cx: &mut Context<Self>);
    fn sidebar_hovered_row(&self) -> Option<&str>;
}

/// What every grouped row needs besides the row itself.
pub(super) struct RowContext<'a> {
    pub avatars: &'a HashMap<String, SidebarAvatar>,
    pub current_route: &'a Route,
    pub hovered_row: Option<&'a str>,
    pub look: RowLook,
    /// The section whose listing of the open conversation is selected.
    pub selected_section: Option<&'a SidebarSectionId>,
    pub live: &'a HashMap<String, crate::models::sidebar_preview::PreviewLive>,
}

impl Sidebar {
    pub fn render<H: SidebarHost>(
        &self,
        sidebar: &SidebarModel,
        connectivity: &Connectivity,
        current_user_display_name: &str,
        current_user_avatar_asset: Option<&str>,
        conversation_avatars: &HashMap<String, SidebarAvatar>,
        current_route: &Route,
        hovered_row: Option<&str>,
        scroll_handle: &ScrollHandle,
        recent_scroll: &UniformListScrollHandle,
        chrome: SidebarChrome,
        cx: &mut Context<H>,
    ) -> AnyElement {
        let drag_scroll_handle = scroll_handle.clone();
        let look = RowLook::new(sidebar.style, sidebar.width_px);
        let selected_section = sidebar.selection_section(current_route);
        let context = RowContext {
            avatars: conversation_avatars,
            current_route,
            hovered_row,
            look,
            selected_section,
            live: &sidebar.live_previews,
        };
        let order_menu_open = chrome.menu == Some(SidebarMenu::Order);

        div()
            .w(px(sidebar.width_px))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .px_2()
            .bg(crate::views::sidebar_surface())
            .overflow_hidden()
            .text_color(rgb(text_primary()))
            .child(Self::render_title_bar_row(sidebar.width_px, chrome, cx))
            .when(sidebar.mode == SidebarMode::Recent, |root| {
                root.child(
                    div()
                        .pt(px(FIRST_SECTION_GAP_PX))
                        .child(Self::order_header("Recent", sidebar.mode, order_menu_open, cx)),
                )
                .child(recent::render(
                    sidebar,
                    conversation_avatars,
                    current_route,
                    hovered_row,
                    recent_scroll,
                    cx,
                ))
            })
            .when(sidebar.mode == SidebarMode::Grouped, |root| {
                let mut previous_section_is_team = false;
                let mut first = true;
                let mut elements = Vec::new();
                for section in &sidebar.sections {
                    let is_team = section.id.0.starts_with("group_");
                    let gap = if first {
                        FIRST_SECTION_GAP_PX
                    } else {
                        SECTION_GAP_PX
                    };
                    if is_team && !previous_section_is_team {
                        // The team list's label carries the order menu.
                        elements.push(
                            div()
                                .pt(px(gap))
                                .child(Self::order_header(
                                    "Teams",
                                    sidebar.mode,
                                    order_menu_open,
                                    cx,
                                ))
                                .into_any_element(),
                        );
                    }
                    let top = if is_team {
                        if previous_section_is_team { 4. } else { 0. }
                    } else {
                        gap
                    };
                    previous_section_is_team = is_team;
                    first = false;
                    elements.push(Self::render_section(section, &context, top, cx));
                }
                root.child(
                    div()
                        .flex_1()
                        .id("sidebar-sections-scroll")
                        .overflow_y_scroll()
                        .scrollbar_width(px(8.))
                        .track_scroll(scroll_handle)
                        .on_drag_move::<DraggedSection>(move |event, _, _| {
                            Self::auto_scroll_during_drag(event, &drag_scroll_handle);
                        })
                        .pb_2()
                        .flex()
                        .flex_col()
                        .children(elements),
                )
            })
            .child(Self::render_profile_footer(
                connectivity,
                current_user_display_name,
                current_user_avatar_asset,
                chrome.menu == Some(SidebarMenu::Identity),
                cx,
            ))
            .into_any_element()
    }

    fn render_title_bar_row<H: SidebarHost>(
        width_px: f32,
        chrome: SidebarChrome,
        cx: &mut Context<H>,
    ) -> AnyElement {
        let compact = width_px < if chrome.update_available { 260. } else { 208. };
        let button_size = if compact { 24. } else { TOOLBAR_BUTTON_PX };
        let history = [
            ToolbarButton {
                id: "sidebar-back",
                tooltip: "Back".to_string(),
                shortcut: Some(shortcut_label("⌘[")),
                icon: "assets/icons/chevron-left.svg",
                action: ToolbarAction::Back,
                on: false,
                enabled: chrome.can_go_back,
            },
            ToolbarButton {
                id: "sidebar-forward",
                tooltip: "Forward".to_string(),
                shortcut: Some(shortcut_label("⌘]")),
                icon: "assets/icons/chevron-right.svg",
                action: ToolbarAction::Forward,
                on: false,
                enabled: chrome.can_go_forward,
            },
        ];
        let tools = [
            ToolbarButton {
                id: "sidebar-search-trigger",
                tooltip: "Jump to conversation".to_string(),
                shortcut: Some(shortcut_label("⌘K")),
                icon: "assets/icons/search.svg",
                action: ToolbarAction::Search,
                on: chrome.search_open,
                enabled: true,
            },
            ToolbarButton {
                id: "sidebar-new-chat-trigger",
                tooltip: "New message".to_string(),
                shortcut: Some(shortcut_label("⌘N")),
                icon: "assets/icons/plus.svg",
                action: ToolbarAction::NewChat,
                on: false,
                enabled: true,
            },
        ];
        let mut tools = tools.into_iter().collect::<Vec<_>>();
        if chrome.update_available {
            tools.push(ToolbarButton {
                id: "sidebar-update",
                tooltip: "Software update available".into(),
                shortcut: None,
                icon: "assets/icons/arrow-down-circle.svg",
                action: ToolbarAction::Updates,
                on: true,
                enabled: true,
            });
        }
        let group = |buttons: Vec<AnyElement>| {
            div()
                .flex()
                .items_center()
                .gap(px(if compact { 0. } else { 2. }))
                .children(buttons)
        };
        div()
            .when(cfg!(target_os = "macos"), |row| row.h(px(44.)))
            .flex_shrink_0()
            .flex()
            .items_end()
            .justify_end()
            .gap(px(if compact { 4. } else { 8. }))
            // Back and forward are a narrower pair, like Finder's toolbar.
            .child(group(
                history
                    .into_iter()
                    .map(|button| button.render(button_size - 4., button_size, cx))
                    .collect(),
            ))
            .child(group(
                tools
                    .into_iter()
                    .map(|button| button.render(button_size, button_size, cx))
                    .collect(),
            ))
            .into_any_element()
    }

    /// The list's label ("Recent", "Teams") as a pop-up button that chooses the
    /// sidebar order, like a Finder pop-up: label, up-down chevrons, and the menu
    /// below it while open.
    fn order_header<H: SidebarHost>(
        title: &'static str,
        mode: SidebarMode,
        open: bool,
        cx: &mut Context<H>,
    ) -> AnyElement {
        let entries = [
            (SidebarMode::Recent, "Sort by Recent", "⌥⌘1"),
            (SidebarMode::Grouped, "Group by Team", "⌥⌘2"),
        ]
        .into_iter()
        .map(|(target, label, keys)| {
            PopupMenuEntry::choice(
                label,
                mode == target,
                cx.listener(move |this: &mut H, _: &ClickEvent, _, cx| {
                    this.sidebar_close_menu(false, cx);
                    this.sidebar_set_mode(target, cx);
                }),
            )
            .shortcut(shortcut_label(keys))
        })
        .collect();
        let label_color = if open { text_primary() } else { text_secondary() };
        div()
            .h(px(SECTION_HEADER_PX))
            .px_1()
            .flex()
            .items_center()
            .child(
                div()
                    .relative()
                    .child(
                        div()
                            .id("sidebar-order-trigger")
                            .h(px(20.))
                            .px_1()
                            .flex()
                            .items_center()
                            .gap(px(3.))
                            .rounded(px(4.))
                            .cursor_pointer()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(label_color))
                            .when(open, |trigger| trigger.bg(tint(text_primary(), 0.08)))
                            .hover(|style| {
                                style
                                    .bg(tint(text_primary(), 0.06))
                                    .text_color(rgb(text_primary()))
                            })
                            .tooltip(SidebarTooltip::single_line("Sidebar order".to_string()))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.sidebar_hide_hover_tooltip(cx);
                                this.sidebar_toggle_menu(SidebarMenu::Order, window, cx);
                            }))
                            .child(title)
                            .child(
                                svg()
                                    .path("assets/icons/chevrons-up-down.svg")
                                    .size(px(10.))
                                    .text_color(rgb(label_color)),
                            ),
                    )
                    .when(open, |trigger| {
                        trigger.child(deferred(
                            anchored()
                                .anchor(Corner::TopLeft)
                                .offset(point(px(0.), px(4.)))
                                .child(popup_menu(
                                    "sidebar-order-menu",
                                    200.,
                                    entries,
                                    cx.listener(|this, _, _, cx| this.sidebar_close_menu(true, cx)),
                                )),
                        ))
                    }),
            )
            .into_any_element()
    }

    fn render_profile_footer<H: SidebarHost>(
        connectivity: &Connectivity,
        current_user_display_name: &str,
        current_user_avatar_asset: Option<&str>,
        menu_open: bool,
        cx: &mut Context<H>,
    ) -> AnyElement {
        // Connected needs no mark: a green dot beside your own name would read as
        // presence. Offline is neutral grey (red means Do not disturb or an
        // error); connecting is amber.
        let connection_status = match connectivity {
            Connectivity::Online => None,
            Connectivity::Reconnecting => {
                Some(("Connecting…", warning(), "Connecting to Keybase…"))
            }
            Connectivity::Offline => Some((
                "Offline",
                text_secondary(),
                "Keybase can't be reached · retrying automatically",
            )),
        };
        let entries = vec![
            PopupMenuEntry::item(
                "View Profile",
                cx.listener(|this: &mut H, _: &ClickEvent, window, cx| {
                    this.sidebar_close_menu(false, cx);
                    this.sidebar_open_own_profile(window, cx);
                }),
            ),
            PopupMenuEntry::Separator,
            PopupMenuEntry::item(
                "Settings…",
                cx.listener(|this: &mut H, _: &ClickEvent, window, cx| {
                    this.sidebar_close_menu(false, cx);
                    this.sidebar_open_preferences(window, cx);
                }),
            )
            .shortcut(shortcut_label("⌘,")),
        ];
        div()
            .flex_shrink_0()
            .py_2()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .relative()
                    .min_w(px(0.))
                    // Before the button, so the menu's anchor is the button's top edge.
                    .when(menu_open, |identity| {
                        identity.child(deferred(
                            anchored()
                                .anchor(Corner::BottomLeft)
                                .offset(point(px(0.), px(-4.)))
                                .child(popup_menu(
                                    "sidebar-identity-menu",
                                    200.,
                                    entries,
                                    cx.listener(|this, _, _, cx| this.sidebar_close_menu(true, cx)),
                                )),
                        ))
                    })
                    .child(
                        div()
                            .id(SharedString::from("sidebar-profile"))
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.sidebar_hide_hover_tooltip(cx);
                                this.sidebar_toggle_menu(SidebarMenu::Identity, window, cx);
                            }))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap_2()
                            .rounded_md()
                            .px_1()
                            .py_1()
                            .when(menu_open, |identity| identity.bg(subtle_surface()))
                            .hover(|s| s.bg(subtle_surface()))
                            .child(Avatar::render(
                                current_user_display_name,
                                current_user_avatar_asset,
                                24.,
                                success_soft(),
                                if is_dark_theme() {
                                    0xffffff
                                } else {
                                    crate::views::affinity_positive()
                                },
                            ))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(current_user_display_name.to_string()),
                                    )
                                    .when_some(connection_status, |name, (status, tint, tooltip)| {
                                        name.child(
                                            div()
                                                .id("sidebar-connection-status")
                                                .flex()
                                                .items_center()
                                                .gap(px(4.))
                                                .text_size(px(11.))
                                                .line_height(px(14.))
                                                .text_color(rgb(text_secondary()))
                                                .tooltip(SidebarTooltip::single_line(
                                                    tooltip.to_string(),
                                                ))
                                                .child(icon_asset(
                                                    "assets/icons/cloud-off.svg",
                                                    12.,
                                                    tint,
                                                ))
                                                .child(status),
                                        )
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn auto_scroll_during_drag(
        event: &DragMoveEvent<DraggedSection>,
        scroll_handle: &ScrollHandle,
    ) {
        let edge_zone = px(40.);
        let max_speed = px(8.);

        let mouse_y = event.event.position.y;
        let top = event.bounds.top();
        let bottom = event.bounds.bottom();

        let delta = if mouse_y < top + edge_zone {
            let proximity = (top + edge_zone - mouse_y) / edge_zone;
            max_speed * proximity.clamp(0.0, 1.0)
        } else if mouse_y > bottom - edge_zone {
            let proximity = (mouse_y - (bottom - edge_zone)) / edge_zone;
            -(max_speed * proximity.clamp(0.0, 1.0))
        } else {
            return;
        };

        let mut offset = scroll_handle.offset();
        let max = scroll_handle.max_offset();
        offset.y = (offset.y + delta).clamp(-max.height, px(0.));
        scroll_handle.set_offset(offset);
    }

    /// A section of the grouped list, `top` px below what precedes it.
    fn render_section<H: SidebarHost>(
        section: &SidebarSection,
        context: &RowContext<'_>,
        top: f32,
        cx: &mut Context<H>,
    ) -> AnyElement {
        if section.id.0.starts_with("group_")
            && let Some(team) = section.rows.iter().find_map(|row| row.team.as_ref())
        {
            return div()
                .pt(px(top))
                .child(
                    TeamGroup {
                        team,
                        section_id: &section.id,
                        rows: &section.rows,
                        collapsed: section.collapsed,
                        context,
                    }
                    .render(cx),
                )
                .into_any_element();
        }
        let is_unread_section = section.is_unread();
        let rows: Vec<AnyElement> = if is_unread_section {
            Self::render_unread_rows(section, context, cx)
        } else if section.collapsed {
            Vec::new()
        } else {
            section
                .rows
                .iter()
                .map(|row| Self::render_row(&section.id, row, context, cx))
                .collect()
        };
        div()
            .pt(px(top))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(Self::section_header(section, cx))
            .children(rows)
            .into_any_element()
    }

    /// A section's label row. Collapsible sections put their disclosure
    /// chevron at the right edge, like Finder: always shown while collapsed,
    /// revealed on hover while expanded.
    fn section_header<H: SidebarHost>(section: &SidebarSection, cx: &mut Context<H>) -> AnyElement {
        let collapsible = !section.is_unread();
        let group = SharedString::from(format!("sidebar-section-{}", section.id.0));
        let section_id = section.id.clone();
        let section_id_for_drop = section.id.clone();
        let drag_title = section.title.clone();
        div()
            .id(group.clone())
            .group(group.clone())
            .h(px(SECTION_HEADER_PX))
            .flex()
            .items_center()
            .justify_between()
            .gap_1()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(text_secondary()))
            .px_2()
            .rounded_md()
            .when(collapsible, |header| {
                header
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.sidebar_toggle_section(section_id.clone(), cx);
                    }))
                    .on_drag(DraggedSection(section.id.clone()), {
                        let title = drag_title.clone();
                        move |_, _, _, cx| {
                            cx.new(|_| DragPreview {
                                title: title.clone(),
                            })
                        }
                    })
                    .drag_over::<DraggedSection>(|style, _, _, _| {
                        style.border_t_2().border_color(rgb(accent()))
                    })
                    .on_drop(cx.listener(move |this, dragged: &DraggedSection, _, cx| {
                        this.sidebar_reorder_section(
                            dragged.0.clone(),
                            section_id_for_drop.clone(),
                            cx,
                        );
                    }))
            })
            .child(section.title.clone())
            .when(collapsible, |header| {
                header.child(disclosure(section.collapsed, group))
            })
            .into_any_element()
    }

    /// Unread shortcuts: one compact line each, whatever the row style, with the
    /// same mark and inset as Recent. The full previews stay in the sections below.
    fn render_unread_rows<H: SidebarHost>(
        section: &SidebarSection,
        context: &RowContext<'_>,
        cx: &mut Context<H>,
    ) -> Vec<AnyElement> {
        let mut seen = HashSet::new();
        let ambiguous = section
            .rows
            .iter()
            .filter(|row| row.team.is_some())
            .filter(|row| !seen.insert(row.label.to_lowercase()))
            .map(|row| row.label.to_lowercase())
            .collect::<HashSet<_>>();
        section
            .rows
            .iter()
            .map(|row| {
                let selection = RowSelection::of(
                    row,
                    context.current_route,
                    context.selected_section == Some(&section.id),
                );
                let route_label = row.route.label();
                let id = format!("sidebar-{}-{route_label}", section.id.0);
                let colors = RowColors::new(
                    selection.is_full(),
                    context.hovered_row == Some(id.as_str()),
                    row.muted,
                );
                let emphasized = selection.is_full() || RowActivity::of(row).is_unread();
                let (label, suffix, leading) = match &row.team {
                    Some(team) => (
                        format!("#{}", row.label),
                        ambiguous
                            .contains(&row.label.to_lowercase())
                            .then(|| team.display_name.clone()),
                        Avatar::render_team(
                            &team.display_name,
                            team_asset(context.avatars, &team.id),
                            ROW_AVATAR_SIZE_PX,
                        ),
                    ),
                    None => (
                        row.label.clone(),
                        None,
                        Self::leading_element(
                            row,
                            &route_label,
                            context.avatars,
                            &colors,
                            ROW_AVATAR_SIZE_PX,
                        ),
                    ),
                };
                CompactRow {
                    id,
                    row,
                    title: title_segments(label, suffix, &colors, emphasized),
                    leading,
                    colors: &colors,
                    selection,
                }
                .render(cx)
            })
            .collect()
    }

    /// A row in a team or the Conversations section, in the chosen style.
    fn render_row<H: SidebarHost>(
        section_id: &SidebarSectionId,
        row: &SidebarRow,
        context: &RowContext<'_>,
        cx: &mut Context<H>,
    ) -> AnyElement {
        let selection = RowSelection::of(
            row,
            context.current_route,
            context.selected_section == Some(section_id),
        );
        let activity = RowActivity::of(row);
        let route_label = row.route.label();
        // Rows can appear in several sections (Unread and their own); keep ids unique.
        let row_id = format!("sidebar-{}-{route_label}", section_id.0);
        let colors = RowColors::new(
            selection.is_full(),
            context.hovered_row == Some(row_id.as_str()),
            row.muted,
        );
        let emphasized = selection.is_full() || activity.is_unread();
        let channel_under_team = row.team.is_some() && matches!(row.route, Route::Channel { .. });
        match context.look {
            RowLook::Previews { compact_time } => {
                // Under its team's header a channel needs no team badge.
                let (lead, circular) = if channel_under_team {
                    (Avatar::render_channel(None, PREVIEW_AVATAR_PX, colors.surface), false)
                } else {
                    (
                        Self::leading_element(
                            row,
                            &route_label,
                            context.avatars,
                            &colors,
                            PREVIEW_AVATAR_PX,
                        ),
                        matches!(row.route, Route::DirectMessage { .. }),
                    )
                };
                PreviewRow {
                    row,
                    id: row_id,
                    title: title_segments(row.label.clone(), None, &colors, emphasized),
                    lead,
                    lead_circular: circular,
                    colors: &colors,
                    selected: selection.is_full(),
                    live: row
                        .route
                        .conversation_key()
                        .and_then(|key| context.live.get(key)),
                    compact_time,
                }
                .render(cx)
            }
            RowLook::Compact => CompactRow {
                title: title_segments(row.label.clone(), None, &colors, emphasized),
                leading: Self::leading_element(
                    row,
                    &route_label,
                    context.avatars,
                    &colors,
                    ROW_AVATAR_SIZE_PX,
                ),
                id: row_id,
                row,
                colors: &colors,
                selection,
            }
            .render(cx),
        }
    }

    /// Avatar or glyph in front of a row label: the team mark for a simple team,
    /// `#` for a channel listed under its team, and faces with presence for DMs.
    fn leading_element(
        row: &SidebarRow,
        route_label: &str,
        conversation_avatars: &HashMap<String, SidebarAvatar>,
        colors: &RowColors,
        size: f32,
    ) -> AnyElement {
        match &row.route {
            Route::Channel { .. } if row.team.is_none() => {
                let asset = match conversation_avatars.get(route_label) {
                    Some(SidebarAvatar::Team(asset)) => Some(asset.as_str()),
                    _ => None,
                };
                Avatar::render_team(&row.label, asset, size)
            }
            Route::Channel { .. } => div()
                .w(px(size))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .child(hash_icon(hsla_to_rgb(colors.secondary)))
                .into_any_element(),
            Route::DirectMessage { .. } => Avatar::render_sidebar_participants_on(
                conversation_avatars
                    .get(route_label)
                    .map(SidebarAvatar::participants)
                    .unwrap_or_default(),
                &row.label,
                size,
                colors.surface,
            ),
            _ => div()
                .w(px(size))
                .flex()
                .items_center()
                .justify_center()
                .child(chevron_right_icon(text_secondary()))
                .into_any_element(),
        }
    }
}

/// A header's disclosure chevron, at its right edge. Shown while collapsed;
/// revealed by hovering `group` while expanded.
pub(super) fn disclosure(collapsed: bool, group: SharedString) -> AnyElement {
    div()
        .flex_shrink_0()
        .when(!collapsed, |chevron| {
            chevron
                .opacity(0.)
                .group_hover(group, |style| style.opacity(1.))
        })
        .child(if collapsed {
            chevron_right_icon(text_secondary())
        } else {
            chevron_down_icon(text_secondary())
        })
        .into_any_element()
}

/// The image of a team, if it has one.
pub(super) fn team_asset<'a>(
    avatars: &'a HashMap<String, SidebarAvatar>,
    team_id: &str,
) -> Option<&'a str> {
    match avatars.get(&team_avatar_key(team_id)) {
        Some(SidebarAvatar::Team(asset)) => Some(asset.as_str()),
        _ => None,
    }
}

/// Icon helpers take packed RGB; sidebar row colors are resolved as `Hsla`.
fn hsla_to_rgb(color: Hsla) -> u32 {
    let rgba = color.to_rgb();
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u32;
    (channel(rgba.r) << 16) | (channel(rgba.g) << 8) | channel(rgba.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mention_badges_read_differently_from_unread_counts() {
        assert_eq!(badge_label(1, true), "@");
        assert_eq!(badge_label(3, true), "@3");
        assert_eq!(badge_label(3, false), "3");
        assert_eq!(badge_label(120, false), "99+");
        assert_eq!(badge_label(120, true), "@99+");
    }

    #[test]
    fn only_the_owning_section_draws_the_full_selection() {
        let route = Route::Preferences;
        let row = SidebarRow {
            label: "general".into(),
            team: None,
            unread_count: 0,
            mention_count: 0,
            route: route.clone(),
            preview: None,
            muted: false,
        };
        assert_eq!(RowSelection::of(&row, &route, true), RowSelection::Full);
        assert_eq!(RowSelection::of(&row, &route, false), RowSelection::Marker);
        let other = Route::Search {
            workspace_id: crate::domain::ids::WorkspaceId::new("ws"),
            query: String::new(),
        };
        assert_eq!(RowSelection::of(&row, &other, true), RowSelection::None);
    }

    #[test]
    fn muted_rows_keep_mentions_but_not_plain_unread() {
        let mut row = SidebarRow {
            label: "random".into(),
            team: None,
            unread_count: 4,
            mention_count: 0,
            route: Route::Channel {
                workspace_id: crate::domain::ids::WorkspaceId::new("ws"),
                channel_id: crate::domain::ids::ChannelId::new("c"),
            },
            preview: None,
            muted: true,
        };
        assert!(!RowActivity::of(&row).is_unread());
        row.mention_count = 1;
        assert!(RowActivity::of(&row).is_unread());
        row.muted = false;
        row.mention_count = 0;
        assert!(RowActivity::of(&row).is_unread());
    }
}
