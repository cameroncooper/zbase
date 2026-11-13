use crate::views::typography;
use crate::{
    models::{
        app_model::SendNotice,
        composer_model::{
            AutocompleteCandidate, AutocompleteState, ComposerMode, ComposerModel, MarkdownFormat,
        },
    },
    views::{
        accent, accent_soft, accent_strong,
        app_window::AppWindow,
        card_shadow, field_bg, field_border, icon_asset,
        input::TextField,
        is_dark_theme, modal_surface,
        popup_menu::{PopupMenuEntry, popup_menu},
        send_icon, shell_border_strong, text_primary, text_secondary, tint,
        tooltip::{TextTooltip, shortcut_label},
    },
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Anchor, AnyElement, ClickEvent, Context, CursorStyle, Entity, FontWeight, Hsla,
    InteractiveElement, IntoElement, ParentElement, ScrollHandle, StatefulInteractiveElement,
    Styled, anchored, deferred, div, point, px, rgb,
};

#[derive(Default)]
pub struct ComposerPanel;

pub(crate) mod giphy_picker;

/// Which input a composer field drives; picks the handlers and element ids.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComposerTarget {
    Conversation,
    Thread,
}

/// Visible state of a composer field.
pub(crate) struct ComposerFieldState {
    pub can_send: bool,
    /// While Keybase can't be reached: what the send tooltip says.
    pub send_notice: Option<SendNotice>,
    pub format_menu_open: bool,
    /// The open @mention or :emoji: list, from [`render_autocomplete_popup`].
    pub autocomplete: Option<AnyElement>,
}

/// Bordered message field shared by the conversation and thread composers: an
/// 8px-radius box (40px tall at one line) with attach, emoji and send controls
/// inside its right edge. It shows no focus highlight: switching chats focuses
/// it automatically, so a ring would be lit nearly all the time.
pub(crate) fn composer_field(
    target: ComposerTarget,
    editor: &Entity<TextField>,
    state: ComposerFieldState,
    fill: Hsla,
    cx: &mut Context<AppWindow>,
) -> AnyElement {
    let thread = target == ComposerTarget::Thread;
    let (surface_id, attach_id, emoji_id, send_id, mention_id, format_id, menu_id) = if thread {
        (
            "thread-pane-input-surface",
            "thread-attach-inline",
            "thread-emoji-inline",
            "thread-send",
            "thread-mention-inline",
            "thread-format-inline",
            "thread-format-menu",
        )
    } else {
        (
            "composer-input-surface",
            "composer-attach-inline",
            "composer-emoji-inline",
            "composer-send",
            "composer-mention-inline",
            "composer-format-inline",
            "composer-format-menu",
        )
    };
    let tool = composer_tool;
    let send = div()
        .id(send_id)
        .w(px(28.))
        .h(px(28.))
        .flex_shrink_0()
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .when(state.can_send, |button| {
            button
                .bg(rgb(accent_strong()))
                .cursor(CursorStyle::PointingHand)
                .hover(|style| style.bg(rgb(accent())))
                .when_some(
                    state.send_notice.and_then(|notice| notice.send_tooltip),
                    |button, offline| button.tooltip(TextTooltip::single_line(offline.to_string())),
                )
                .when(
                    state
                        .send_notice
                        .and_then(|notice| notice.send_tooltip)
                        .is_none(),
                    |button| {
                        button.tooltip(TextTooltip::with_shortcut(
                            if thread { "Send reply" } else { "Send" }.to_string(),
                            "↩".to_string(),
                        ))
                    },
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    if thread {
                        this.send_thread_reply(window, cx);
                    } else {
                        this.send_composer_message(window, cx);
                    }
                }))
                .child(send_icon(0xffffff))
        })
        .when(!state.can_send, |button| {
            button.child(div().opacity(0.45).child(send_icon(text_secondary())))
        });

    div()
        .id(surface_id)
        .relative()
        .w_full()
        .min_h(px(40.))
        .rounded(px(8.))
        .border_1()
        .border_color(rgb(field_border()))
        .bg(fill)
        .text_size(typography::body())
        .line_height(typography::font_px(22.))
        .text_color(rgb(text_primary()))
        .flex()
        .items_end()
        .cursor(CursorStyle::IBeam)
        .on_click(cx.listener(move |this, event, window, cx| {
            if thread {
                this.focus_thread_input(event, window, cx);
            } else {
                this.focus_composer_input(event, window, cx);
            }
        }))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .pl_3()
                .pr_1()
                // 22px line + 2 * 8px + 2px border = 40px for one line.
                .py(px(8.))
                .child(editor.clone()),
        )
        .child(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(2.))
                .pr(px(5.))
                .pb(px(5.))
                .child(format_tool(
                    format_id,
                    menu_id,
                    target,
                    state.format_menu_open,
                    cx,
                ))
                .child(
                    tool(mention_id, at_icon_sized(text_secondary()))
                        .tooltip(TextTooltip::single_line("Mention someone".to_string()))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.composer_insert_mention(target, window, cx);
                        })),
                )
                .child(
                    tool(attach_id, paperclip_icon_sized(text_secondary()))
                        .tooltip(TextTooltip::single_line("Attach files".to_string()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if thread {
                                this.open_thread_file_upload_picker(cx);
                            } else {
                                this.open_composer_file_upload_picker(cx);
                            }
                        })),
                )
                .child(
                    tool(emoji_id, emoji_icon_sized(text_secondary()))
                        .tooltip(TextTooltip::single_line("Emoji".to_string()))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_emoji_picker(cx);
                        })),
                )
                .child(send),
        )
        // Sits on the field's top edge at the field's width, however tall the
        // draft has grown; deferred so it paints over the timeline.
        .when_some(state.autocomplete, |field, popup| {
            field.child(deferred(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_full()
                    .pb(px(6.))
                    .child(popup),
            ))
        })
        .into_any_element()
}

/// A 28px icon button inside the composer field's right edge.
fn composer_tool(id: &'static str, icon: AnyElement) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .w(px(28.))
        .h(px(28.))
        .flex_shrink_0()
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .cursor(CursorStyle::PointingHand)
        .hover(|style| style.bg(tint(text_primary(), 0.08)))
        .child(icon)
}

/// "Aa": opens a small menu that wraps the selection in Keybase Markdown.
fn format_tool(
    id: &'static str,
    menu_id: &'static str,
    target: ComposerTarget,
    menu_open: bool,
    cx: &mut Context<AppWindow>,
) -> AnyElement {
    let entries = MarkdownFormat::ALL
        .into_iter()
        .map(|format| {
            let entry = PopupMenuEntry::item(
                format.label(),
                cx.listener(move |this: &mut AppWindow, _: &ClickEvent, window, cx| {
                    this.apply_composer_format(target, format, window, cx);
                }),
            );
            match format {
                MarkdownFormat::Bold => entry.shortcut(shortcut_label("⌘B")),
                MarkdownFormat::Italic => entry.shortcut(shortcut_label("⌘I")),
                MarkdownFormat::Code => entry.shortcut(shortcut_label("⌘E")),
                _ => entry,
            }
        })
        .collect::<Vec<_>>();
    div()
        .relative()
        .flex_shrink_0()
        // Before the button, so the menu's anchor is the button's top edge.
        .when(menu_open, |wrapper| {
            wrapper.child(deferred(
                anchored()
                    .anchor(Anchor::BottomLeft)
                    .offset(point(px(0.), px(-6.)))
                    .child(popup_menu(
                        menu_id,
                        180.,
                        entries,
                        cx.listener(|this, _, _, cx| this.close_composer_format_menu(cx)),
                    )),
            ))
        })
        .child(
            composer_tool(
                id,
                div()
                    .text_size(typography::label())
                    .line_height(typography::font_px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(text_secondary()))
                    .child("Aa")
                    .into_any_element(),
            )
            .when(menu_open, |button| button.bg(tint(text_primary(), 0.08)))
            .when(!menu_open, |button| {
                button.tooltip(TextTooltip::single_line("Formatting".to_string()))
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_composer_format_menu(target, cx);
            })),
        )
        .into_any_element()
}

/// Space between the composer field and the bottom of the window or pane.
pub(crate) const COMPOSER_BOTTOM_INSET_PX: f32 = 16.;

/// The row under the conversation composer that says who is typing.
const TYPING_SLOT_HEIGHT_PX: f32 = 18.;

/// The composer field's fill: the window's white in light mode, a raised
/// field in dark mode where the panel is already dark.
pub(crate) fn composer_fill() -> Hsla {
    if is_dark_theme() {
        rgb(field_bg()).into()
    } else {
        rgb(crate::views::main_bg()).into()
    }
}

fn at_icon_sized(color: u32) -> AnyElement {
    icon_asset("assets/icons/at-sign.svg", 16., color)
}

fn paperclip_icon_sized(color: u32) -> AnyElement {
    icon_asset("assets/icons/paperclip.svg", 16., color)
}

fn emoji_icon_sized(color: u32) -> AnyElement {
    icon_asset("assets/icons/emoji.svg", 16., color)
}

impl ComposerPanel {
    pub fn render(
        &self,
        composer: &ComposerModel,
        editor: &Entity<TextField>,
        state: ComposerFieldState,
        typing: Option<AnyElement>,
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        let mode_banner = match &composer.mode {
            ComposerMode::Compose => None,
            ComposerMode::Edit { .. } => Some(
                div()
                    .pl_2()
                    .border_l_2()
                    .border_color(rgb(accent()))
                    .text_size(typography::caption())
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(text_secondary()))
                    .child("Editing · press Escape to cancel")
                    .into_any_element(),
            ),
            ComposerMode::ReplyInThread { .. } => Some(
                div()
                    .pl_2()
                    .border_l_2()
                    .border_color(rgb(accent()))
                    .text_size(typography::caption())
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(text_secondary()))
                    .child("Replying in thread")
                    .into_any_element(),
            ),
        };

        div()
            .px_4()
            .pt_2()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .when_some(mode_banner, |div, banner| div.mb_2().child(banner))
            .child(composer_field(
                ComposerTarget::Conversation,
                editor,
                state,
                composer_fill(),
                cx,
            ))
            // Under the field, as in Keybase. Always reserved, so nothing shifts
            // when typing starts or stops; it is also the composer's bottom inset.
            .child(
                div()
                    .h(typography::font_px(TYPING_SLOT_HEIGHT_PX))
                    // Lines the dots up with the field's text.
                    .pl(px(13.))
                    .flex()
                    .items_center()
                    .children(typing),
            )
            .into_any_element()
    }
}

/// The inline @mention / :emoji: list. It scrolls, and the caller keeps the
/// highlighted row in view through `scroll`.
pub(crate) fn render_autocomplete_popup(
    autocomplete: &AutocompleteState,
    target: ComposerTarget,
    scroll: &ScrollHandle,
    cx: &mut Context<AppWindow>,
) -> AnyElement {
    let (list_id, row_id) = match target {
        ComposerTarget::Conversation => (
            "inline-autocomplete-composer-list",
            "inline-autocomplete-composer",
        ),
        ComposerTarget::Thread => (
            "inline-autocomplete-thread-list",
            "inline-autocomplete-thread",
        ),
    };
    let rows = autocomplete
        .candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let selected = index == autocomplete.selected_index;
            div()
                .id((row_id, index))
                .w_full()
                .px_2()
                .py_1()
                .rounded_sm()
                .cursor(CursorStyle::PointingHand)
                .bg(if selected {
                    tint(accent_soft(), if is_dark_theme() { 0.45 } else { 0.85 })
                } else {
                    tint(0x000000, 0.0)
                })
                .hover(|style| {
                    style.bg(tint(
                        accent_soft(),
                        if is_dark_theme() { 0.32 } else { 0.65 },
                    ))
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_autocomplete_index(target, index, window, cx);
                }))
                .child(render_candidate_row(candidate))
        })
        .collect::<Vec<_>>();

    div()
        .w_full()
        .rounded_xl()
        .border_1()
        .border_color(shell_border_strong())
        .bg(modal_surface())
        .shadow(card_shadow())
        .p_1()
        // Clicks and wheel scrolls stay in the list instead of reaching the
        // timeline painted underneath.
        .occlude()
        .child(
            div()
                .id(list_id)
                .flex()
                .flex_col()
                .gap_1()
                // About seven rows; the rest scroll.
                .max_h(typography::font_px(232.))
                .overflow_y_scroll()
                .track_scroll(scroll)
                .children(rows),
        )
        .into_any_element()
}

fn render_candidate_row(candidate: &AutocompleteCandidate) -> AnyElement {
    match candidate {
        AutocompleteCandidate::MentionUser {
            username,
            display_name,
            ..
        } => div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .text_size(typography::body())
                    .text_color(rgb(text_primary()))
                    .child(crate::domain::user::shown_name(username, display_name).to_string()),
            )
            .children(
                crate::domain::user::other_name(username, display_name).map(|other_name| {
                    div()
                        .text_size(typography::caption())
                        .text_color(rgb(text_secondary()))
                        .child(other_name)
                }),
            )
            .into_any_element(),
        AutocompleteCandidate::MentionBroadcast {
            keyword,
            description,
        } => div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .text_size(typography::body())
                    .text_color(rgb(text_primary()))
                    .child(format!("@{keyword}")),
            )
            .child(
                div()
                    .text_size(typography::caption())
                    .text_color(rgb(text_secondary()))
                    .child(description.clone()),
            )
            .into_any_element(),
        AutocompleteCandidate::Emoji {
            label,
            glyph,
            insert_text: _,
        } => div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_size(typography::large_body())
                    .text_color(rgb(text_primary()))
                    .child(glyph.clone().unwrap_or_else(|| "◻".to_string())),
            )
            .child(
                div()
                    .text_size(typography::body())
                    .text_color(rgb(text_primary()))
                    .child(label.clone()),
            )
            .into_any_element(),
    }
}
