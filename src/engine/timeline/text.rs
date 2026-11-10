//! Styled text as the web view draws it: the counterpart of `SelectableText`'s
//! link and style ranges (views/selectable_text.rs), flattened into runs.

use std::ops::Range;

use serde::Serialize;

/// A palette color, named after its `views::*` helper; the web view maps each
/// to a CSS variable (`--text-secondary`), so both themes work.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    TextPrimary,
    TextSecondary,
    Accent,
    AccentSoft,
    PanelAltBg,
    Mention,
    MentionSoft,
    MentionYou,
    MentionYouSoft,
    AffinityPositive,
    AffinityPositiveSoft,
    AffinityBroken,
    AffinityBrokenSoft,
    Warning,
    WarningSoft,
    InlineCodeForeground,
    InlineCodeBackground,
    SyntaxKeyword,
    SyntaxFunction,
    SyntaxString,
    SyntaxNumber,
    SyntaxType,
    SyntaxComment,
    SyntaxPunctuation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkRange {
    pub byte_range: Range<usize>,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyledRange {
    pub byte_range: Range<usize>,
    pub color: Option<Role>,
    pub background_color: Option<Role>,
    pub bold: bool,
    pub italic: bool,
    pub monospace: bool,
    pub strikethrough: bool,
    /// Annotation such as "(edited)": drawn small and left out of copied text.
    pub meta: bool,
}

impl StyledRange {
    pub fn inline_code(byte_range: Range<usize>) -> Self {
        Self {
            byte_range,
            color: Some(Role::InlineCodeForeground),
            background_color: Some(Role::InlineCodeBackground),
            bold: false,
            italic: false,
            monospace: true,
            strikethrough: false,
            meta: false,
        }
    }

    pub fn colored(byte_range: Range<usize>, color: Role, background: Option<Role>) -> Self {
        Self {
            byte_range,
            color: Some(color),
            background_color: background,
            bold: false,
            italic: false,
            monospace: false,
            strikethrough: false,
            meta: false,
        }
    }
}

/// A custom emoji drawn inline in place of a placeholder character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineAttachment {
    pub byte_range: Range<usize>,
    pub source: String,
    pub fallback_text: String,
    pub size_px: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InlineImage {
    pub src: String,
    pub fallback: String,
    pub size: u16,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// One span of uniformly styled text (or one inline image).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Run {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Role>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<Role>,
    #[serde(skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub mono: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub strike: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub meta: bool,
    /// Underlined in the link color unless it is an internal `zbase-` link.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<InlineImage>,
}

/// `build_text_runs_for_content`: later styles override colors, flags accumulate.
pub fn build_runs(
    content: &str,
    link_ranges: &[LinkRange],
    styled_ranges: &[StyledRange],
    attachments: &[InlineAttachment],
) -> Vec<Run> {
    let len = content.len();
    if len == 0 {
        return Vec::new();
    }
    let clamp = |range: &Range<usize>| range.start.min(len)..range.end.min(len);
    let mut boundaries = vec![0, len];
    for range in link_ranges
        .iter()
        .map(|link| &link.byte_range)
        .chain(styled_ranges.iter().map(|styled| &styled.byte_range))
        .chain(attachments.iter().map(|attachment| &attachment.byte_range))
    {
        let range = clamp(range);
        if range.start < range.end {
            boundaries.push(range.start);
            boundaries.push(range.end);
        }
    }
    boundaries.retain(|index| content.is_char_boundary(*index));
    boundaries.sort_unstable();
    boundaries.dedup();
    let covers = |range: &Range<usize>, start: usize, end: usize| {
        let range = clamp(range);
        range.start < range.end && start >= range.start && end <= range.end
    };

    let mut runs: Vec<Run> = Vec::new();
    for window in boundaries.windows(2) {
        let (start, end) = (window[0], window[1]);
        if end <= start {
            continue;
        }
        if let Some(attachment) = attachments
            .iter()
            .find(|attachment| covers(&attachment.byte_range, start, end))
        {
            runs.push(Run {
                text: content[start..end].to_string(),
                image: Some(InlineImage {
                    src: attachment.source.clone(),
                    fallback: attachment.fallback_text.clone(),
                    size: attachment.size_px,
                }),
                ..Default::default()
            });
            continue;
        }
        let mut run = Run {
            text: content[start..end].to_string(),
            link: link_ranges
                .iter()
                .find(|link| covers(&link.byte_range, start, end))
                .map(|link| link.url.clone()),
            ..Default::default()
        };
        for styled in styled_ranges
            .iter()
            .filter(|styled| covers(&styled.byte_range, start, end))
        {
            if styled.color.is_some() {
                run.color = styled.color;
            }
            if styled.background_color.is_some() {
                run.bg = styled.background_color;
            }
            run.mono |= styled.monospace;
            run.bold |= styled.bold;
            run.italic |= styled.italic;
            run.strike |= styled.strikethrough;
            run.meta |= styled.meta;
        }
        if let Some(last) = runs.last_mut()
            && last.image.is_none()
            && same_style(last, &run)
        {
            last.text.push_str(&run.text);
            continue;
        }
        runs.push(run);
    }
    runs
}

fn same_style(a: &Run, b: &Run) -> bool {
    a.color == b.color
        && a.bg == b.bg
        && a.bold == b.bold
        && a.italic == b.italic
        && a.mono == b.mono
        && a.strike == b.strike
        && a.meta == b.meta
        && a.link == b.link
}
