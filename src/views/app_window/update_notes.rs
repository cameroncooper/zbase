//! Limited Markdown presentation for GitHub notes. No HTML or remote images.
use super::AppWindow;
use crate::views::{
    inline_markdown::{InlineMarkdownConfig, apply_inline_markdown},
    selectable_text::{LinkRange, SelectableText, resolve_selectable_text},
};
use gpui::{AnyElement, Context, Entity, FontWeight, IntoElement, ParentElement, Styled, div, px};
use std::collections::HashMap;

fn links(source: &str) -> (String, Vec<LinkRange>) {
    let mut text = String::new();
    let mut ranges = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find('[') {
        let Some(middle) = rest[start..].find("](").map(|index| start + index) else {
            break;
        };
        let Some(end) = rest[middle + 2..].find(')').map(|index| middle + 2 + index) else {
            break;
        };
        let url = &rest[middle + 2..end];
        text.push_str(&rest[..start]);
        if reqwest::Url::parse(url)
            .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        {
            let label = &rest[start + 1..middle];
            let offset = text.len();
            text.push_str(label);
            ranges.push(LinkRange {
                byte_range: offset..text.len(),
                url: url.into(),
            });
        } else {
            text.push_str(&rest[start..=end]);
        }
        rest = &rest[end + 1..];
    }
    text.push_str(rest);
    (text, ranges)
}

pub(super) fn render(
    notes: &str,
    registry: &mut HashMap<String, Entity<SelectableText>>,
    cx: &mut Context<AppWindow>,
) -> AnyElement {
    let mut code = false;
    let children = notes
        .lines()
        .take(500)
        .enumerate()
        .filter_map(|(index, line)| {
            if line.trim_start().starts_with("```") {
                code = !code;
                return None;
            }
            let heading =
                !code && line.starts_with('#') && line.trim_start_matches('#').starts_with(' ');
            let line = if heading {
                line.trim_start_matches('#').trim_start()
            } else {
                line
            };
            let line = if !code && (line.starts_with("- ") || line.starts_with("* ")) {
                format!("• {}", &line[2..])
            } else {
                line.into()
            };
            let (text, ranges) = if code {
                (line, Vec::new())
            } else {
                links(&line)
            };
            let (text, ranges, styles) = if code {
                (text, Vec::new(), Vec::new())
            } else {
                let parsed =
                    apply_inline_markdown(&text, &ranges, &[], InlineMarkdownConfig::default());
                (parsed.text, parsed.link_ranges, parsed.styled_ranges)
            };
            let view = resolve_selectable_text(
                registry,
                format!("release-note-{index}"),
                text,
                ranges,
                styles,
                cx,
            );
            let mut row = div().w_full().min_w(px(0.)).min_h(px(12.)).child(view);
            if heading {
                row = row.font_weight(FontWeight::SEMIBOLD);
            }
            if code {
                row = row.font_family(crate::views::mono_font_family());
            }
            Some(row.into_any_element())
        })
        .collect::<Vec<_>>();
    div()
        .w_full()
        .min_w(px(0.))
        .flex()
        .flex_col()
        .gap_1()
        .children(children)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::links;
    #[test]
    fn github_notes_preserve_unicode_and_only_open_web_links() {
        let (text, links) =
            links("See [改善](https://example.com) or [local](file:///etc/passwd).");
        assert_eq!(text, "See 改善 or [local](file:///etc/passwd).");
        assert_eq!(links.len(), 1);
        assert_eq!(&text[links[0].byte_range.clone()], "改善");
    }
}
