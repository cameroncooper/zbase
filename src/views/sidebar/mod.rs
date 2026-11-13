use crate::views::typography;
mod grouped;
mod preview_row;
mod recent;
mod team_group;

use preview_row::{PREVIEW_AVATAR_PX, PREVIEW_ROW_HEIGHT_PX, PreviewRow, title_segments};

use crate::{
    domain::{ids::SidebarSectionId, route::Route},
    models::{
        app_model::Connectivity,
        settings_model::{SidebarMode, SidebarStyle},
        sidebar_model::{SidebarAvatar, SidebarModel, SidebarRow, team_avatar_key},
    },
    views::{
        PREVIEWS_SIDEBAR_WIDTH_PX, accent_strong,
        avatar::Avatar,
        chevron_right_icon, icon_asset, is_dark_theme,
        popup_menu::{PopupMenuEntry, popup_menu},
        sidebar_bg, sidebar_card_bg, sidebar_card_shadow, sidebar_hover_bg, subtle_surface,
        success_soft, text_primary, text_secondary, tint,
        tooltip::shortcut_label,
        truncated_line::{Segment, TruncatedLine},
        warning,
    },
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Anchor, AnyElement, ClickEvent, Context, DragMoveEvent, FontWeight, Hsla, InteractiveElement,
    IntoElement, ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement,
    Styled, UniformListScrollHandle, Window, anchored, deferred, div, point, px, rgb, svg,
};
use std::{cell::Cell, collections::HashMap, rc::Rc};

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
            .text_size(typography::caption())
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(text_secondary()))
            .child(self.title.clone())
    }
}

use crate::views::tooltip::TextTooltip as SidebarTooltip;

pub(super) const ROW_AVATAR_SIZE_PX: f32 = 20.;
pub(super) const ROW_TEXT_SIZE_PX: f32 = 13.;
pub(super) const ROW_INSET_PX: f32 = 8.;
pub(super) const ROW_GAP_PX: f32 = 2.;
const COMPACT_ROW_HEIGHT_PX: f32 = 30.;
const CHANNEL_ROW_HEIGHT_PX: f32 = 26.;
const LEAD_GAP_PX: f32 = 8.;
pub(super) const SIDEBAR_PADDING_PX: f32 = 8.;
/// The narrowest sidebars whose toolbar fits full-size buttons beside the
/// window controls, without and with the update button.
const ROOMY_TOOLBAR_PX: f32 = 260.;
const ROOMY_TOOLBAR_WITH_UPDATE_PX: f32 = 288.;
const TOOLBAR_BUTTON_PX: f32 = 28.;
const TOOLBAR_ICON_PX: f32 = 16.;
const FIRST_SECTION_GAP_PX: f32 = 8.;
const SECTION_HEADER_PX: f32 = 24.;
const HEADER_ICON_PX: f32 = 12.;
const VIEW_OPTIONS_MIN_WIDTH_PX: f32 = 200.;
const SCROLLBAR_GUTTER_PX: f32 = 8.;
const SCROLLBAR_PX: f32 = 8.;
/// The scrollbar sits this far in from the sidebar's right edge, straddling the
/// sidebar's padding and the list's empty gutter.
const SCROLLBAR_EDGE_INSET_PX: f32 = 4.;

/// A menu that pops up from the sidebar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarMenu {
    Order,
    /// Your profile and Settings, from your name in the footer.
    Identity,
}

/// Row style, with what the Previews style needs to know about its width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowLook {
    Compact,
    /// `compact_time`: the sidebar is narrower than Previews' preferred width,
    /// so times shorten ("10:30", "Thu") before names truncate.
    Previews {
        compact_time: bool,
    },
    /// A team's channel in Grouped mode: a compact row, set closer.
    Channel,
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

    pub fn row_px(self) -> f32 {
        typography::sidebar_px(match self {
            Self::Compact => COMPACT_ROW_HEIGHT_PX,
            Self::Previews { .. } => PREVIEW_ROW_HEIGHT_PX,
            Self::Channel => CHANNEL_ROW_HEIGHT_PX,
        })
    }
}

/// Text and fill colors of one sidebar row, from its selection and hover state.
pub(super) struct RowColors {
    pub primary: Hsla,
    pub secondary: Hsla,
    pub background: Option<Hsla>,
    /// What the row sits on: rings around avatars and presence dots use it.
    pub surface: Hsla,
    /// The selected row is a card with a soft shadow.
    pub raised: bool,
}

impl RowColors {
    /// Only unread conversations read in full contrast, so they stand out from
    /// the rest: read rows recede to secondary text and muted ones further still.
    /// Hover and selection bring a row back to full contrast.
    pub fn new(selected: bool, hovered: bool, activity: &RowActivity) -> Self {
        if selected {
            let background: Hsla = rgb(sidebar_card_bg()).into();
            return Self {
                primary: rgb(text_primary()).into(),
                secondary: rgb(text_secondary()).into(),
                background: Some(background),
                surface: background,
                raised: true,
            };
        }
        let background = hovered.then(|| rgb(sidebar_hover_bg()).into());
        let secondary: Hsla = rgb(text_secondary()).into();
        let (primary, secondary) = if hovered || activity.is_unread() {
            (rgb(text_primary()).into(), secondary)
        } else if activity.muted {
            let faded = tint(text_secondary(), 0.6);
            (faded, faded)
        } else {
            (secondary, secondary)
        };
        Self {
            primary,
            secondary,
            background,
            surface: background.unwrap_or_else(|| rgb(sidebar_bg()).into()),
            raised: false,
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
    is_unread: bool,
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
            is_unread: row.is_unread(),
        }
    }

    pub fn is_unread(&self) -> bool {
        self.is_unread
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
                    .h(typography::font_px(16.))
                    .min_w(typography::font_px(16.))
                    .px(px(5.))
                    .rounded_full()
                    .bg(rgb(accent_strong()))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(typography::font_px(11.))
                    .line_height(typography::font_px(16.))
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
/// Shared by Recent and grouped rows.
pub(super) struct CompactRow<'a> {
    pub id: String,
    pub row: &'a SidebarRow,
    pub title: Vec<Segment>,
    pub leading: AnyElement,
    pub colors: &'a RowColors,
    pub selected: bool,
    pub height: f32,
}

impl CompactRow<'_> {
    pub fn render<H: SidebarRowHost>(self, cx: &mut Context<H>) -> AnyElement {
        let Self {
            id,
            row,
            title,
            leading,
            colors,
            selected,
            height,
        } = self;
        let activity = RowActivity::of(row);
        let hover_id = id.clone();
        let route = row.route.clone();
        div()
            .id(SharedString::from(id))
            .relative()
            .h(px(height))
            .w_full()
            .flex_shrink_0()
            .px(px(ROW_INSET_PX))
            .flex()
            .items_center()
            .gap(px(LEAD_GAP_PX))
            .rounded(px(6.))
            .cursor_pointer()
            .when_some(colors.background, |row, background| row.bg(background))
            .when(colors.raised, |row| row.shadow(sidebar_card_shadow()))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.sidebar_set_row_hovered(hover_id.clone(), *hovered, cx);
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
                    .text_size(typography::font_px(ROW_TEXT_SIZE_PX))
                    .child(TruncatedLine::new(title)),
            )
            .when_some(activity.indicator(selected), |row, indicator| {
                row.child(indicator)
            })
            .into_any_element()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolbarAction {
    Back,
    Forward,
    Order,
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
    pub reduced_motion: bool,
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
            if crate::views::window_is_active() {
                0.11
            } else {
                0.06
            },
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
                        ToolbarAction::Order => {
                            this.sidebar_toggle_menu(SidebarMenu::Order, window, cx)
                        }
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

/// Conversation rows as the Previews style draws them, for the menu bar panel.
pub fn preview_rows<H: SidebarRowHost>(
    rows: &[crate::models::recent_sidebar::RecentSidebarRow],
    avatars: &HashMap<String, SidebarAvatar>,
    live: &HashMap<String, crate::models::sidebar_preview::PreviewLive>,
    hovered: Option<&str>,
    cx: &mut Context<H>,
) -> Vec<AnyElement> {
    recent::previews(rows, avatars, live, hovered, cx)
}

/// Height of one Previews row at the current sidebar text size.
pub fn preview_row_height_px() -> f32 {
    typography::sidebar_px(preview_row::PREVIEW_ROW_HEIGHT_PX)
}

/// What a conversation row asks of whoever shows it: the sidebar, or the menu
/// bar panel.
pub trait SidebarRowHost: Sized + 'static {
    fn sidebar_hide_hover_tooltip(&mut self, cx: &mut Context<Self>);
    fn sidebar_navigate_to(&mut self, route: Route, window: &mut Window, cx: &mut Context<Self>);
    /// Marks `row_id` hovered, or unhovered. Leaving a row clears the hover only
    /// if it is still that row's: moving down, the next row's enter arrives first.
    fn sidebar_set_row_hovered(&mut self, row_id: String, hovered: bool, cx: &mut Context<Self>);
    /// A channel row's team logo was clicked: show its team's details.
    fn sidebar_open_conversation_team(&mut self, route: Route, cx: &mut Context<Self>);
}

pub trait SidebarHost: SidebarRowHost {
    fn sidebar_open_team_details(
        &mut self,
        team: crate::domain::conversation::ConversationGroup,
        cx: &mut Context<Self>,
    );
    fn sidebar_show_updates(&mut self, window: &mut Window, cx: &mut Context<Self>);
    fn sidebar_set_mode(&mut self, mode: SidebarMode, cx: &mut Context<Self>);
    fn sidebar_toggle_quick_switcher(&mut self, cx: &mut Context<Self>);
    fn sidebar_open_new_chat(&mut self, window: &mut Window, cx: &mut Context<Self>);
    fn sidebar_open_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>);
    /// Opens the signed-in user's own profile.
    fn sidebar_open_own_profile(&mut self, window: &mut Window, cx: &mut Context<Self>);
    /// Opens `menu`, or closes it when it is already open.
    fn sidebar_toggle_menu(
        &mut self,
        menu: SidebarMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
    /// Closes any open sidebar menu; `dismissed` when a press outside closed it.
    fn sidebar_close_menu(&mut self, dismissed: bool, cx: &mut Context<Self>);
    fn sidebar_show_hover_tooltip(
        &mut self,
        text: String,
        anchor_x: f32,
        anchor_y: f32,
        cx: &mut Context<Self>,
    );
    fn sidebar_toggle_section(&mut self, section_id: SidebarSectionId, cx: &mut Context<Self>);
    /// Persists the share of Grouped mode's list the conversations take;
    /// `None` sizes them automatically. Does nothing when it is unchanged.
    fn sidebar_save_conversations_share(&mut self, share: Option<f32>, cx: &mut Context<Self>);
    fn sidebar_reorder_section(
        &mut self,
        dragged_id: SidebarSectionId,
        target_id: SidebarSectionId,
        cx: &mut Context<Self>,
    );
    fn sidebar_navigate_history(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
    fn sidebar_hovered_row(&self) -> Option<&str>;
}

/// What every grouped row needs besides the row itself.
#[derive(Clone, Copy)]
pub(super) struct RowContext<'a> {
    pub avatars: &'a HashMap<String, SidebarAvatar>,
    pub current_route: &'a Route,
    pub hovered_row: Option<&'a str>,
    pub look: RowLook,
    pub live: &'a HashMap<String, crate::models::sidebar_preview::PreviewLive>,
}

/// The sidebar's scroll positions and Grouped split. Clones share them, so
/// every view that renders the sidebar keeps the same ones.
#[derive(Clone)]
pub struct SidebarHandles {
    pub conversations: ScrollHandle,
    pub teams: ScrollHandle,
    pub recent: UniformListScrollHandle,
    split: Rc<GroupedSplit>,
}

impl SidebarHandles {
    /// `share`: the saved splitter position; `None` sizes the conversations
    /// automatically.
    pub fn new(share: Option<f32>) -> Self {
        Self {
            conversations: ScrollHandle::new(),
            teams: ScrollHandle::new(),
            recent: UniformListScrollHandle::new(),
            split: Rc::new(GroupedSplit {
                height: Cell::new(None),
                share: Cell::new(share),
            }),
        }
    }
}

/// How Grouped mode's list divides between Conversations and teams.
struct GroupedSplit {
    /// The list's height at its last layout.
    height: Cell<Option<f32>>,
    /// Where the splitter sits, as a share of the list; live while it is
    /// dragged. `None` sizes the conversations automatically.
    share: Cell<Option<f32>>,
}

/// Keep toolbar/footer chrome outside the elastic viewport in both sidebar modes,
/// with a scrollbar near the sidebar's right edge that shows on hover and can be dragged.
pub(super) fn scroll_viewport<H: gpui_kit::base::ScrollbarHandle + Clone>(
    id: impl Into<SharedString>,
    handle: &H,
    content: impl IntoElement,
    reduced_motion: bool,
) -> AnyElement {
    let id = id.into();
    let thumb = |thumb: gpui_kit::base::ScrollbarThumbStyle| {
        thumb
            .width(px(SCROLLBAR_PX))
            .inset(px(0.))
            .radius(px(SCROLLBAR_PX / 2.))
    };
    div()
        .relative()
        .flex_1()
        .min_h(px(0.))
        .flex()
        .flex_col()
        // Reach into the sidebar's right padding for the scrollbar, without
        // widening the list.
        .mr(px(SCROLLBAR_EDGE_INSET_PX - SIDEBAR_PADDING_PX))
        .pr(px(SIDEBAR_PADDING_PX - SCROLLBAR_EDGE_INSET_PX))
        .overflow_hidden()
        .child(
            gpui_kit::base::ScrollBounce::new(id.clone(), handle, content)
                .enabled(gpui_kit::base::ScrollBounce::enabled_by_default() && !reduced_motion),
        )
        .child(
            div().absolute().inset_0().child(
                gpui_kit::base::Scrollbar::vertical(handle)
                    .id(SharedString::from(format!("{id}-scrollbar")))
                    .mode(gpui_kit::base::ScrollbarMode::Hover)
                    .viewport_from_layout()
                    .styles(|styles| {
                        styles
                            .track(|track| track.width(px(SCROLLBAR_PX)))
                            .thumb(thumb)
                            .thumb_hover(thumb)
                            .thumb_active(thumb)
                    }),
            ),
        )
        .into_any_element()
}

impl Sidebar {
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep the explicit inputs of this existing view API."
    )]
    pub fn render<H: SidebarHost>(
        &self,
        sidebar: &SidebarModel,
        connectivity: &Connectivity,
        current_user_display_name: &str,
        current_user_avatar_asset: Option<&str>,
        conversation_avatars: &HashMap<String, SidebarAvatar>,
        current_route: &Route,
        hovered_row: Option<&str>,
        handles: &SidebarHandles,
        chrome: SidebarChrome,
        cx: &mut Context<H>,
    ) -> AnyElement {
        let look = RowLook::new(sidebar.style, sidebar.width_px);
        let context = RowContext {
            avatars: conversation_avatars,
            current_route,
            hovered_row,
            look,
            live: &sidebar.live_previews,
        };
        div()
            .w(px(sidebar.width_px))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .px(px(SIDEBAR_PADDING_PX))
            .bg(crate::views::sidebar_surface())
            .overflow_hidden()
            .text_color(rgb(text_primary()))
            .child(Self::render_title_bar_row(
                sidebar.width_px,
                sidebar.mode,
                chrome,
                cx,
            ))
            .when(sidebar.mode == SidebarMode::Recent, |root| {
                root.child(
                    div()
                        .pt(px(FIRST_SECTION_GAP_PX))
                        .h(px(
                            FIRST_SECTION_GAP_PX + typography::sidebar_px(SECTION_HEADER_PX)
                        ))
                        .flex()
                        .items_center()
                        .child(section_label("Recent")),
                )
                .child(recent::render(
                    sidebar,
                    conversation_avatars,
                    current_route,
                    hovered_row,
                    &handles.recent,
                    chrome.reduced_motion,
                    cx,
                ))
            })
            .when(sidebar.mode == SidebarMode::Grouped, |root| {
                root.child(grouped::render(
                    sidebar,
                    &context,
                    handles,
                    chrome.reduced_motion,
                    cx,
                ))
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
        mode: SidebarMode,
        chrome: SidebarChrome,
        cx: &mut Context<H>,
    ) -> AnyElement {
        let view_options = width_px >= VIEW_OPTIONS_MIN_WIDTH_PX;
        let roomy_px = if chrome.update_available {
            ROOMY_TOOLBAR_WITH_UPDATE_PX
        } else {
            ROOMY_TOOLBAR_PX
        };
        let compact = width_px < roomy_px;
        let order_open = chrome.menu == Some(SidebarMenu::Order);
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
        let order = view_options.then(|| {
            let button = ToolbarButton {
                id: "sidebar-view-options",
                tooltip: "Sort by Recent or Group by Team".to_string(),
                shortcut: None,
                icon: "assets/icons/arrow-down-up.svg",
                action: ToolbarAction::Order,
                on: order_open,
                enabled: true,
            }
            .render(button_size, button_size, cx);
            if order_open {
                div()
                    .relative()
                    .child(button)
                    .child(Self::order_menu(mode, cx))
                    .into_any_element()
            } else {
                button
            }
        });
        let mut tools = vec![
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
            // On macOS the window buttons sit to the left; elsewhere history leads the row.
            .when(cfg!(target_os = "macos"), |row| row.justify_end())
            .when(!cfg!(target_os = "macos"), |row| row.justify_between())
            .gap(px(if compact { 4. } else { 8. }))
            // Back and forward are a narrower pair, like Finder's toolbar.
            .child(group(
                history
                    .into_iter()
                    .map(|button| button.render(button_size - 4., button_size, cx))
                    .collect(),
            ))
            .child(group(
                order
                    .into_iter()
                    .chain(
                        tools
                            .into_iter()
                            .map(|button| button.render(button_size, button_size, cx)),
                    )
                    .collect(),
            ))
            .into_any_element()
    }

    fn order_menu<H: SidebarHost>(mode: SidebarMode, cx: &mut Context<H>) -> AnyElement {
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
        deferred(
            anchored()
                .anchor(Anchor::TopRight)
                .offset(point(px(0.), px(4.)))
                .child(popup_menu(
                    "sidebar-order-menu",
                    200.,
                    entries,
                    cx.listener(|this, _, _, cx| this.sidebar_close_menu(true, cx)),
                )),
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
            // Out to the sidebar's edges, so the divider spans its full width.
            .mx(px(-SIDEBAR_PADDING_PX))
            .px_2()
            .py(px(6.))
            .border_t_1()
            .border_color(rgb(crate::views::border()))
            .flex()
            .items_center()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w(px(0.))
                    // Before the button, so the menu's anchor is the button's top edge.
                    .when(menu_open, |identity| {
                        identity.child(deferred(
                            anchored()
                                .anchor(Anchor::BottomLeft)
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
                            // The whole row opens the menu, not just the name.
                            .w_full()
                            .flex()
                            .items_center()
                            .gap_2()
                            .rounded_md()
                            .px(px(6.))
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
                                            .text_size(typography::caption())
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(current_user_display_name.to_string()),
                                    )
                                    .when_some(
                                        connection_status,
                                        |name, (status, tint, tooltip)| {
                                            name.child(
                                                div()
                                                    .id("sidebar-connection-status")
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(4.))
                                                    .text_size(typography::font_px(11.))
                                                    .line_height(typography::font_px(14.))
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
                                        },
                                    ),
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
        offset.y = (offset.y + delta).clamp(-max.y, px(0.));
        scroll_handle.set_offset(offset);
    }

    /// A row in a team or the Conversations section, in the chosen style.
    fn render_row<H: SidebarHost>(
        section_id: &SidebarSectionId,
        row: &SidebarRow,
        context: &RowContext<'_>,
        cx: &mut Context<H>,
    ) -> AnyElement {
        let selected = &row.route == context.current_route;
        let activity = RowActivity::of(row);
        let route_label = row.route.label();
        let row_id = format!("sidebar-{}-{route_label}", section_id.0);
        let colors = RowColors::new(
            selected,
            context.hovered_row == Some(row_id.as_str()),
            &activity,
        );
        let emphasized = !selected && activity.is_unread();
        match context.look {
            RowLook::Previews { compact_time } => {
                let lead = Self::leading_element(
                    row,
                    &route_label,
                    context.avatars,
                    &colors,
                    PREVIEW_AVATAR_PX,
                );
                PreviewRow {
                    row,
                    id: row_id,
                    title: title_segments(row.label.clone(), None, &colors, emphasized),
                    lead,
                    colors: &colors,
                    selected,
                    live: row
                        .route
                        .conversation_key()
                        .and_then(|key| context.live.get(key)),
                    compact_time,
                }
                .render(cx)
            }
            RowLook::Compact | RowLook::Channel => CompactRow {
                height: context.look.row_px(),
                title: title_segments(row.label.clone(), None, &colors, emphasized),
                leading: {
                    let lead = Self::leading_element(
                        row,
                        &route_label,
                        context.avatars,
                        &colors,
                        ROW_AVATAR_SIZE_PX,
                    );
                    if row.team.is_none() {
                        team_logo_link(lead, row, &row_id, cx)
                    } else {
                        lead
                    }
                },
                id: row_id,
                row,
                colors: &colors,
                selected,
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
                .size(px(size))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .child(icon_asset(
                    "assets/icons/hash.svg",
                    if size >= PREVIEW_AVATAR_PX { 18. } else { 16. },
                    hsla_to_rgb(colors.secondary),
                ))
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

/// Keep logo actions separate from the surrounding conversation row.
fn team_logo_link<H: SidebarRowHost>(
    logo: AnyElement,
    row: &SidebarRow,
    id: &str,
    cx: &mut Context<H>,
) -> AnyElement {
    if !matches!(row.route, Route::Channel { .. }) {
        return logo;
    }
    let route = row.route.clone();
    div()
        .id(SharedString::from(format!("{id}-team-details")))
        .cursor_pointer()
        .tooltip(SidebarTooltip::single_line("Team details".into()))
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.sidebar_open_conversation_team(route.clone(), cx);
        }))
        .child(logo)
        .into_any_element()
}

/// A header's disclosure chevron, at its right edge. Shown while collapsed;
/// revealed by hovering `group` while expanded.
pub(super) fn disclosure(collapsed: bool, group: SharedString) -> AnyElement {
    svg()
        .path(if collapsed {
            "assets/icons/chevron-right.svg"
        } else {
            "assets/icons/chevron-down.svg"
        })
        .size(px(HEADER_ICON_PX))
        .flex_shrink_0()
        .text_color(rgb(text_secondary()))
        .when(!collapsed, |chevron| chevron.opacity(0.6))
        .group_hover(group, |style| {
            style.opacity(1.).text_color(rgb(text_primary()))
        })
        .into_any_element()
}

fn section_label(title: &'static str) -> AnyElement {
    div()
        .pl(px(ROW_INSET_PX))
        .text_size(typography::caption())
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(text_secondary()))
        .child(title)
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
