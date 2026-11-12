use crate::views::typography;
use crate::{
    domain::attachment::{AttachmentKind, AttachmentSource, AttachmentSummary},
    domain::route::Route,
    models::{
        emoji_picker_model::EmojiPickerModel,
        file_upload_model::FileUploadLightboxModel,
        new_chat_model::NewChatModel,
        notifications_model::NotificationsModel,
        overlay_model::{FullscreenImageOverlay, OverlayModel},
        profile_panel_model::ProfilePanelModel,
        quick_switcher_model::{QuickSwitcherModel, QuickSwitcherResult, QuickSwitcherResultKind},
    },
    state::state::EmojiRenderState,
    util::search_snippet::clean_search_snippet,
    views::{
        accent, accent_strong,
        app_window::AppWindow,
        attachment_display_label, badge, card_shadow, dm_icon, empty_state, hash_icon, icon_asset,
        image_source_from_attachment_source,
        input::TextField,
        is_dark_theme, keycap, modal_surface, panel_alt_bg, panel_surface,
        scaled_image::{Fit, scaled_for_box},
        search::{MessageHit, SWITCHER_HIT, SearchResultContext, snippet_text},
        section_label, shell_border, shell_border_strong, subtle_surface, text_primary,
        text_secondary, tint,
        tooltip::shortcut_label,
        truncated_line::{Segment, TruncatedLine},
    },
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, Context, Entity, FontWeight, InteractiveElement, IntoElement, ObjectFit,
    ParentElement, Pixels, SharedString, StatefulInteractiveElement, Styled, StyledImage, div, img,
    px, rgb,
};
use std::collections::HashMap;

mod emoji_picker;
mod new_chat;

/// Distance from the window top to every centered dialog (quick switcher, new
/// chat, command palette, upload preview), so moving between them never jumps.
pub(crate) const OVERLAY_TOP_PX: f32 = 96.;

const QUICK_SWITCHER_RENDER_LIMIT_UNREAD: usize = 14;
const QUICK_SWITCHER_RENDER_LIMIT_RECENT: usize = 14;
const QUICK_SWITCHER_RECENT_SHORTCUT_LIMIT: usize = 5;

#[derive(Default)]
pub struct OverlayHost;

impl OverlayHost {
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep the explicit inputs of this existing view API."
    )]
    pub fn render(
        &self,
        overlay: &OverlayModel,
        profile_panel: &ProfilePanelModel,
        _notifications: &NotificationsModel,
        quick_switcher: &QuickSwitcherModel,
        new_chat: &NewChatModel,
        quick_switcher_input: &Entity<TextField>,
        new_chat_input: &Entity<TextField>,
        new_chat_input_focused: bool,
        file_upload_caption_input: &Entity<TextField>,
        emoji_picker_state: &EmojiPickerModel,
        emoji_picker_input: &Entity<TextField>,
        custom_emoji_index: Option<&HashMap<String, EmojiRenderState>>,
        supports_custom_emoji: bool,
        quick_switcher_indexing_status: Option<&str>,
        quick_switcher_hits: &HashMap<usize, SearchResultContext>,
        conversation_avatars: &HashMap<String, crate::models::sidebar_model::SidebarAvatar>,
        viewport: gpui_kit::Size<Pixels>,
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        if overlay.active_modal.is_none()
            && !overlay.new_chat_open
            && !overlay.quick_switcher_open
            && !overlay.command_palette_open
            && !overlay.emoji_picker_open
            && overlay.fullscreen_image.is_none()
            && overlay.file_upload_lightbox.is_none()
            && overlay.profile_card_user_id.is_none()
        {
            return div().into_any_element();
        }

        let has_backdrop = overlay.new_chat_open
            || overlay.quick_switcher_open
            || overlay.command_palette_open
            || overlay.fullscreen_image.is_some()
            || overlay.file_upload_lightbox.is_some();

        div()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .flex_col()
            .items_center()
            .id("overlay-root")
            .on_scroll(cx.listener(AppWindow::consume_scroll_wheel))
            .on_mouse_move(cx.listener(AppWindow::consume_mouse_move))
            .when(has_backdrop, |d| {
                d.bg(tint(0x000000, if is_dark_theme() { 0.50 } else { 0.25 }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.dismiss_overlays(cx);
                    }))
            })
            .when(overlay.new_chat_open, |container| {
                container.child(new_chat::render_new_chat_modal(
                    new_chat,
                    new_chat_input,
                    new_chat_input_focused,
                    cx,
                ))
            })
            .when(overlay.quick_switcher_open, |container| {
                container.child(Self::render_search_overlay(
                    quick_switcher,
                    quick_switcher_input,
                    quick_switcher_indexing_status,
                    quick_switcher_hits,
                    conversation_avatars,
                    cx,
                ))
            })
            .when(overlay.command_palette_open, |container| {
                container.child(Self::render_command_palette(cx))
            })
            .when_some(overlay.fullscreen_image.as_ref(), |container, image| {
                container.child(Self::render_fullscreen_image_overlay(image, viewport, cx))
            })
            .when_some(
                overlay.file_upload_lightbox.as_ref(),
                |container, lightbox| {
                    container.child(Self::render_file_upload_lightbox(
                        lightbox,
                        file_upload_caption_input,
                        cx,
                    ))
                },
            )
            .when(!has_backdrop, |d| {
                d.justify_end().items_end().px_6().pb_4().child(
                    div()
                        .flex()
                        .gap_2()
                        .when_some(overlay.active_modal.as_ref(), |container, modal| {
                            container.child(
                                div()
                                    .rounded_lg()
                                    .bg(panel_surface())
                                    .px_3()
                                    .py_2()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(modal.clone())
                                    .child(
                                        div()
                                            .id("overlay-modal-close")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.dismiss_overlays(cx);
                                            }))
                                            .child(badge("Close", panel_alt_bg(), text_primary())),
                                    ),
                            )
                        })
                        .when(overlay.emoji_picker_open, |container| {
                            container.child(emoji_picker::render_emoji_picker(
                                overlay,
                                emoji_picker_state,
                                emoji_picker_input,
                                custom_emoji_index,
                                supports_custom_emoji,
                                cx,
                            ))
                        })
                        .when_some(
                            overlay.profile_card_user_id.as_ref(),
                            |container, user_id| {
                                container.child(crate::views::profile::render_profile_card(
                                    profile_panel,
                                    user_id,
                                    cx,
                                ))
                            },
                        ),
                )
            })
            .into_any_element()
    }

    fn render_fullscreen_image_overlay(
        image: &FullscreenImageOverlay,
        viewport: gpui_kit::Size<Pixels>,
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        let caption = image
            .caption
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let Some(image_source) = image_source_from_attachment_source(&image.source) else {
            return div()
                .id("overlay-fullscreen-image-unavailable")
                .on_mouse_down(
                    gpui_kit::MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.dismiss_overlays(cx);
                        cx.stop_propagation();
                    }),
                )
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .px_4()
                        .py_3()
                        .rounded_lg()
                        .bg(panel_surface())
                        .text_size(typography::body())
                        .text_color(rgb(text_primary()))
                        .id("lightbox-error-card")
                        .on_mouse_down(
                            gpui_kit::MouseButton::Left,
                            cx.listener(|_, _, _, cx| cx.stop_propagation()),
                        )
                        .child("Unable to load image preview."),
                )
                .into_any_element();
        };
        let (frame_width, frame_height) = fullscreen_image_frame_size(
            image.width,
            image.height,
            (f32::from(viewport.width) - 48.).clamp(1., 1120.),
            (f32::from(viewport.height) - if caption.is_some() { 120. } else { 48. })
                .clamp(1., 780.),
        );

        div()
            .id("overlay-fullscreen-image")
            .on_mouse_down(
                gpui_kit::MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.dismiss_overlays(cx);
                    cx.stop_propagation();
                }),
            )
            .size_full()
            .px_6()
            .py_6()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .w_full()
                            .min_h(px(0.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .id("lightbox-image-area")
                                    .w(px(frame_width))
                                    .h(px(frame_height))
                                    .flex_shrink_0()
                                    .on_mouse_down(
                                        gpui_kit::MouseButton::Left,
                                        cx.listener(|_, _, _, cx| cx.stop_propagation()),
                                    )
                                    .child(
                                        img(scaled_for_box(
                                            image_source,
                                            1120.,
                                            780.,
                                            Fit::Contain,
                                        ))
                                        .id("lightbox-image")
                                        .size_full()
                                        .object_fit(ObjectFit::Contain)
                                        .with_fallback(
                                            || {
                                                div()
                                                    .size_full()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .text_size(typography::body())
                                                    .text_color(rgb(text_secondary()))
                                                    .child("Image unavailable")
                                                    .into_any_element()
                                            },
                                        ),
                                    ),
                            ),
                    )
                    .when_some(caption, |container, caption| {
                        container.child(
                            div()
                                .id("lightbox-caption")
                                .on_mouse_down(
                                    gpui_kit::MouseButton::Left,
                                    cx.listener(|_, _, _, cx| cx.stop_propagation()),
                                )
                                .max_w(px(900.))
                                .text_size(typography::body())
                                .text_color(rgb(text_primary()))
                                .child(caption),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_file_upload_lightbox(
        lightbox: &FileUploadLightboxModel,
        file_upload_caption_input: &Entity<TextField>,
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        let total = lightbox.candidates.len();
        let index = lightbox.current_index.min(total.saturating_sub(1));
        let Some(candidate) = lightbox.candidates.get(index) else {
            return div()
                .mt(px(OVERLAY_TOP_PX))
                .w(px(420.))
                .rounded_xl()
                .border_1()
                .border_color(shell_border_strong())
                .bg(modal_surface())
                .shadow(card_shadow())
                .px_4()
                .py_3()
                .text_size(typography::body())
                .text_color(rgb(text_primary()))
                .child("No files selected.")
                .into_any_element();
        };

        let source_text = candidate.path.to_string_lossy().to_string();
        let image_source = AttachmentSource::LocalPath(source_text.clone());
        let attachment_label = attachment_display_label(&AttachmentSummary {
            name: candidate.filename.clone(),
            kind: candidate.kind.clone(),
            size_bytes: candidate.size_bytes,
            ..AttachmentSummary::default()
        });
        let is_last = index + 1 >= total;
        let counter = format!("{} of {}", index + 1, total);

        let preview = match candidate.kind {
            AttachmentKind::Image => {
                if let Some(preview_source) = image_source_from_attachment_source(&image_source) {
                    let (frame_width, frame_height) = file_upload_preview_frame_size(
                        candidate.width,
                        candidate.height,
                        480.0,
                        280.0,
                    );
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            img(scaled_for_box(
                                preview_source,
                                frame_width,
                                frame_height,
                                Fit::Contain,
                            ))
                            .w(px(frame_width))
                            .h(px(frame_height))
                            .object_fit(ObjectFit::Contain)
                            .with_fallback(|| {
                                div()
                                    .size_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(typography::body())
                                    .text_color(rgb(text_secondary()))
                                    .child("Image unavailable")
                                    .into_any_element()
                            }),
                        )
                        .into_any_element()
                } else {
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(typography::body())
                        .text_color(rgb(text_secondary()))
                        .child("Image unavailable")
                        .into_any_element()
                }
            }
            AttachmentKind::Video => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(typography::body())
                .text_color(rgb(text_secondary()))
                .child("Video preview coming soon")
                .into_any_element(),
            AttachmentKind::Audio => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(typography::body())
                .text_color(rgb(text_secondary()))
                .child("Audio file selected")
                .into_any_element(),
            AttachmentKind::File => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(typography::body())
                .text_color(rgb(text_secondary()))
                .child("File selected")
                .into_any_element(),
        };

        div()
            .mt(px(OVERLAY_TOP_PX))
            .w(px(520.))
            .max_h(px(620.))
            .rounded_xl()
            .border_1()
            .border_color(shell_border_strong())
            .bg(modal_surface())
            .shadow(card_shadow())
            .flex()
            .flex_col()
            .overflow_hidden()
            .id("overlay-file-upload-lightbox")
            .on_click(cx.listener(|_, _, _, cx| {
                cx.stop_propagation();
            }))
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(shell_border())
                    .text_size(typography::body())
                    .text_color(rgb(text_primary()))
                    .child("Upload files"),
            )
            .child(
                div()
                    .px_4()
                    .py_3()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .h(px(280.))
                            .rounded_lg()
                            .bg(panel_surface())
                            .border_1()
                            .border_color(shell_border())
                            .overflow_hidden()
                            .child(preview),
                    )
                    .child(
                        div()
                            .text_size(typography::body())
                            .text_color(rgb(text_primary()))
                            .child(candidate.filename.clone()),
                    )
                    .child(
                        div()
                            .text_size(typography::caption())
                            .text_color(rgb(text_secondary()))
                            .child(format!("{counter} · {attachment_label}")),
                    )
                    .child(
                        div()
                            .id("file-upload-caption-input")
                            .rounded_md()
                            .bg(panel_surface())
                            .border_1()
                            .border_color(shell_border())
                            .px_3()
                            .py_2()
                            .text_size(typography::body())
                            .text_color(rgb(text_primary()))
                            .on_click(cx.listener(AppWindow::focus_file_upload_caption_input))
                            .child(file_upload_caption_input.clone()),
                    ),
            )
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(shell_border())
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .child(
                        div()
                            .id("file-upload-cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.file_upload_cancel(cx);
                            }))
                            .child(badge("Cancel", panel_alt_bg(), text_primary())),
                    )
                    .when(!is_last, |container| {
                        container.child(
                            div()
                                .id("file-upload-next")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.file_upload_next(cx);
                                }))
                                .child(badge("Next", panel_alt_bg(), text_primary())),
                        )
                    })
                    .when(is_last || total == 1, |container| {
                        container.child(
                            div()
                                .id("file-upload-send")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if total > 1 {
                                        this.file_upload_send_all(window, cx);
                                    } else {
                                        this.file_upload_send_current(window, cx);
                                    }
                                }))
                                .child(badge("Send", accent(), panel_alt_bg())),
                        )
                    })
                    .when(total > 1, |container| {
                        container.child(
                            div()
                                .id("file-upload-send-all")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.file_upload_send_all(window, cx);
                                }))
                                .child(badge("Send All", panel_alt_bg(), text_primary())),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_search_overlay(
        quick_switcher: &QuickSwitcherModel,
        quick_switcher_input: &Entity<TextField>,
        quick_switcher_indexing_status: Option<&str>,
        hits: &HashMap<usize, SearchResultContext>,
        conversation_avatars: &HashMap<String, crate::models::sidebar_model::SidebarAvatar>,
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        let query_empty = quick_switcher.query.trim().is_empty();
        let mut unread_indices = Vec::new();
        let mut recent_indices = Vec::new();
        let sections = quick_switcher.search_sections();
        for (index, result) in quick_switcher.results.iter().enumerate() {
            if query_empty {
                if result.kind == QuickSwitcherResultKind::UnreadChannel
                    && unread_indices.len() < QUICK_SWITCHER_RENDER_LIMIT_UNREAD
                {
                    unread_indices.push(index);
                } else if result.kind != QuickSwitcherResultKind::UnreadChannel
                    && recent_indices.len() < QUICK_SWITCHER_RENDER_LIMIT_RECENT
                {
                    recent_indices.push(index);
                }

                if unread_indices.len() >= QUICK_SWITCHER_RENDER_LIMIT_UNREAD
                    && recent_indices.len() >= QUICK_SWITCHER_RENDER_LIMIT_RECENT
                {
                    break;
                }
            }
        }

        let mut results = div().px_2().py_2().flex().flex_col().gap(px(10.));
        if query_empty {
            if !unread_indices.is_empty() {
                results = results.child(Self::quick_switcher_section(
                    "Unread",
                    &unread_indices,
                    quick_switcher,
                    false,
                    hits,
                    conversation_avatars,
                    cx,
                ));
            }
            if !recent_indices.is_empty() {
                results = results.child(Self::quick_switcher_section(
                    "Recent",
                    &recent_indices,
                    quick_switcher,
                    true,
                    hits,
                    conversation_avatars,
                    cx,
                ));
            }
        } else {
            if !sections.strong.is_empty() {
                results = results.child(Self::quick_switcher_section(
                    "Channels and people",
                    &sections.strong,
                    quick_switcher,
                    false,
                    hits,
                    conversation_avatars,
                    cx,
                ));
            }
            if !sections.messages.is_empty() {
                results = results.child(Self::quick_switcher_section(
                    "Messages",
                    &sections.messages,
                    quick_switcher,
                    false,
                    hits,
                    conversation_avatars,
                    cx,
                ));
            }
            if !sections.weak.is_empty() {
                results = results.child(Self::quick_switcher_section(
                    "Other channels and people",
                    &sections.weak,
                    quick_switcher,
                    false,
                    hits,
                    conversation_avatars,
                    cx,
                ));
            }
        }

        if quick_switcher.results.is_empty() && !quick_switcher.loading_messages {
            results = results.child(if query_empty {
                empty_state(
                    "assets/icons/dm.svg",
                    "No conversations yet",
                    Some("Channels and people you talk to will show up here.".to_string()),
                )
            } else {
                empty_state(
                    "assets/icons/search.svg",
                    format!("No matches for “{}”", quick_switcher.query.trim()),
                    Some("Try a channel, a person, or words from a message.".to_string()),
                )
            });
        }
        let status_line = |text: String| {
            div()
                .px_2()
                .text_size(typography::font_px(12.))
                .text_color(rgb(text_secondary()))
                .child(text)
        };
        if quick_switcher.loading_messages && !query_empty {
            results = results.child(status_line("Searching messages…".to_string()));
        }
        if let Some(indexing_status) = quick_switcher_indexing_status
            && !query_empty
        {
            results = results.child(status_line(indexing_status.to_string()));
        }

        let hint = |keys: Vec<&'static str>, label: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .children(keys.into_iter().map(|key| keycap(shortcut_label(key))))
                .child(div().pl(px(2.)).child(label))
        };

        div()
            .mt(px(OVERLAY_TOP_PX))
            .w(px(480.))
            .max_h(px(560.))
            .rounded_xl()
            .border_1()
            .border_color(shell_border_strong())
            .bg(modal_surface())
            .shadow(card_shadow())
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .min_h(typography::font_px(48.))
                    .flex_shrink_0()
                    .px_4()
                    .flex()
                    .items_center()
                    .gap_3()
                    .border_b_1()
                    .border_color(shell_border())
                    .child(icon_asset("assets/icons/search.svg", 16., text_secondary()))
                    .child(
                        div()
                            .flex_1()
                            .text_size(typography::font_px(15.))
                            .text_color(rgb(text_primary()))
                            .id("quick-switcher-overlay-input")
                            .on_click(cx.listener(AppWindow::focus_quick_switcher_input))
                            .child(quick_switcher_input.clone()),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .id("quick-switcher-results-scroll")
                    .overflow_y_scroll()
                    .on_scroll(cx.listener(AppWindow::consume_scroll_wheel))
                    .scrollbar_width(px(8.))
                    .child(results),
            )
            .child(
                div()
                    .min_h(typography::font_px(36.))
                    .py_1()
                    .flex_shrink_0()
                    .px_4()
                    .border_t_1()
                    .border_color(shell_border())
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(16.))
                    .text_size(typography::font_px(12.))
                    .text_color(rgb(text_secondary()))
                    .child(hint(vec!["↑", "↓"], "Navigate"))
                    .child(hint(vec!["↵"], "Open"))
                    .child(hint(vec!["esc"], "Close"))
                    .child(div().flex_1())
                    .child(hint(vec!["⌘K"], "Toggle")),
            )
            .into_any_element()
    }

    fn quick_switcher_section(
        title: &str,
        indices: &[usize],
        quick_switcher: &QuickSwitcherModel,
        show_recent_shortcuts: bool,
        hits: &HashMap<usize, SearchResultContext>,
        conversation_avatars: &HashMap<String, crate::models::sidebar_model::SidebarAvatar>,
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(section_label(title.to_string()).px_2().pb(px(4.)))
            .children(
                indices
                    .iter()
                    .enumerate()
                    .filter_map(|(display_index, index)| {
                        quick_switcher.results.get(*index).map(|result| {
                            let shortcut_hint = (show_recent_shortcuts
                                && display_index < QUICK_SWITCHER_RECENT_SHORTCUT_LIMIT)
                                .then(|| shortcut_label(&format!("⌘{}", display_index + 1)));
                            let selected = quick_switcher.selected_index == *index;
                            match hits.get(index) {
                                Some(context) => Self::quick_switcher_message_row(
                                    *index, result, context, selected, cx,
                                ),
                                None => Self::quick_switcher_row(
                                    *index,
                                    result,
                                    selected,
                                    shortcut_hint,
                                    conversation_avatars,
                                    cx,
                                ),
                            }
                        })
                    }),
            )
            .into_any_element()
    }

    /// Conversation rows are one 36px line: icon, name, team. Message rows add
    /// the snippet underneath. The selected row is a grey fill with the normal
    /// text colors, like the sidebar's selection and pop-up menus.
    fn quick_switcher_row(
        index: usize,
        result: &QuickSwitcherResult,
        selected: bool,
        shortcut_hint: Option<String>,
        conversation_avatars: &HashMap<String, crate::models::sidebar_model::SidebarAvatar>,
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        let row_surface = if selected {
            switcher_selection_bg()
        } else {
            modal_surface()
        };
        let primary_color: gpui_kit::Hsla = rgb(text_primary()).into();
        let secondary_color: gpui_kit::Hsla = rgb(text_secondary()).into();
        let match_color: gpui_kit::Hsla = rgb(accent_strong()).into();
        let is_message = result.kind == QuickSwitcherResultKind::Message;
        let is_dm = matches!(result.route, Route::DirectMessage { .. });
        let unread = result.kind == QuickSwitcherResultKind::UnreadChannel;
        let base_weight = if unread {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        };

        // A single-channel team is labeled by its team alone ("bookclub").
        let simple_team = !is_message && !is_dm && is_simple_team_label(&result.label);
        let (primary, team) = if is_message || is_dm || simple_team {
            let ranges = if is_message {
                Vec::new()
            } else {
                result.match_ranges.clone()
            };
            ((result.label.clone(), ranges), None)
        } else {
            split_channel_label(&result.label, &result.match_ranges)
        };
        let mut segments = highlight_segments(
            &primary.0,
            &primary.1,
            primary_color,
            match_color,
            if is_message {
                FontWeight::MEDIUM
            } else {
                base_weight
            },
        );
        if simple_team {
            // Its one channel, as a quiet descriptor.
            segments.push(Segment {
                text: "  #general".to_string(),
                color: secondary_color,
                weight: FontWeight::NORMAL,
            });
        }
        if let Some((team_name, team_ranges)) = team.as_ref() {
            segments.push(Segment {
                text: "  ".to_string(),
                color: secondary_color,
                weight: FontWeight::NORMAL,
            });
            segments.extend(highlight_segments(
                team_name,
                team_ranges,
                secondary_color,
                match_color,
                FontWeight::NORMAL,
            ));
        }
        let group_username_match = result
            .username_match
            .as_ref()
            .filter(|matched| matched.group);
        if let Some(username_match) = result
            .username_match
            .as_ref()
            .filter(|matched| !matched.group)
        {
            segments.push(Segment {
                text: "  ".to_string(),
                color: secondary_color,
                weight: FontWeight::NORMAL,
            });
            segments.extend(highlight_segments(
                &username_match.text,
                &username_match.ranges,
                secondary_color,
                match_color,
                FontWeight::NORMAL,
            ));
        }
        let expanded = is_message || group_username_match.is_some();

        let icon = Self::quick_switcher_icon(
            result,
            is_dm,
            team.as_ref()
                .map(|(name, _)| name.as_str())
                .or(simple_team.then_some(result.label.as_str())),
            conversation_avatars,
            row_surface,
            text_secondary(),
        );
        let snippet = is_message.then(|| {
            clean_search_snippet(
                result.sublabel.as_deref().unwrap_or_default(),
                &result.match_ranges,
            )
        });

        div()
            .id(SharedString::from(format!("quick-switcher-result-{index}")))
            .w_full()
            .min_h(px(36.))
            .px_2()
            .when(expanded, |row| row.py(px(6.)))
            .rounded(px(6.))
            .flex()
            .when(expanded, |row| row.items_start())
            .when(!expanded, |row| row.items_center())
            .gap(px(10.))
            .text_size(typography::font_px(14.))
            .bg(row_surface)
            .when(!selected, |row| {
                row.hover(|style| style.bg(tint(text_primary(), 0.06)))
            })
            .cursor(gpui_kit::CursorStyle::PointingHand)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_quick_switcher_result_at(index, window, cx);
            }))
            .child(
                div()
                    .size(px(20.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(expanded, |icon| icon.mt(px(1.)))
                    .child(icon),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .w_full()
                            .when(is_message, |line| line.text_size(typography::font_px(12.)))
                            .line_height(typography::font_px(if is_message { 16. } else { 20. }))
                            .child(TruncatedLine::new(segments)),
                    )
                    .when_some(group_username_match, |column, username_match| {
                        let mut segments = vec![Segment {
                            text: "Matched ".to_string(),
                            color: secondary_color,
                            weight: FontWeight::NORMAL,
                        }];
                        segments.extend(highlight_segments(
                            &username_match.text,
                            &username_match.ranges,
                            secondary_color,
                            match_color,
                            FontWeight::NORMAL,
                        ));
                        column.child(
                            div()
                                .w_full()
                                .text_size(px(12.))
                                .line_height(px(16.))
                                .child(TruncatedLine::new(segments)),
                        )
                    })
                    .when_some(snippet, |column, snippet| {
                        column.child(snippet_text(&snippet, SWITCHER_HIT, false))
                    }),
            )
            .when_some(shortcut_hint, |row, hint| row.child(keycap(hint)))
            .into_any_element()
    }

    /// A message hit, drawn with the Search result anatomy: author avatar,
    /// author · location · time, then a two-line snippet.
    fn quick_switcher_message_row(
        index: usize,
        result: &QuickSwitcherResult,
        context: &SearchResultContext,
        selected: bool,
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        let snippet = clean_search_snippet(
            result.sublabel.as_deref().unwrap_or_default(),
            &result.match_ranges,
        );
        let hit = MessageHit {
            id: SharedString::from(format!("quick-switcher-result-{index}")),
            author: &context.author_name,
            author_avatar: context.author_avatar.as_deref(),
            location: context.location.as_deref(),
            timestamp_ms: result.timestamp_ms,
            snippet: &snippet,
        };
        div()
            .id(hit.id.clone())
            .w_full()
            .px_2()
            .py(px(6.))
            .rounded(px(6.))
            .flex()
            .items_start()
            .gap(px(10.))
            .when(selected, |row| row.bg(switcher_selection_bg()))
            .when(!selected, |row| {
                row.hover(|style| style.bg(tint(text_primary(), 0.06)))
            })
            .cursor(gpui_kit::CursorStyle::PointingHand)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_quick_switcher_result_at(index, window, cx);
            }))
            .child(hit.avatar(SWITCHER_HIT))
            .child(hit.column(SWITCHER_HIT, false))
            .into_any_element()
    }

    /// DMs lead with the person's face and presence, channels with their team
    /// mark, and channel messages (or channels without a team) with `#`.
    fn quick_switcher_icon(
        result: &QuickSwitcherResult,
        is_dm: bool,
        team_name: Option<&str>,
        conversation_avatars: &HashMap<String, crate::models::sidebar_model::SidebarAvatar>,
        surface: gpui_kit::Hsla,
        glyph_color: u32,
    ) -> AnyElement {
        use crate::models::sidebar_model::{SidebarAvatar, team_avatar_key};
        if is_dm {
            return match conversation_avatars
                .get(&result.route.label())
                .map(SidebarAvatar::participants)
                .filter(|participants| !participants.is_empty())
            {
                Some(participants) => crate::views::avatar::Avatar::render_participants_on(
                    participants,
                    &result.label,
                    20.,
                    surface,
                ),
                None => dm_icon(glyph_color),
            };
        }
        match team_name {
            Some(team) => {
                let asset = result
                    .sublabel
                    .as_deref()
                    .and_then(|team_id| conversation_avatars.get(&team_avatar_key(team_id)))
                    .and_then(|avatar| match avatar {
                        SidebarAvatar::Team(asset) => Some(asset.as_str()),
                        SidebarAvatar::Participants(_) => None,
                    });
                crate::views::avatar::Avatar::render_team(team, asset, 20.)
            }
            None => hash_icon(glyph_color),
        }
    }

    fn render_command_palette(cx: &mut Context<AppWindow>) -> AnyElement {
        div()
            .mt(px(OVERLAY_TOP_PX))
            .w(px(400.))
            .rounded_xl()
            .border_1()
            .border_color(shell_border_strong())
            .bg(modal_surface())
            .shadow(card_shadow())
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(shell_border())
                    .text_size(typography::caption())
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(text_secondary()))
                    .child("Commands"),
            )
            .child(
                div()
                    .px_4()
                    .py_3()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(Self::command_palette_row(
                        "overlay-command-new-chat",
                        "Start new chat",
                        "⌘N",
                        "new-chat",
                        cx,
                    ))
                    .child(Self::command_palette_row(
                        "overlay-command-search",
                        "Search messages",
                        "⌘J",
                        "search",
                        cx,
                    ))
                    .child(Self::command_palette_row(
                        "overlay-command-thread",
                        "Toggle thread pane",
                        "⌘⇧\\",
                        "thread",
                        cx,
                    ))
                    .child(Self::command_palette_row(
                        "overlay-command-prefs",
                        "Open settings",
                        "⌘,",
                        "preferences",
                        cx,
                    ))
                    .child(Self::command_palette_row(
                        "overlay-command-activity",
                        "Show activity",
                        "⌘⇧A",
                        "activity",
                        cx,
                    ))
                    .child(Self::command_palette_row(
                        "overlay-command-mark-all-read",
                        "Mark all as read",
                        "",
                        "mark-all-read",
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn command_palette_row(
        id: &'static str,
        label: &'static str,
        shortcut: &'static str,
        action: &'static str,
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        div()
            .id(SharedString::from(id))
            .w_full()
            .px_2()
            .py_1p5()
            .rounded_md()
            .flex()
            .items_center()
            .justify_between()
            .text_size(typography::body())
            .text_color(rgb(text_primary()))
            .hover(|s| s.bg(subtle_surface()))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.run_palette_action(action, window, cx);
            }))
            .child(label)
            .child(
                div()
                    .text_size(typography::caption())
                    .text_color(rgb(text_secondary()))
                    .child(shortcut_label(shortcut)),
            )
            .into_any_element()
    }
}

fn fullscreen_image_frame_size(
    width: Option<u32>,
    height: Option<u32>,
    max_width: f32,
    max_height: f32,
) -> (f32, f32) {
    let width = width.filter(|width| *width > 0).unwrap_or(1600) as f32;
    let height = height.filter(|height| *height > 0).unwrap_or(1000) as f32;
    let scale = (max_width / width).min(max_height / height).min(1.0);
    (width * scale, height * scale)
}

fn file_upload_preview_frame_size(
    width: Option<u32>,
    height: Option<u32>,
    max_width: f32,
    max_height: f32,
) -> (f32, f32) {
    let mut width = width.unwrap_or(900) as f32;
    let mut height = height.unwrap_or(600) as f32;
    if width <= 1.0 || height <= 1.0 {
        return (max_width.min(420.0), max_height.min(240.0));
    }
    let scale = (max_width / width).min(max_height / height).min(1.0);
    width *= scale;
    height *= scale;
    (width.max(60.0), height.max(60.0))
}

/// A channel label without "#": a single-channel team named by its team
/// ("bookclub"), whose channel is `#general`.
fn is_simple_team_label(label: &str) -> bool {
    !label.starts_with('#') && !label.contains(" #")
}

/// Splits a switcher channel label "lighthouse #general" into the channel
/// ("#general") and its team ("lighthouse"), moving match ranges with each part.
#[allow(clippy::type_complexity)]
fn split_channel_label(
    label: &str,
    ranges: &[(usize, usize)],
) -> (
    (String, Vec<(usize, usize)>),
    Option<(String, Vec<(usize, usize)>)>,
) {
    let Some(split) = label.find(" #") else {
        return ((label.to_string(), ranges.to_vec()), None);
    };
    let channel_start = split + 1;
    let channel = label[channel_start..].to_string();
    let channel_ranges = ranges
        .iter()
        .filter_map(|&(start, end)| {
            let start = start.max(channel_start);
            (start < end).then(|| (start - channel_start, end - channel_start))
        })
        .collect();
    let team_ranges = ranges
        .iter()
        .filter_map(|&(start, end)| {
            let end = end.min(split);
            (start < end).then_some((start, end))
        })
        .collect();
    (
        (channel, channel_ranges),
        Some((label[..split].to_string(), team_ranges)),
    )
}

/// Text split into plain and matched segments; matches are semibold in
/// `match_color`. Ranges off char boundaries are ignored.
fn highlight_segments(
    text: &str,
    ranges: &[(usize, usize)],
    color: gpui_kit::Hsla,
    match_color: gpui_kit::Hsla,
    weight: FontWeight,
) -> Vec<Segment> {
    let mut ranges: Vec<(usize, usize)> = ranges
        .iter()
        .copied()
        .filter(|&(start, end)| {
            start < end
                && end <= text.len()
                && text.is_char_boundary(start)
                && text.is_char_boundary(end)
        })
        .collect();
    ranges.sort_unstable();
    let mut segments = Vec::new();
    let mut cursor = 0;
    for (start, end) in ranges {
        let start = start.max(cursor);
        if start >= end {
            continue;
        }
        if start > cursor {
            segments.push(Segment {
                text: text[cursor..start].to_string(),
                color,
                weight,
            });
        }
        segments.push(Segment {
            text: text[start..end].to_string(),
            color: match_color,
            weight: FontWeight::SEMIBOLD,
        });
        cursor = end;
    }
    if cursor < text.len() {
        segments.push(Segment {
            text: text[cursor..].to_string(),
            color,
            weight,
        });
    }
    segments
}

#[cfg(test)]
mod quick_switcher_label_tests {
    use super::split_channel_label;

    #[test]
    fn channel_labels_split_into_channel_and_team_with_ranges_rebased() {
        // "lighthouse #general" with "gen" and "light" matched.
        let ((channel, channel_ranges), team) =
            split_channel_label("lighthouse #general", &[(12, 15), (0, 5)]);
        assert_eq!(channel, "#general");
        assert_eq!(channel_ranges, [(1, 4)]);
        let (team, team_ranges) = team.expect("team");
        assert_eq!(team, "lighthouse");
        assert_eq!(team_ranges, [(0, 5)]);
    }

    #[test]
    fn a_match_across_the_separator_is_split_between_the_parts() {
        let ((channel, channel_ranges), team) =
            split_channel_label("lighthouse #general", &[(8, 13)]);
        assert_eq!(&channel[channel_ranges[0].0..channel_ranges[0].1], "#g");
        let (team, team_ranges) = team.expect("team");
        assert_eq!(&team[team_ranges[0].0..team_ranges[0].1], "se");
    }

    #[test]
    fn single_channel_teams_are_named_by_their_team() {
        assert!(super::is_simple_team_label("bookclub"));
        assert!(!super::is_simple_team_label("#general"));
        assert!(!super::is_simple_team_label("lighthouse #general"));
    }

    #[test]
    fn labels_without_a_team_are_kept_whole() {
        let ((label, ranges), team) = split_channel_label("#general", &[(1, 3)]);
        assert_eq!(label, "#general");
        assert_eq!(ranges, [(1, 3)]);
        assert!(team.is_none());
    }
}

#[cfg(test)]
mod image_viewer_tests {
    use super::fullscreen_image_frame_size;

    #[test]
    fn image_hit_area_preserves_aspect_ratio_and_fits_compact_windows() {
        for (width, height) in [(4000, 2000), (2000, 4000), (20, 2000), (20, 10)] {
            let (rendered_width, rendered_height) =
                fullscreen_image_frame_size(Some(width), Some(height), 732., 600.);
            assert!(rendered_width <= 732. && rendered_height <= 600.);
            assert!(rendered_width <= width as f32 && rendered_height <= height as f32);
            assert!(
                (rendered_width / rendered_height - width as f32 / height as f32).abs() < 0.001
            );
        }
    }
}

/// The quick switcher's keyboard selection: a grey a step stronger than a
/// row's hover, kept opaque because avatar rings are cut from it.
fn switcher_selection_bg() -> gpui_kit::Hsla {
    modal_surface().blend(tint(text_primary(), 0.10))
}
