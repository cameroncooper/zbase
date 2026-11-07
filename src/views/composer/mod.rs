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
        card_shadow, field_bg, field_border, focus_ring, icon_asset,
        input::TextField,
        is_dark_theme, modal_surface,
        popup_menu::{PopupMenuEntry, popup_menu},
        send_icon, shell_border_strong, text_primary, text_secondary, tint,
        tooltip::{TextTooltip, shortcut_label},
    },
};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, ClickEvent, Context, Corner, CursorStyle, Entity, FontWeight, Hsla,
    InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled, anchored,
    deferred, div, point, px, rgb,
};

#[derive(Default)]
pub struct ComposerPanel;

/// Which input a composer field drives; picks the handlers and element ids.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComposerTarget {
    Conversation,
    Thread,
}

/// Visible state of a composer field.
#[derive(Clone, Copy)]
pub(crate) struct ComposerFieldState {
    pub focused: bool,
    pub can_send: bool,
    /// While Keybase can't be reached: what the hint and send tooltip say.
    pub send_notice: Option<SendNotice>,
    pub format_menu_open: bool,
}

/// Bordered message field shared by the conversation and thread composers: an
/// 8px-radius box (40px tall at one line) whose border turns accent with a soft
/// ring on focus, and attach, emoji and send controls inside its right edge.
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
        .w_full()
        .min_h(px(40.))
        .rounded(px(8.))
        .border_1()
        .border_color(if state.focused {
            rgb(accent())
        } else {
            rgb(field_border())
        })
        .when(state.focused, |field| field.shadow(vec![focus_ring()]))
        .bg(fill)
        .text_sm()
        .line_height(px(22.))
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
        .into_any_element()
}

/// A 28px icon button inside the composer field's right edge.
fn composer_tool(id: &'static str, icon: AnyElement) -> gpui::Stateful<gpui::Div> {
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
                    .anchor(Corner::BottomLeft)
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
                    .text_size(px(13.))
                    .line_height(px(16.))
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

/// Space between the composer and the bottom of the window or pane.
pub(crate) const COMPOSER_BOTTOM_INSET_PX: f32 = 12.;

/// Show an offline notice only when needed; otherwise reserve no space below
/// the field. The send button's tooltip provides the keyboard shortcut.
pub(crate) fn composer_hint(state: &ComposerFieldState) -> Option<AnyElement> {
    if !state.focused {
        return None;
    }
    let text = state.send_notice.and_then(|notice| notice.composer_hint)?;
    Some(
        div()
            .flex_shrink_0()
            .pt(px(4.))
            // Line up with the text inside the field (1px border + 12px inset).
            .pl(px(13.))
            .text_size(px(11.))
            .line_height(px(14.))
            .text_color(rgb(text_secondary()))
            .child(text)
            .into_any_element(),
    )
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
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        let mode_banner = match &composer.mode {
            ComposerMode::Compose => None,
            ComposerMode::Edit { .. } => Some(
                div()
                    .pl_2()
                    .border_l_2()
                    .border_color(rgb(accent()))
                    .text_xs()
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
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(text_secondary()))
                    .child("Replying in thread")
                    .into_any_element(),
            ),
        };

        let hint = composer_hint(&state);
        div()
            .px_4()
            .pt_1()
            .pb(px(COMPOSER_BOTTOM_INSET_PX))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .when_some(mode_banner, |div, banner| div.mb_2().child(banner))
            .child(composer_field(
                ComposerTarget::Conversation,
                editor,
                state,
                rgb(field_bg()).into(),
                cx,
            ))
            .children(hint)
            .into_any_element()
    }
}

pub(crate) fn render_autocomplete_popup(
    autocomplete: &AutocompleteState,
    composer_target: bool,
    cx: &mut Context<AppWindow>,
) -> AnyElement {
    let rows = autocomplete
        .candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let selected = index == autocomplete.selected_index;
            let base = div()
                .id((
                    if composer_target {
                        "inline-autocomplete-composer"
                    } else {
                        "inline-autocomplete-thread"
                    },
                    index,
                ))
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
                });
            let clickable = if composer_target {
                base.on_click(cx.listener(move |this, _, _, cx| {
                    this.select_composer_autocomplete_index(index, cx);
                }))
            } else {
                base.on_click(cx.listener(move |this, _, _, cx| {
                    this.select_thread_autocomplete_index(index, cx);
                }))
            };
            clickable.child(render_candidate_row(candidate))
        })
        .collect::<Vec<_>>();

    div()
        .w_full()
        .rounded_xl()
        .border_1()
        .border_color(shell_border_strong())
        .bg(modal_surface())
        .shadow(card_shadow())
        .px_1()
        .py_1()
        .flex()
        .flex_col()
        .gap_1()
        .max_h(px(220.))
        .overflow_hidden()
        .children(rows)
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
                    .text_sm()
                    .text_color(rgb(text_primary()))
                    .child(display_name.clone()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(text_secondary()))
                    .child(format!("@{username}")),
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
                    .text_sm()
                    .text_color(rgb(text_primary()))
                    .child(format!("@{keyword}")),
            )
            .child(
                div()
                    .text_xs()
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
                    .text_base()
                    .text_color(rgb(text_primary()))
                    .child(glyph.clone().unwrap_or_else(|| "◻".to_string())),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(text_primary()))
                    .child(label.clone()),
            )
            .into_any_element(),
    }
}
