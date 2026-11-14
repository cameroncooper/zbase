//! The `/giphy` picker above a composer: GIFs you sent before for a bare
//! `/giphy`, GIPHY search results once a query is typed.

use super::ComposerTarget;
use crate::services::giphy_service::{GiphyGif, GiphyService, GiphyView};
use crate::views::{
    accent, app_window::AppWindow, card_shadow, link, link_preview_image_source, modal_surface,
    shell_border_strong, subtle_surface, text_secondary, tint, typography,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, Context, CursorStyle, InteractiveElement, IntoElement, ObjectFit, ParentElement,
    ScrollHandle, SharedString, StatefulInteractiveElement, Styled, StyledImage, div, img, px, rgb,
};

const TILE_HEIGHT: f32 = 90.;

/// The search text of a `/giphy` command, `""` for a bare `/giphy`; `None`
/// when the text is not one.
pub(crate) fn giphy_command_query(text: &str) -> Option<&str> {
    let rest = text.trim_start().strip_prefix("/giphy")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(rest.trim())
}

/// The GIF the picker shows at `index`, if loaded.
pub(crate) fn giphy_picker_gif<'a>(
    service: &'a GiphyService,
    query: &str,
    index: usize,
) -> Option<&'a GiphyGif> {
    giphy_picker_gifs(service, query).get(index)
}

pub(crate) fn giphy_picker_gifs<'a>(service: &'a GiphyService, query: &str) -> &'a [GiphyGif] {
    match service.view(query) {
        GiphyView::Recents(gifs) | GiphyView::Results(gifs) => gifs,
        _ => &[],
    }
}

pub(crate) fn render_giphy_picker(
    service: &GiphyService,
    query: &str,
    selected: usize,
    target: ComposerTarget,
    scroll: &ScrollHandle,
    cx: &mut Context<AppWindow>,
) -> AnyElement {
    let view = service.view(query);
    let heading = match view {
        GiphyView::Recents(_) => "Recently sent".to_string(),
        _ => format!("GIFs for “{query}”"),
    };
    let notice = match view {
        GiphyView::Unavailable => Some("GIF search isn't set up in this build.".to_string()),
        GiphyView::KeepTyping => Some("Keep typing to search GIPHY.".to_string()),
        GiphyView::Loading => Some("Searching GIPHY…".to_string()),
        GiphyView::RateLimited { can_retry: false } => {
            Some("GIPHY is getting too many searches. Try again in a minute.".to_string())
        }
        GiphyView::RateLimited { can_retry: true } => {
            Some("GIPHY was getting too many searches.".to_string())
        }
        GiphyView::Failed => Some("Couldn't reach GIPHY.".to_string()),
        GiphyView::Recents([]) => Some("Type to search GIPHY. GIFs you send show up here.".into()),
        GiphyView::Results([]) => Some(format!("No GIFs for “{query}”.")),
        GiphyView::Recents(_) | GiphyView::Results(_) => None,
    };
    let can_retry = matches!(
        view,
        GiphyView::Failed | GiphyView::RateLimited { can_retry: true }
    );
    let retry_id = match target {
        ComposerTarget::Conversation => "giphy-picker-composer-retry",
        ComposerTarget::Thread => "giphy-picker-thread-retry",
    };
    let gifs = giphy_picker_gifs(service, query);
    let (grid_id, tile_id) = match target {
        ComposerTarget::Conversation => ("giphy-picker-composer", "giphy-picker-composer-gif"),
        ComposerTarget::Thread => ("giphy-picker-thread", "giphy-picker-thread-gif"),
    };
    let tiles = gifs
        .iter()
        .enumerate()
        .map(|(index, gif)| {
            let width = TILE_HEIGHT * gif.preview_width as f32 / gif.preview_height.max(1) as f32;
            let preview = service.preview_path(&gif.preview_url).cloned();
            div()
                .id((tile_id, index))
                .h(px(TILE_HEIGHT))
                .w(px(width))
                .flex_shrink_0()
                .rounded_md()
                .overflow_hidden()
                .border_2()
                .border_color(if index == selected {
                    rgb(accent()).into()
                } else {
                    tint(0x000000, 0.0)
                })
                .bg(subtle_surface())
                .cursor(CursorStyle::PointingHand)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.send_giphy_gif(target, index, window, cx);
                }))
                .when_some(preview, |tile, path| {
                    tile.child(
                        img(link_preview_image_source(&path.to_string_lossy()))
                            .id((SharedString::from(format!("{tile_id}-img")), index))
                            .size_full()
                            .object_fit(ObjectFit::Cover),
                    )
                })
        })
        .collect::<Vec<_>>();

    div()
        .w_full()
        .rounded_xl()
        .border_1()
        .border_color(shell_border_strong())
        .bg(modal_surface())
        .shadow(card_shadow())
        .p_2()
        .flex()
        .flex_col()
        .gap_1p5()
        // Clicks and wheel scrolls stay here instead of reaching the timeline.
        .occlude()
        .child(
            div()
                .flex()
                .justify_between()
                .text_size(typography::caption())
                .text_color(rgb(text_secondary()))
                .child(heading)
                .child("Powered by GIPHY"),
        )
        .when_some(notice, |picker, notice| {
            picker.child(
                div()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(typography::body())
                    .text_color(rgb(text_secondary()))
                    .child(notice)
                    .when(can_retry, |row| {
                        row.child(
                            div()
                                .id(retry_id)
                                .text_color(rgb(link()))
                                .cursor(CursorStyle::PointingHand)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.retry_giphy_search(target, cx);
                                }))
                                .child("Try again"),
                        )
                    }),
            )
        })
        .when(!tiles.is_empty(), |picker| {
            picker
                .child(
                    div()
                        .id(grid_id)
                        .flex()
                        .flex_wrap()
                        .gap_1()
                        // Two rows; the rest scroll.
                        .max_h(px(TILE_HEIGHT * 2. + 8.))
                        .overflow_y_scroll()
                        .track_scroll(scroll)
                        .children(tiles),
                )
                .child(
                    div()
                        .text_size(typography::caption())
                        .text_color(rgb(text_secondary()))
                        .child("↑↓ to choose · Return to send · Esc to close"),
                )
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::giphy_command_query;

    #[test]
    fn reads_the_query_of_a_giphy_command() {
        assert_eq!(giphy_command_query("/giphy"), Some(""));
        assert_eq!(giphy_command_query("/giphy "), Some(""));
        assert_eq!(
            giphy_command_query("  /giphy  happy cat "),
            Some("happy cat")
        );
        assert_eq!(giphy_command_query("/giphys"), None);
        assert_eq!(giphy_command_query("say /giphy cats"), None);
        assert_eq!(giphy_command_query("/gif cats"), None);
    }
}
