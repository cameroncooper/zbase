use super::{
    CompactRow, ROW_AVATAR_SIZE_PX, RowActivity, RowColors, RowLook, Sidebar, SidebarHost,
    SidebarRowHost,
    preview_row::{PREVIEW_AVATAR_PX, PreviewRow, title_segments},
    team_asset, team_logo_link,
};
use crate::views::typography;
use crate::{
    domain::route::Route,
    models::{
        recent_sidebar::{RecentSidebarRow, ambiguous_channel_labels},
        sidebar_model::{SidebarAvatar, SidebarModel},
        sidebar_preview::PreviewLive,
    },
    views::{avatar::Avatar, text_secondary},
};
use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement, Styled, UniformListScrollHandle, div, px, rgb,
    uniform_list,
};
use std::collections::{HashMap, HashSet};

pub(super) fn render<H: SidebarHost>(
    sidebar: &SidebarModel,
    avatars: &HashMap<String, SidebarAvatar>,
    current: &Route,
    hovered: Option<&str>,
    scroll: &UniformListScrollHandle,
    reduced_motion: bool,
    cx: &mut Context<H>,
) -> AnyElement {
    let filter = sidebar.filter.trim().to_lowercase();
    let rows = sidebar.visible_recent_rows().cloned().collect::<Vec<_>>();
    let ambiguous = ambiguous_channel_labels(&rows);
    let theme = crate::views::current_theme();
    let text_size = typography::sidebar_text_size();
    let window_active = crate::views::window_is_active();
    let avatars = avatars.clone();
    let current = current.clone();
    let hovered = hovered.map(str::to_owned);
    let highlighted = sidebar.highlighted_route.clone();
    let look = RowLook::new(sidebar.style, sidebar.width_px);
    let live = sidebar.live_previews.clone();
    let owner = cx.entity().downgrade();
    if rows.is_empty() {
        return div()
            .flex_1()
            .px_2()
            .py_2()
            .text_size(typography::caption())
            .text_color(rgb(text_secondary()))
            .child(if filter.is_empty() {
                "No conversations yet"
            } else {
                "No matching conversations"
            })
            .into_any_element();
    }
    let list = uniform_list("sidebar-recent-list", rows.len(), move |range, _, app| {
        owner
            .update(app, |_, cx| {
                typography::with_sidebar_size(text_size, || {
                    crate::views::with_theme(theme, || {
                        crate::views::with_window_active(window_active, || {
                            rows[range]
                                .iter()
                                .map(|recent| {
                                    render_row(
                                        recent,
                                        &ambiguous,
                                        &avatars,
                                        RowState {
                                            look,
                                            current: &current,
                                            hovered: hovered.as_deref(),
                                            highlighted: highlighted.as_ref(),
                                            live: &live,
                                        },
                                        cx,
                                    )
                                })
                                .collect::<Vec<_>>()
                        })
                    })
                })
            })
            .unwrap_or_default()
    })
    .track_scroll(scroll)
    .flex_1()
    .min_h(px(0.))
    .pr(px(super::SCROLLBAR_GUTTER_PX));
    super::scroll_viewport("sidebar-recent-bounce", scroll, list, reduced_motion)
}

/// Rows in the Previews style, for the menu bar panel. None reads as the open
/// conversation: Settings is never one.
pub(super) fn previews<H: SidebarRowHost>(
    rows: &[RecentSidebarRow],
    avatars: &HashMap<String, SidebarAvatar>,
    live: &HashMap<String, PreviewLive>,
    hovered: Option<&str>,
    cx: &mut Context<H>,
) -> Vec<AnyElement> {
    let ambiguous = ambiguous_channel_labels(rows);
    let current = Route::Preferences;
    rows.iter()
        .map(|recent| {
            let state = RowState {
                look: RowLook::Previews {
                    compact_time: false,
                },
                current: &current,
                hovered,
                highlighted: None,
                live,
            };
            render_row(recent, &ambiguous, avatars, state, cx)
        })
        .collect()
}

/// What decides a row's look besides the row itself.
struct RowState<'a> {
    look: RowLook,
    current: &'a Route,
    hovered: Option<&'a str>,
    /// Keyboard highlight, drawn like hover.
    highlighted: Option<&'a Route>,
    live: &'a HashMap<String, PreviewLive>,
}

fn render_row<H: SidebarRowHost>(
    recent: &RecentSidebarRow,
    ambiguous: &HashSet<String>,
    avatars: &HashMap<String, SidebarAvatar>,
    state: RowState<'_>,
    cx: &mut Context<H>,
) -> AnyElement {
    let RowState {
        look,
        current,
        hovered,
        highlighted,
        live,
    } = state;
    let row = &recent.row;
    let selected = &row.route == current;
    let activity = RowActivity::of(row);
    let route_key = row.route.label();
    let row_id = format!("sidebar-recent-{route_key}");
    let is_hovered = hovered == Some(row_id.as_str()) || highlighted == Some(&row.route);
    let colors = RowColors::new(selected, is_hovered, &activity);
    let team_label = recent.team_label(ambiguous).map(str::to_owned);
    let emphasized = activity.is_unread() && !selected;
    match look {
        RowLook::Previews { compact_time } => {
            // A channel shows its team's logo; a simple team keeps its mark.
            let lead = match &row.team {
                Some(team) => Avatar::render_team(
                    &team.display_name,
                    team_asset(avatars, &team.id),
                    PREVIEW_AVATAR_PX,
                ),
                None => {
                    Sidebar::leading_element(row, &route_key, avatars, &colors, PREVIEW_AVATAR_PX)
                }
            };
            let lead = team_logo_link(lead, row, &row_id, cx);
            PreviewRow {
                row,
                id: row_id,
                title: title_segments(row.label.clone(), team_label, &colors, emphasized),
                lead,
                colors: &colors,
                selected,
                live: row.route.conversation_key().and_then(|key| live.get(key)),
                compact_time,
            }
            .render(cx)
        }
        RowLook::Compact | RowLook::Channel => {
            let label = if recent.is_channel {
                format!("#{}", row.label)
            } else {
                row.label.clone()
            };
            let leading = match &row.team {
                Some(team) => Avatar::render_team(
                    &team.display_name,
                    team_asset(avatars, &team.id),
                    ROW_AVATAR_SIZE_PX,
                ),
                None => {
                    Sidebar::leading_element(row, &route_key, avatars, &colors, ROW_AVATAR_SIZE_PX)
                }
            };
            let leading = team_logo_link(leading, row, &row_id, cx);
            CompactRow {
                id: row_id,
                row,
                title: title_segments(label, team_label, &colors, emphasized),
                leading,
                colors: &colors,
                selected,
                height: look.row_px(),
            }
            .render(cx)
        }
    }
}
