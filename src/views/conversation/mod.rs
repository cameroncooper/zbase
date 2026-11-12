use crate::views::typography;
use crate::{
    domain::{
        conversation::ConversationKind,
        pins::{PinnedItem, PinnedTarget},
    },
    models::{
        app_model::Connectivity, composer_model::ComposerModel,
        conversation_model::ConversationModel, find_in_chat_model::FindInChatModel,
        navigation_model::RightPaneMode, settings_model::SettingsModel,
        timeline_model::TimelineModel,
    },
    views::{
        accent, accent_soft,
        app_window::AppWindow,
        avatar::{Avatar, presence_label},
        chevron_down_icon, close_icon,
        composer::ComposerPanel,
        content_surface, glass_surface_dark, icon_asset,
        input::TextField,
        is_dark_theme, pin_icon, search_icon, shell_border, subtle_surface, text_primary,
        text_secondary,
        timeline::TimelineList,
        tint,
        tooltip::{TextTooltip, shortcut_label},
    },
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Animation, AnimationExt, AnyElement, Context, CursorStyle, Entity, ExternalPaths, FontWeight,
    InteractiveElement, IntoElement, ListState, ParentElement, StatefulInteractiveElement, Styled,
    div, px, rgb,
};
use std::time::Duration;

#[derive(Default)]
pub struct ConversationView;

#[derive(Default)]
pub struct ConversationHeader;

impl ConversationView {
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep the explicit inputs of this existing view API."
    )]
    pub fn render(
        &self,
        conversation: &ConversationModel,
        timeline: &TimelineModel,
        find_in_chat: &FindInChatModel,
        find_in_chat_input: &Entity<TextField>,
        composer: &ComposerModel,
        composer_input: &Entity<TextField>,
        composer_state: crate::views::composer::ComposerFieldState,
        right_pane: &RightPaneMode,
        timeline_list_state: &ListState,
        unseen_message_count: usize,
        show_jump_to_bottom: bool,
        settings: &SettingsModel,
        connectivity: &Connectivity,
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        let reduced_motion = settings.reduced_motion;
        // Who is typing is unknown while disconnected; don't show stale news.
        let typing_label = timeline
            .typing_text
            .as_ref()
            .filter(|label| *connectivity == Connectivity::Online && !label.trim().is_empty());
        let show_jump = show_jump_to_bottom || unseen_message_count > 0;
        let jump_label = if unseen_message_count > 0 {
            format!(
                "{} new message{}",
                unseen_message_count,
                if unseen_message_count == 1 { "" } else { "s" }
            )
        } else {
            "Jump to latest".to_string()
        };
        let jump_pill = div()
            .id("conversation-jump-latest")
            .on_click(cx.listener(AppWindow::scroll_timeline_to_bottom))
            .on_mouse_move(cx.listener(|this, _, _, cx| {
                this.clear_hovered_message(cx);
                cx.stop_propagation();
            }))
            .rounded_full()
            .bg(glass_surface_dark().opacity(0.92))
            .border_1()
            .border_color(shell_border())
            .px_3()
            .py_1p5()
            .flex()
            .items_center()
            .gap_2()
            .text_size(typography::caption())
            .text_color(rgb(text_primary()))
            .hover(|s| s.bg(glass_surface_dark().opacity(0.98)))
            .child(chevron_down_icon(text_primary()))
            .child(jump_label);
        div()
            .id("conversation-view")
            .flex_1()
            .h_full()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .on_click(cx.listener(|this, _, window, cx| {
                if !this.any_text_selected(cx) {
                    this.restore_chat_focus(window, cx);
                }
            }))
            .drag_over::<ExternalPaths>(|style, _, _, _| {
                style
                    .bg(if is_dark_theme() {
                        tint(crate::views::main_bg(), 0.72)
                    } else {
                        tint(0xe8eef5, 0.70)
                    })
                    .border_2()
                    .border_color(rgb(accent()))
            })
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                let file_paths = paths.paths().to_vec();
                if !file_paths.is_empty() {
                    this.open_composer_file_upload_with_paths(file_paths, cx);
                }
            }))
            .child(ConversationHeader.render(conversation, right_pane, find_in_chat.open, cx))
            .when_some(connection_banner(connectivity, cx), |container, banner| {
                container.child(banner)
            })
            .when_some(conversation.pinned_message.as_ref(), |container, pinned| {
                container.child(Self::render_pinned_banner(pinned, cx))
            })
            .when(find_in_chat.open, |container| {
                container.child(Self::render_find_bar(find_in_chat, find_in_chat_input, cx))
            })
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .relative()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(TimelineList.render(timeline, timeline_list_state, reduced_motion, cx))
                    .when(show_jump, |container| {
                        container.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom(px(8.))
                                .flex()
                                .justify_center()
                                .child(jump_pill),
                        )
                    }),
            )
            .when(conversation.can_post, |container| {
                container.child(ComposerPanel.render(
                    composer,
                    composer_input,
                    composer_state,
                    typing_label.map(|label| typing_indicator(label.clone(), reduced_motion)),
                    cx,
                ))
            })
            .when(!conversation.can_post, |container| {
                container.child(Self::render_join_bar(conversation, cx))
            })
            .into_any_element()
    }

    fn render_join_bar(
        conversation: &ConversationModel,
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        let channel_label = conversation.summary.title.trim();
        let join_label = if channel_label.is_empty() {
            "Join Channel".to_string()
        } else {
            format!("Join #{channel_label}")
        };
        div()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(shell_border())
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(typography::body())
                    .text_color(rgb(text_secondary()))
                    .child("Join to send messages."),
            )
            .child(
                div()
                    .id("conversation-join-button")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(accent()))
                    .text_size(typography::body())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(accent_soft())))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.join_current_channel(window, cx);
                    }))
                    .child(join_label),
            )
            .into_any_element()
    }

    fn render_find_bar(
        find_in_chat: &FindInChatModel,
        find_in_chat_input: &Entity<TextField>,
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        let result_count = find_in_chat.results.len();
        let can_navigate = result_count > 0;
        let status = if find_in_chat.query.trim().is_empty() {
            "Type to search this conversation".to_string()
        } else if find_in_chat.loading {
            "Searching…".to_string()
        } else if result_count == 0 {
            "0 results".to_string()
        } else if let Some(selected) = find_in_chat.selected_index {
            format!("{} of {}", selected.saturating_add(1), result_count)
        } else {
            format!(
                "{} result{}",
                result_count,
                if result_count == 1 { "" } else { "s" }
            )
        };

        div()
            .id("conversation-find-bar")
            .px_3()
            .py_1p5()
            .flex()
            .items_center()
            .gap_2()
            .bg(rgb(accent_soft()))
            .child(search_icon(accent()))
            .child(
                div()
                    .id("conversation-find-input")
                    .flex_1()
                    .min_w(px(0.))
                    .on_click(cx.listener(AppWindow::focus_find_in_chat_input))
                    .child(find_in_chat_input.clone()),
            )
            .child(
                div()
                    .text_size(typography::caption())
                    .text_color(rgb(text_secondary()))
                    .child(status),
            )
            .children(
                [
                    (
                        "conversation-find-previous",
                        "Previous match",
                        "Shift+Enter",
                        "assets/icons/chevron-up.svg",
                        true,
                    ),
                    (
                        "conversation-find-next",
                        "Next match",
                        "Enter",
                        "assets/icons/chevron-down.svg",
                        false,
                    ),
                ]
                .into_iter()
                .map(|(id, label, shortcut, icon, previous)| {
                    div()
                        .id(id)
                        .w(px(22.))
                        .h(px(22.))
                        .flex_shrink_0()
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .opacity(if can_navigate { 1. } else { 0.35 })
                        .tooltip(TextTooltip::with_shortcut(label.into(), shortcut.into()))
                        .when(can_navigate, |button| {
                            button
                                .cursor(CursorStyle::PointingHand)
                                .hover(|style| style.bg(subtle_surface()))
                                .on_click(cx.listener(move |_, _, window, cx| {
                                    cx.stop_propagation();
                                    if previous {
                                        window.dispatch_action(
                                            Box::new(crate::app::commands::FindPrevMatch),
                                            cx,
                                        );
                                    } else {
                                        window.dispatch_action(
                                            Box::new(crate::app::commands::FindNextMatch),
                                            cx,
                                        );
                                    }
                                }))
                        })
                        .child(icon_asset(icon, 14., text_secondary()))
                }),
            )
            .child(
                div()
                    .id("conversation-find-close")
                    .w(px(22.))
                    .h(px(22.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(subtle_surface()))
                    .on_click(cx.listener(AppWindow::close_find_in_chat_click))
                    .child(close_icon(text_secondary())),
            )
            .into_any_element()
    }

    fn render_pinned_banner(pinned: &PinnedItem, cx: &mut Context<AppWindow>) -> AnyElement {
        let preview_text = pinned
            .preview
            .as_ref()
            .and_then(|preview| preview.text.as_deref())
            .map(compact_pinned_preview_text)
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| "Pinned message".to_string());
        let is_message_pin = matches!(&pinned.target, PinnedTarget::Message { .. });
        div()
            .id("conversation-pinned-banner")
            .px_3()
            .py_1p5()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .bg(rgb(accent_soft()))
            .child(
                div()
                    .id("conversation-pinned-banner-open")
                    .flex()
                    .flex_1()
                    .items_center()
                    .gap_2()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .on_click(cx.listener(AppWindow::jump_to_pinned_message))
                    .child(pin_icon(accent()))
                    .child(
                        div()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .text_size(typography::caption())
                            .text_color(rgb(text_secondary()))
                            .child(preview_text),
                    )
                    .when(!is_message_pin, |d| {
                        d.text_color(rgb(text_secondary()))
                            .child(div().text_size(typography::caption()).child("Unavailable"))
                    }),
            )
            .child(
                div()
                    .id("conversation-pinned-banner-dismiss")
                    .w(px(22.))
                    .h(px(22.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(subtle_surface()))
                    .on_click(cx.listener(AppWindow::dismiss_pinned_banner))
                    .child(close_icon(text_secondary())),
            )
            .into_any_element()
    }
}

fn compact_pinned_preview_text(raw: &str) -> String {
    let mut lines = raw.lines().map(str::trim).filter(|line| !line.is_empty());
    let mut preview_lines = Vec::new();
    for line in lines.by_ref().take(2) {
        preview_lines.push(line.to_string());
    }
    if preview_lines.is_empty() {
        return String::new();
    }
    let mut preview = preview_lines.join("\n");
    const MAX_CHARS: usize = 180;
    if preview.chars().count() > MAX_CHARS {
        preview = preview.chars().take(MAX_CHARS).collect::<String>();
        preview.push('…');
        return preview;
    }
    if lines.next().is_some() {
        preview.push('…');
    }
    preview
}

/// One trailing header action: a 28px icon button, optionally with a count.
struct HeaderAction {
    id: &'static str,
    icon: &'static str,
    label: &'static str,
    shortcut: &'static str,
    count: Option<u32>,
    active: bool,
    pane: Option<RightPaneMode>,
}

impl ConversationHeader {
    pub fn render(
        &self,
        conversation: &ConversationModel,
        right_pane: &RightPaneMode,
        find_open: bool,
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        let kind = &conversation.summary.kind;
        let is_channel = matches!(kind, ConversationKind::Channel);
        let is_dm = matches!(kind, ConversationKind::DirectMessage);
        let title = if is_channel {
            conversation
                .summary
                .simple_team_label()
                .unwrap_or_else(|| format!("#{}", conversation.summary.title))
        } else {
            conversation.summary.title.clone()
        };
        let subtitle = Self::subtitle(conversation);
        let title_content = div()
            .flex()
            .flex_col()
            .min_w(px(0.))
            .max_w_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .min_w(px(0.))
                    .max_w_full()
                    .child(
                        div()
                            .text_size(typography::font_px(17.))
                            .line_height(typography::font_px(22.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(text_primary()))
                            .min_w(px(0.))
                            .truncate()
                            .child(title),
                    )
                    .when(is_channel || is_dm, |row| {
                        row.child(chevron_down_icon(text_secondary()))
                    }),
            )
            .when_some(subtitle, |block, subtitle| {
                block.child(
                    div().flex().min_w(px(0.)).max_w_full().child(
                        div()
                            .min_w(px(0.))
                            .max_w_full()
                            .text_size(typography::font_px(12.))
                            .line_height(typography::font_px(16.))
                            .text_color(rgb(text_secondary()))
                            .whitespace_normal()
                            .line_clamp(1)
                            .text_ellipsis()
                            .child(subtitle),
                    ),
                )
            });
        // Only the conversation name/subtitle opens the details or profile.
        // The empty space beside it stays available for window dragging.
        let title_content = if is_channel {
            div()
                .id("conversation-header-title")
                .flex()
                .min_w(px(0.))
                .max_w_full()
                .cursor(CursorStyle::PointingHand)
                .tooltip(TextTooltip::with_shortcut(
                    "Conversation details".to_string(),
                    shortcut_label("⇧⌘D"),
                ))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.toggle_pane(RightPaneMode::Details, window, cx);
                }))
                .child(title_content)
                .into_any_element()
        } else if is_dm {
            div()
                .id("conversation-header-title")
                .flex()
                .min_w(px(0.))
                .max_w_full()
                .cursor(CursorStyle::PointingHand)
                .tooltip(TextTooltip::single_line("View profile".to_string()))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.open_dm_header_profile(cx);
                }))
                .child(title_content)
                .into_any_element()
        } else {
            title_content.into_any_element()
        };
        let title_row = div()
            .flex_1()
            .flex()
            .min_w(px(0.))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap_3()
                    .min_w(px(0.))
                    .child(Self::conversation_badge(conversation, cx))
                    .child(title_content),
            )
            .into_any_element();

        let member_count = conversation
            .details
            .as_ref()
            .map(|details| details.member_count)
            .filter(|count| *count > 0)
            .unwrap_or(conversation.member_count);
        let mut actions = vec![HeaderAction {
            id: "conversation-header-find",
            icon: "assets/icons/search.svg",
            label: "Find in conversation",
            shortcut: "⌘F",
            count: None,
            active: find_open,
            pane: None,
        }];
        if !is_dm {
            actions.push(HeaderAction {
                id: "conversation-header-members",
                icon: "assets/icons/members.svg",
                label: "Members",
                shortcut: "⇧⌘M",
                count: (member_count > 0).then_some(member_count),
                active: *right_pane == RightPaneMode::Members,
                pane: Some(RightPaneMode::Members),
            });
        }
        actions.push(HeaderAction {
            id: "conversation-header-files",
            icon: "assets/icons/files.svg",
            label: "Files",
            shortcut: "⇧⌘F",
            count: None,
            active: *right_pane == RightPaneMode::Files,
            pane: Some(RightPaneMode::Files),
        });
        if is_channel {
            actions.push(HeaderAction {
                id: "conversation-header-details",
                icon: "assets/icons/info.svg",
                label: "Conversation details",
                shortcut: "⇧⌘D",
                count: None,
                active: *right_pane == RightPaneMode::Details,
                pane: Some(RightPaneMode::Details),
            });
        }

        div()
            .min_h(typography::font_px(56.))
            .flex_shrink_0()
            .px_4()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .border_b_1()
            .border_color(shell_border())
            .child(title_row)
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(2.))
                    .children(
                        actions
                            .into_iter()
                            .map(|action| Self::render_action(action, cx)),
                    ),
            )
            .into_any_element()
    }

    /// Channel: team and topic. DM: the other person's presence and status.
    fn subtitle(conversation: &ConversationModel) -> Option<String> {
        let topic = conversation.summary.topic.trim();
        match conversation.summary.kind {
            ConversationKind::Channel => {
                // A single-channel team's title is the team ("bookclub"); its
                // channel is the quiet descriptor, as in the switcher.
                let team = if conversation.summary.is_simple_team() {
                    Some(format!("#{}", conversation.summary.title))
                } else {
                    conversation
                        .summary
                        .group
                        .as_ref()
                        .map(|group| group.display_name.clone())
                };
                match (team, topic.is_empty()) {
                    (Some(team), false) => Some(format!("{team} · {topic}")),
                    (Some(team), true) => Some(team),
                    (None, false) => Some(topic.to_string()),
                    (None, true) => None,
                }
            }
            ConversationKind::DirectMessage => {
                let presence = conversation
                    .participant_avatars
                    .first()
                    .and_then(|participant| participant.presence.as_ref())?;
                let label = presence_label(&presence.availability)?;
                Some(
                    match presence
                        .status_text
                        .as_deref()
                        .map(str::trim)
                        .filter(|status| !status.is_empty())
                    {
                        Some(status) => format!("{label} · {status}"),
                        None => label.to_string(),
                    },
                )
            }
            ConversationKind::GroupDirectMessage => {
                let count = conversation
                    .details
                    .as_ref()
                    .map(|details| details.member_count)
                    .filter(|count| *count > 0)
                    .unwrap_or(conversation.member_count);
                (count > 0).then(|| format!("{count} members"))
            }
        }
    }

    fn render_action(action: HeaderAction, cx: &mut Context<AppWindow>) -> AnyElement {
        let HeaderAction {
            id,
            icon,
            label,
            shortcut,
            count,
            active,
            pane,
        } = action;
        let color = if active {
            text_primary()
        } else {
            text_secondary()
        };
        div()
            .id(id)
            .h(px(28.))
            .min_w(px(28.))
            .when(count.is_some(), |button| button.px(px(6.)))
            .rounded(px(6.))
            .flex()
            .items_center()
            .justify_center()
            .gap_1()
            .cursor(CursorStyle::PointingHand)
            // The pressed fill recedes while the window is in the background.
            .when(active, |button| {
                button.bg(tint(
                    text_primary(),
                    if crate::views::window_is_active() {
                        0.10
                    } else {
                        0.05
                    },
                ))
            })
            .when(!active, |button| {
                button.hover(|style| style.bg(tint(text_primary(), 0.07)))
            })
            .tooltip(TextTooltip::with_shortcut(
                label.to_string(),
                shortcut_label(shortcut),
            ))
            .on_click(cx.listener(move |this, _, window, cx| match &pane {
                Some(pane) => this.toggle_pane(pane.clone(), window, cx),
                None => this.toggle_find_in_chat(window, cx),
            }))
            .child(icon_asset(icon, 16., color))
            .when_some(count, |button, count| {
                button.child(
                    div()
                        .text_size(typography::font_px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(color))
                        .child(count.to_string()),
                )
            })
            .into_any_element()
    }

    fn conversation_badge(
        conversation: &ConversationModel,
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        match conversation.summary.kind {
            ConversationKind::Channel => {
                let team_name = conversation
                    .summary
                    .group
                    .as_ref()
                    .map(|group| group.display_name.as_str())
                    .unwrap_or(conversation.summary.title.as_str());
                let badge =
                    Avatar::render_team(team_name, conversation.avatar_asset.as_deref(), 32.);
                if let Some(team) = conversation.summary.group.clone() {
                    div()
                        .id("chat-header-team-details")
                        .cursor(CursorStyle::PointingHand)
                        .tooltip(TextTooltip::single_line("Team details".into()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.open_team_details(team.clone(), cx);
                        }))
                        .child(badge)
                        .into_any_element()
                } else {
                    badge
                }
            }
            ConversationKind::DirectMessage | ConversationKind::GroupDirectMessage => {
                Avatar::render_participants_on(
                    &conversation.participant_avatars,
                    &conversation.summary.title,
                    32.,
                    content_surface(),
                )
            }
        }
    }
}

/// "Lena Fischer is typing…" with three pulsing dots (static when reduced
/// motion is on), in the row the composer reserves under its field.
fn typing_indicator(label: String, reduced_motion: bool) -> AnyElement {
    let dots = (0..3).map(|index| {
        let dot = div().size(px(4.)).rounded_full().bg(rgb(text_secondary()));
        if reduced_motion {
            dot.opacity(0.7).into_any_element()
        } else {
            dot.with_animation(
                ("typing-dot", index),
                Animation::new(Duration::from_millis(1200)).repeat(),
                move |dot, delta| dot.opacity(typing_dot_opacity(delta, index)),
            )
            .into_any_element()
        }
    });
    div()
        .min_w(px(0.))
        .flex()
        .items_center()
        .gap_2()
        .child(div().flex_shrink_0().flex().gap(px(3.)).children(dots))
        .child(
            div()
                .min_w(px(0.))
                .text_size(typography::caption())
                .text_color(rgb(text_secondary()))
                .truncate()
                .child(label),
        )
        .into_any_element()
}

/// Opacity of typing dot `index` at animation progress `delta` (0..1): each dot
/// brightens in turn, a fifth of a cycle apart, then rests.
fn typing_dot_opacity(delta: f32, index: usize) -> f32 {
    let phase = (delta - index as f32 * 0.2).rem_euclid(1.0);
    let pulse = if phase < 0.4 {
        (phase / 0.4 * std::f32::consts::PI).sin()
    } else {
        0.
    };
    0.35 + 0.65 * pulse
}

/// Strip under the header while Keybase is unreachable: what happened and what it
/// means for sending, as a warning (amber), not an error. Nothing while connected.
/// Once automatic reconnects stop, it offers to reconnect.
fn connection_banner(
    connectivity: &Connectivity,
    cx: &mut Context<AppWindow>,
) -> Option<AnyElement> {
    let notice = connectivity.send_notice()?;
    let icon = match connectivity {
        Connectivity::Offline => "assets/icons/cloud-off.svg",
        _ => "assets/icons/info.svg",
    };
    let foreground = crate::views::warning_banner_fg();
    Some(
        div()
            .id("conversation-connection-banner")
            .flex_shrink_0()
            .min_h(px(36.))
            .px(px(16.))
            .py(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .bg(rgb(crate::views::warning_banner_bg()))
            .border_b_1()
            .border_color(shell_border())
            .text_size(typography::label())
            .line_height(typography::font_px(18.))
            .text_color(rgb(foreground))
            .child(icon_asset(icon, 14., foreground))
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(notice.banner_title),
            )
            .child(div().flex_1().min_w(px(0.)).child(notice.banner_detail))
            .when_some(notice.reconnect_label, |banner, label| {
                banner.child(
                    div()
                        .id("conversation-connection-reconnect")
                        .flex_shrink_0()
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor(CursorStyle::PointingHand)
                        .hover(|style| style.underline())
                        .on_click(cx.listener(|this, _, _, cx| this.reconnect_accounts(cx)))
                        .child(label),
                )
            })
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::typing_dot_opacity;

    #[test]
    fn typing_dots_pulse_in_turn_within_their_range() {
        for step in 0..=100 {
            let delta = step as f32 / 100.;
            for index in 0..3 {
                let opacity = typing_dot_opacity(delta, index);
                assert!(
                    (0.35..=1.0).contains(&opacity),
                    "{delta} {index}: {opacity}"
                );
            }
        }
        // Each dot peaks a fifth of a cycle after the previous one.
        assert!((typing_dot_opacity(0.2, 0) - 1.0).abs() < 1e-4);
        assert!((typing_dot_opacity(0.4, 1) - 1.0).abs() < 1e-4);
        assert!((typing_dot_opacity(0.6, 2) - 1.0).abs() < 1e-4);
        assert!((typing_dot_opacity(0.6, 0) - 0.35).abs() < 1e-4);
    }
}
