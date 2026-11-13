use crate::views::typography;
use crate::{
    domain::{
        message::LinkPreview,
        search::{SearchFilter, SearchResult},
    },
    models::search_model::SearchModel,
    util::{
        formatting::{compact_date_label, message_full_timestamp},
        search_snippet::{CleanSnippet, MentionKind, SnippetRun, clean_search_snippet},
    },
    views::{
        accent_strong,
        app_window::{AppWindow, video_preview_cache_key},
        attachment_file_row,
        avatar::{Avatar, default_avatar_background},
        border, empty_state, field_border, icon_asset,
        input::TextField,
        keycap, link, mention, page_header, panel_alt_surface, search_field_shell, search_match_bg,
        search_match_fg, sidebar_selected_bg, sidebar_selected_fg, sidebar_selected_secondary,
        text_primary, text_secondary, tint,
        tooltip::TextTooltip,
        warning,
    },
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, Context, Entity, FontWeight, HighlightStyle, Hsla, ImageSource, InteractiveElement,
    IntoElement, ObjectFit, ParentElement, RenderImage, SharedString, StatefulInteractiveElement,
    Styled, StyledImage, StyledText, div, img, px, rgb,
};
use std::{collections::HashMap, sync::Arc};

/// Who wrote a result and where it lives, resolved from profiles and the inbox
/// so rows never fall back to raw conversation ids.
#[derive(Clone, Debug, Default)]
pub struct SearchResultContext {
    pub author_name: String,
    pub author_avatar: Option<String>,
    pub location: Option<String>,
}

#[derive(Default)]
pub struct SearchView;

impl SearchView {
    pub fn render(
        &self,
        models: &crate::models::AppModels,
        contexts: &[SearchResultContext],
        input_focused: bool,
        video_render_cache: &HashMap<String, Arc<RenderImage>>,
        search_input: &Entity<TextField>,
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        let search = &models.search;
        let can_go_back = !models.navigation.back_stack.is_empty();
        let query = search.query.trim();
        let submitted = search.submitted_query.as_deref().map(str::trim);
        let status = if search.is_loading {
            Some("Searching…".to_string())
        } else if let Some(submitted) = submitted.filter(|_| !search.results.is_empty()) {
            let count = search.results.len();
            let noun = if count == 1 { "result" } else { "results" };
            Some(if submitted.is_empty() {
                format!("{count} {noun}")
            } else {
                format!("{count} {noun} for “{submitted}”")
            })
        } else {
            None
        };

        let controls = div()
            .flex_shrink_0()
            .px(px(24.))
            .pt(px(16.))
            .pb(px(12.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                search_field_shell(
                    "search-page-input",
                    search_input.clone().into_any_element(),
                    input_focused,
                    36.,
                )
                .on_click(cx.listener(AppWindow::focus_search_input)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(Self::filter_chip(
                        "Has file",
                        "assets/icons/paperclip.svg",
                        search.filters.contains(&SearchFilter::HasFile),
                        SearchFilter::HasFile,
                        cx,
                    ))
                    .child(Self::filter_chip(
                        "Mentions me",
                        "assets/icons/at-sign.svg",
                        search.filters.contains(&SearchFilter::MentionsMe),
                        SearchFilter::MentionsMe,
                        cx,
                    ))
                    .child(div().flex_1())
                    .when_some(status, |row, status| {
                        row.child(
                            div()
                                .min_w(px(0.))
                                .truncate()
                                .text_size(typography::font_px(12.))
                                .text_color(rgb(text_secondary()))
                                .child(status),
                        )
                    }),
            );

        let body = if search.results.is_empty() {
            Self::empty_body(search, query, submitted)
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .children(search.results.iter().enumerate().map(|(index, result)| {
                    search_result_row(
                        index,
                        result,
                        contexts.get(index),
                        search.highlighted_index == Some(index),
                        Some(video_render_cache),
                        cx,
                    )
                }))
                .into_any_element()
        };

        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(page_header(
                "Search",
                can_go_back.then(|| {
                    crate::views::back_button("search-back")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.navigate_back(window, cx);
                        }))
                        .into_any_element()
                }),
                None,
            ))
            .child(controls)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .id("search-results-scroll")
                    .overflow_y_scroll()
                    .scrollbar_width(px(8.))
                    // Result bounds share the field's 24px inset on both sides;
                    // the scrollbar gutter supplies 8px of the right one.
                    .pl(px(24.))
                    .pr(px(16.))
                    .pb(px(16.))
                    .child(body),
            )
            .into_any_element()
    }

    fn empty_body(search: &SearchModel, query: &str, submitted: Option<&str>) -> AnyElement {
        if search.is_loading {
            return div().into_any_element();
        }
        let state = match submitted {
            Some(submitted) if submitted == query => {
                let title = if submitted.is_empty() {
                    "No matching messages".to_string()
                } else {
                    format!("No results for “{submitted}”")
                };
                let body = if search.filters.is_empty() {
                    "Check the spelling or try different words."
                } else {
                    "Try removing a filter or searching for different words."
                };
                empty_state("assets/icons/search.svg", title, Some(body.to_string()))
            }
            _ if !query.is_empty() => empty_state(
                "assets/icons/search.svg",
                format!("Search for “{query}”"),
                None,
            )
            .child(
                div()
                    .pt(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(typography::label())
                    .text_color(rgb(text_secondary()))
                    .child("Press")
                    .child(keycap("↵"))
                    .child("to search every conversation"),
            ),
            _ => empty_state(
                "assets/icons/search.svg",
                "Search your messages",
                Some(
                    "Find messages, people and files across every conversation you're in."
                        .to_string(),
                ),
            ),
        };
        div().pt(px(24.)).child(state).into_any_element()
    }

    fn filter_chip(
        label: &'static str,
        icon: &'static str,
        active: bool,
        filter: SearchFilter,
        cx: &mut Context<AppWindow>,
    ) -> AnyElement {
        let foreground = if active { 0xffffff } else { text_primary() };
        div()
            .id(SharedString::from(format!("search-filter-{label}")))
            .min_h(typography::font_px(26.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded_full()
            .border_1()
            .border_color(rgb(if active {
                accent_strong()
            } else {
                field_border()
            }))
            .when(active, |chip| chip.bg(rgb(accent_strong())))
            .when(!active, |chip| {
                chip.hover(|style| style.bg(tint(text_primary(), 0.06)))
            })
            .cursor(gpui_kit::CursorStyle::PointingHand)
            .text_size(typography::font_px(12.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(foreground))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_search_filter_click(filter.clone(), cx);
            }))
            .child(icon_asset(
                icon,
                12.,
                if active { foreground } else { text_secondary() },
            ))
            .child(label)
            .into_any_element()
    }
}

/// Sizes of one message-hit presentation. Search and the quick switcher share
/// the anatomy (avatar, author · location · time, snippet) at different scales.
#[derive(Clone, Copy)]
pub(crate) struct MessageHitScale {
    pub avatar: f32,
    pub snippet_size: f32,
    pub snippet_line_height: f32,
    pub snippet_lines: usize,
}

pub(crate) const SEARCH_PAGE_HIT: MessageHitScale = MessageHitScale {
    avatar: 24.,
    snippet_size: 14.,
    snippet_line_height: 21.,
    snippet_lines: 3,
};

pub(crate) const SWITCHER_HIT: MessageHitScale = MessageHitScale {
    avatar: 20.,
    snippet_size: 13.,
    snippet_line_height: 18.,
    snippet_lines: 2,
};

/// Author, location and time of a message hit, and the snippet under them.
pub(crate) struct MessageHit<'a> {
    pub id: SharedString,
    pub author: &'a str,
    pub author_avatar: Option<&'a str>,
    pub location: Option<&'a str>,
    pub timestamp_ms: Option<i64>,
    pub snippet: &'a CleanSnippet,
}

impl MessageHit<'_> {
    /// The author's avatar, centered on the 18px header line.
    pub(crate) fn avatar(&self, scale: MessageHitScale) -> AnyElement {
        div()
            .flex_shrink_0()
            .pt(px(((18. - scale.avatar) / 2.).max(0.)))
            .child(Avatar::render(
                self.author,
                self.author_avatar,
                scale.avatar,
                default_avatar_background(self.author),
                text_primary(),
            ))
            .into_any_element()
    }

    /// Header line and snippet; callers append extras below. `on_accent` is for
    /// the accent-filled selected row, where everything turns white.
    pub(crate) fn column(&self, scale: MessageHitScale, on_accent: bool) -> gpui_kit::Div {
        let (primary, secondary): (Hsla, Hsla) = if on_accent {
            (
                rgb(sidebar_selected_fg()).into(),
                sidebar_selected_secondary(),
            )
        } else {
            (rgb(text_primary()).into(), rgb(text_secondary()).into())
        };
        let time = div()
            .id(SharedString::from(format!("{}-time", self.id)))
            .flex_shrink_0()
            .text_size(typography::font_px(12.))
            .line_height(typography::font_px(18.))
            .text_color(secondary)
            .child(compact_date_label(self.timestamp_ms))
            .when_some(
                self.timestamp_ms.and_then(message_full_timestamp),
                |label, full| label.tooltip(TextTooltip::single_line(full)),
            );
        div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(typography::label())
                            .line_height(typography::font_px(18.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(primary)
                            .child(self.author.to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .text_size(typography::font_px(12.))
                            .line_height(typography::font_px(18.))
                            .text_color(secondary)
                            .when_some(self.location, |label, location| {
                                label.child(location.to_string())
                            }),
                    )
                    .child(time),
            )
            .child(snippet_text(self.snippet, scale, on_accent))
    }
}

/// One search hit: author avatar, author, location and date, then the cleaned
/// snippet with matches marked. `previews` is `None` in the narrow scoped pane,
/// which also leaves out the location since every hit shares it.
pub(crate) fn search_result_row(
    index: usize,
    result: &SearchResult,
    context: Option<&SearchResultContext>,
    selected: bool,
    previews: Option<&HashMap<String, Arc<RenderImage>>>,
    cx: &mut Context<AppWindow>,
) -> AnyElement {
    let author = context
        .map(|context| context.author_name.as_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(result.message.author_id.0.as_str());
    let location = context
        .filter(|_| previews.is_some())
        .and_then(|context| context.location.as_deref());
    let snippet = clean_search_snippet(&result.snippet, &result.snippet_highlight_ranges);
    let hit = MessageHit {
        id: SharedString::from(format!("search-result-{index}")),
        author,
        author_avatar: context.and_then(|context| context.author_avatar.as_deref()),
        location,
        timestamp_ms: result.message.timestamp_ms,
        snippet: &snippet,
    };
    let first_file = result.message.attachments.first();

    div()
        .id(hit.id.clone())
        .w_full()
        .min_w(px(0.))
        .px(px(8.))
        .py(px(10.))
        .rounded(px(6.))
        // The keyboard selection shared with the switcher and New conversation:
        // solid accent, grey while the window isn't key.
        .when(selected, |row| row.bg(rgb(sidebar_selected_bg())))
        .when(!selected, |row| {
            row.hover(|style| style.bg(tint(text_primary(), 0.05)))
        })
        .cursor(gpui_kit::CursorStyle::PointingHand)
        .flex()
        .items_start()
        .gap(px(8.))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.open_search_result_at(index, window, cx);
        }))
        .child(hit.avatar(SEARCH_PAGE_HIT))
        .child(
            hit.column(SEARCH_PAGE_HIT, selected)
                .when_some(
                    first_file.filter(|_| result.message.attachments.len() == 1),
                    |column, attachment| {
                        column.child(div().pt(px(4.)).child(attachment_file_row(attachment)))
                    },
                )
                .when(result.message.attachments.len() > 1, |column| {
                    column.child(
                        div()
                            .text_size(typography::font_px(12.))
                            .text_color(if selected {
                                sidebar_selected_secondary()
                            } else {
                                rgb(text_secondary()).into()
                            })
                            .child(format!("{} files", result.message.attachments.len())),
                    )
                })
                .when(result.message.thread_reply_count > 0, |column| {
                    let count = result.message.thread_reply_count;
                    column.child(
                        div()
                            .text_size(typography::font_px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(if selected {
                                sidebar_selected_fg()
                            } else {
                                link()
                            }))
                            .child(format!(
                                "{count} {}",
                                if count == 1 { "reply" } else { "replies" }
                            )),
                    )
                })
                .when_some(
                    previews.filter(|_| !result.message.link_previews.is_empty()),
                    |column, video_render_cache| {
                        column.child(render_link_previews(
                            index,
                            &result.message.link_previews,
                            video_render_cache,
                            cx,
                        ))
                    },
                ),
        )
        .into_any_element()
}

/// How one snippet run is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RunPaint {
    pub weight: FontWeight,
    pub color: Option<u32>,
    pub background: Option<u32>,
    pub underline: bool,
}

/// Paint for a snippet run, or `None` for plain text. Query matches are bold on
/// the match fill; mentions keep their text color but get no fill, so neither
/// can be mistaken for the other. On the accent-filled selection everything is
/// white, and matches are bold and underlined.
pub(crate) fn snippet_run_paint(run: &SnippetRun<'_>, on_accent: bool) -> Option<RunPaint> {
    if run.mention.is_none() && !run.matched {
        return None;
    }
    if on_accent {
        return Some(RunPaint {
            weight: if run.matched {
                FontWeight::BOLD
            } else {
                FontWeight::SEMIBOLD
            },
            color: None,
            background: None,
            underline: run.matched,
        });
    }
    if run.matched {
        return Some(RunPaint {
            weight: FontWeight::BOLD,
            color: Some(search_match_fg()),
            background: Some(search_match_bg()),
            underline: false,
        });
    }
    let color = match run.mention? {
        MentionKind::User(_) => mention(),
        MentionKind::Broadcast => warning(),
        MentionKind::Channel => accent_strong(),
    };
    Some(RunPaint {
        weight: FontWeight::SEMIBOLD,
        color: Some(color),
        background: None,
        underline: false,
    })
}

/// A cleaned snippet clamped to `scale.snippet_lines`: query matches marked,
/// mentions in their text color. On the accent-filled selection (`on_accent`)
/// text is white and both are marked by weight alone.
pub(crate) fn snippet_text(
    snippet: &CleanSnippet,
    scale: MessageHitScale,
    on_accent: bool,
) -> AnyElement {
    let highlights = snippet.runs().into_iter().filter_map(|run| {
        let paint = snippet_run_paint(&run, on_accent)?;
        let style = HighlightStyle {
            font_weight: Some(paint.weight),
            color: paint.color.map(|color| rgb(color).into()),
            background_color: paint.background.map(|color| rgb(color).into()),
            underline: paint.underline.then(|| gpui_kit::UnderlineStyle {
                thickness: px(1.),
                color: None,
                wavy: false,
            }),
            ..Default::default()
        };
        Some((run.range.0..run.range.1, style))
    });
    div()
        .w_full()
        .min_w(px(0.))
        .text_size(typography::font_px(scale.snippet_size))
        .line_height(typography::font_px(scale.snippet_line_height))
        .text_color(if on_accent {
            rgb(sidebar_selected_fg())
        } else {
            rgb(text_primary())
        })
        .whitespace_normal()
        // No text_ellipsis: GPUI's truncation reuses the highlight runs of its
        // first measurement and would clip the snippet to a few characters.
        .line_clamp(scale.snippet_lines)
        .child(StyledText::new(snippet.text.clone()).with_highlights(highlights))
        .into_any_element()
}

fn render_link_previews(
    result_index: usize,
    previews: &[LinkPreview],
    video_render_cache: &HashMap<String, Arc<RenderImage>>,
    cx: &mut Context<AppWindow>,
) -> AnyElement {
    const MAX_VISIBLE: usize = 1;
    let hidden = previews.len().saturating_sub(MAX_VISIBLE);
    let visible = previews
        .iter()
        .take(MAX_VISIBLE)
        .cloned()
        .collect::<Vec<_>>();

    div()
        .flex()
        .flex_col()
        .items_start()
        .gap_1()
        .children(visible.into_iter().enumerate().map(|(index, preview)| {
            let url = preview.url.clone();
            let element_id =
                SharedString::from(format!("search-link-preview-{result_index}-{index}"));
            let image_element_id =
                SharedString::from(format!("search-link-preview-image-{result_index}-{index}"));
            let is_giphy = {
                let lower = preview.url.to_ascii_lowercase();
                lower.contains("giphy.com") || lower.contains("gph.is")
            };
            if preview.is_media {
                let media_source = preview
                    .thumbnail_asset
                    .clone()
                    .unwrap_or_else(|| preview.url.clone());
                let (media_width, media_height) =
                    media_frame_size(preview.media_width, preview.media_height, 260.0, 180.0);
                return div()
                    .id(element_id.clone())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_url_or_deep_link(&url, window, cx);
                    }))
                    .cursor(gpui_kit::CursorStyle::PointingHand)
                    .rounded_md()
                    .overflow_hidden()
                    .w(px(media_width))
                    .h(px(media_height))
                    .relative()
                    .child({
                        let decoded_video = if preview.is_video {
                            let key_url =
                                preview.video_url.as_deref().unwrap_or(preview.url.as_str());
                            let cache_key = video_preview_cache_key(key_url);
                            video_render_cache.get(&cache_key).cloned()
                        } else {
                            None
                        };
                        if let Some(render_image) = decoded_video {
                            img(ImageSource::Render(render_image))
                                .id(image_element_id.clone())
                                .w(px(media_width))
                                .h(px(media_height))
                                .object_fit(ObjectFit::Contain)
                        } else {
                            img(crate::views::link_preview_image_source(&media_source))
                                .id(image_element_id.clone())
                                .w(px(media_width))
                                .h(px(media_height))
                                .object_fit(ObjectFit::Contain)
                                .with_fallback({
                                    let site =
                                        preview.site.clone().unwrap_or_else(|| "media".to_string());
                                    move || {
                                        div()
                                            .text_size(typography::caption())
                                            .text_color(rgb(text_secondary()))
                                            .child(site.clone())
                                            .into_any_element()
                                    }
                                })
                        }
                    })
                    .when(preview.is_video, |container| {
                        let key_url = preview.video_url.as_deref().unwrap_or(preview.url.as_str());
                        let cache_key = video_preview_cache_key(key_url);
                        let has_decoded_video = video_render_cache.contains_key(&cache_key);
                        if has_decoded_video {
                            container
                        } else {
                            container.child(
                                div()
                                    .absolute()
                                    .right_1()
                                    .bottom_1()
                                    .px_1()
                                    .py_0p5()
                                    .rounded_sm()
                                    .bg(panel_alt_surface())
                                    .border_1()
                                    .border_color(rgb(border()))
                                    .text_size(typography::caption())
                                    .text_color(rgb(text_primary()))
                                    .child("video"),
                            )
                        }
                    })
                    .into_any_element();
            }
            let site = preview.site.clone().unwrap_or_else(|| "link".to_string());
            // The message already shows the URL, so a card without a title has none.
            let title = preview
                .title
                .clone()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| is_giphy.then(|| "GIPHY".to_string()));
            let thumbnail = preview.thumbnail_asset.clone();
            div()
                .id(element_id)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_url_or_deep_link(&url, window, cx);
                }))
                .cursor(gpui_kit::CursorStyle::PointingHand)
                .rounded_md()
                .border_1()
                .border_color(rgb(border()))
                .bg(panel_alt_surface())
                .px_2()
                .py_1()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .text_size(typography::caption())
                        .text_color(rgb(text_secondary()))
                        .child(site),
                )
                .when_some(title, |container, title| {
                    container.child(div().text_size(typography::body()).child(title))
                })
                .when_some(thumbnail, |container, thumb_path| {
                    let (tw, th) = if let (Some(w), Some(h)) =
                        (preview.media_width, preview.media_height)
                        && w > 0
                        && h > 0
                    {
                        let w = w as f32;
                        let h = h as f32;
                        let scale = (260.0 / w).min(150.0 / h).min(1.0);
                        (w * scale, h * scale)
                    } else {
                        (260.0, 150.0)
                    };
                    container.child(
                        img(ImageSource::from(std::path::PathBuf::from(thumb_path)))
                            .id(image_element_id)
                            .mt_0p5()
                            .w(px(tw))
                            .h(px(th))
                            .rounded_md()
                            .object_fit(ObjectFit::Contain),
                    )
                })
                .into_any_element()
        }))
        .when(hidden > 0, |container| {
            container.child(
                div()
                    .text_size(typography::caption())
                    .text_color(rgb(text_secondary()))
                    .child(format!("+{} more links", hidden)),
            )
        })
        .into_any_element()
}

fn media_frame_size(
    width: Option<u32>,
    height: Option<u32>,
    max_width: f32,
    max_height: f32,
) -> (f32, f32) {
    let mut width = width.unwrap_or(240) as f32;
    let mut height = height.unwrap_or(160) as f32;
    if width <= 1.0 || height <= 1.0 {
        return (max_width.min(240.0), max_height.min(160.0));
    }
    let scale = (max_width / width).min(max_height / height).min(1.0);
    width *= scale;
    height *= scale;
    (width.max(96.0), height.max(72.0))
}

#[cfg(test)]
mod tests {
    use super::snippet_run_paint;
    use crate::{
        app::theme::ThemeVariant,
        util::search_snippet::clean_search_snippet,
        views::{mention_you_soft, search_match_bg, with_theme},
    };
    use gpui_kit::FontWeight;

    #[test]
    fn matches_get_the_fill_and_mentions_never_do() {
        let raw = "@robin can you take the release build?";
        let start = raw.find("release").unwrap();
        let snippet = clean_search_snippet(raw, &[(start, start + "release".len())]);
        for theme in [ThemeVariant::Light, ThemeVariant::Dark] {
            with_theme(theme, || {
                let painted: Vec<_> = snippet
                    .runs()
                    .into_iter()
                    .filter_map(|run| {
                        let text = &snippet.text[run.range.0..run.range.1];
                        snippet_run_paint(&run, false).map(|paint| (text.to_string(), paint))
                    })
                    .collect();
                let (_, mention) = painted.iter().find(|(text, _)| text == "@robin").unwrap();
                assert_eq!(mention.background, None, "{theme:?}: mention filled");
                assert!(mention.color.is_some());
                let (_, matched) = painted.iter().find(|(text, _)| text == "release").unwrap();
                assert_eq!(matched.background, Some(search_match_bg()));
                assert_eq!(matched.weight, FontWeight::BOLD);
                assert_ne!(search_match_bg(), mention_you_soft(), "{theme:?}");
            });
        }
    }

    #[test]
    fn on_the_accent_selection_matches_are_bold_and_underlined() {
        let raw = "ship the release";
        let start = raw.find("release").unwrap();
        let snippet = clean_search_snippet(raw, &[(start, raw.len())]);
        let runs = snippet.runs();
        let matched = runs.iter().find(|run| run.matched).unwrap();
        let paint = snippet_run_paint(matched, true).unwrap();
        assert_eq!(paint.weight, FontWeight::BOLD);
        assert!(paint.underline);
        assert_eq!(paint.background, None);
        let plain = runs.iter().find(|run| !run.matched).unwrap();
        assert_eq!(snippet_run_paint(plain, true), None);
    }
}
