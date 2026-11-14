use crate::domain::{
    attachment::AttachmentSummary,
    ids::{ConversationId, MessageId},
};

#[derive(Clone, Debug)]
pub enum ComposerMode {
    Compose,
    Edit { message_id: MessageId },
    ReplyInThread { root_id: MessageId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutocompleteCandidate {
    BotCommand {
        text: String,
        username: String,
        description: String,
    },
    MentionUser {
        username: String,
        display_name: String,
        avatar_asset: Option<String>,
    },
    MentionBroadcast {
        keyword: String,
        description: String,
    },
    Emoji {
        label: String,
        insert_text: String,
        glyph: Option<String>,
    },
}

impl AutocompleteCandidate {
    pub fn completion_text(&self) -> String {
        match self {
            Self::BotCommand { text, .. } => format!("{text} "),
            Self::MentionUser { username, .. } => format!("@{username} "),
            Self::MentionBroadcast { keyword, .. } => format!("@{keyword} "),
            Self::Emoji { insert_text, .. } => format!("{insert_text} "),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutocompleteState {
    pub trigger: char,
    pub query: String,
    pub trigger_offset: usize,
    pub selected_index: usize,
    pub candidates: Vec<AutocompleteCandidate>,
}

#[derive(Clone, Debug)]
pub struct ComposerModel {
    pub conversation_id: ConversationId,
    pub mode: ComposerMode,
    pub draft_text: String,
    pub attachments: Vec<AttachmentSummary>,
    pub autocomplete: Option<AutocompleteState>,
}

/// Keybase chat Markdown the composer's format menu can apply. Keybase has no
/// link syntax (it links bare URLs itself), so there is no "Link" here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownFormat {
    Bold,
    Italic,
    Strikethrough,
    Code,
    CodeBlock,
    Quote,
}

impl MarkdownFormat {
    pub const ALL: [Self; 6] = [
        Self::Bold,
        Self::Italic,
        Self::Strikethrough,
        Self::Code,
        Self::CodeBlock,
        Self::Quote,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bold => "Bold",
            Self::Italic => "Italic",
            Self::Strikethrough => "Strikethrough",
            Self::Code => "Code",
            Self::CodeBlock => "Code Block",
            Self::Quote => "Quote",
        }
    }

    /// The marker wrapped around an inline span; `None` for block formats.
    fn inline_marker(self) -> Option<&'static str> {
        match self {
            Self::Bold => Some("*"),
            Self::Italic => Some("_"),
            Self::Strikethrough => Some("~"),
            Self::Code => Some("`"),
            Self::CodeBlock | Self::Quote => None,
        }
    }
}

/// Applies `format` to `selection` (byte offsets into `text`) and returns the
/// new text and the selection to restore.
///
/// Inline formats wrap the selected words, leaving surrounding spaces outside
/// the markers (Keybase ignores `*bold *`), and keep them selected. Applied to
/// a span that is already wrapped, they unwrap it. With nothing selected they
/// insert an empty pair and put the caret between its markers. A code block
/// fences the selection on lines of its own; a quote prefixes each selected
/// line with `> `.
pub fn apply_markdown_format(
    text: &str,
    selection: std::ops::Range<usize>,
    format: MarkdownFormat,
) -> (String, std::ops::Range<usize>) {
    let end = selection.end.min(text.len());
    let start = selection.start.min(end);
    if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return (text.to_string(), start..end);
    }
    match format.inline_marker() {
        Some(marker) => wrap_inline(text, start..end, marker),
        None if format == MarkdownFormat::CodeBlock => fence_block(text, start..end),
        None => quote_lines(text, start..end),
    }
}

fn wrap_inline(
    text: &str,
    selection: std::ops::Range<usize>,
    marker: &str,
) -> (String, std::ops::Range<usize>) {
    let selected = &text[selection.clone()];
    let start = selection.start + (selected.len() - selected.trim_start().len());
    let end = (selection.end - (selected.len() - selected.trim_end().len())).max(start);
    let width = marker.len();
    // Already wrapped: either the selection sits just inside the markers or
    // includes them.
    if start >= width
        && text[..start].ends_with(marker)
        && text[end..].starts_with(marker)
        && start < end
    {
        let mut out = String::with_capacity(text.len());
        out.push_str(&text[..start - width]);
        out.push_str(&text[start..end]);
        out.push_str(&text[end + width..]);
        return (out, start - width..end - width);
    }
    let inner = &text[start..end];
    if inner.len() > 2 * width && inner.starts_with(marker) && inner.ends_with(marker) {
        let mut out = String::with_capacity(text.len());
        out.push_str(&text[..start]);
        out.push_str(&inner[width..inner.len() - width]);
        out.push_str(&text[end..]);
        return (out, start..end - 2 * width);
    }
    let mut out = String::with_capacity(text.len() + 2 * width);
    out.push_str(&text[..start]);
    out.push_str(marker);
    out.push_str(inner);
    out.push_str(marker);
    out.push_str(&text[end..]);
    (out, start + width..end + width)
}

fn fence_block(text: &str, selection: std::ops::Range<usize>) -> (String, std::ops::Range<usize>) {
    let before = &text[..selection.start];
    let inner = &text[selection.clone()];
    let after = &text[selection.end..];
    let open = if before.is_empty() || before.ends_with('\n') {
        "```\n"
    } else {
        "\n```\n"
    };
    let close = if after.is_empty() || after.starts_with('\n') {
        "\n```"
    } else {
        "\n```\n"
    };
    let mut out = String::with_capacity(text.len() + open.len() + close.len());
    out.push_str(before);
    out.push_str(open);
    let inner_start = out.len();
    out.push_str(inner);
    let inner_end = out.len();
    out.push_str(close);
    out.push_str(after);
    (out, inner_start..inner_end)
}

fn quote_lines(text: &str, selection: std::ops::Range<usize>) -> (String, std::ops::Range<usize>) {
    let line_start = text[..selection.start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let line_end = text[selection.end..]
        .find('\n')
        .map_or(text.len(), |index| selection.end + index);
    let quoted = text[line_start..line_end]
        .split('\n')
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut out = String::with_capacity(text.len() + quoted.len());
    out.push_str(&text[..line_start]);
    out.push_str(&quoted);
    let quoted_end = out.len();
    out.push_str(&text[line_end..]);
    if selection.is_empty() {
        let caret = selection.start + 2;
        (out, caret..caret)
    } else {
        (out, line_start..quoted_end)
    }
}

#[cfg(test)]
mod tests {
    use super::{MarkdownFormat, apply_markdown_format};

    fn apply(text: &str, selection: std::ops::Range<usize>, format: MarkdownFormat) -> String {
        let (out, selected) = apply_markdown_format(text, selection, format);
        format!("{out}|{}", &out[selected])
    }

    #[test]
    fn inline_formats_wrap_the_selection_and_keep_it_selected() {
        assert_eq!(
            apply("ship it now", 5..7, MarkdownFormat::Bold),
            "ship *it* now|it"
        );
        assert_eq!(
            apply("ship it now", 5..7, MarkdownFormat::Italic),
            "ship _it_ now|it"
        );
        assert_eq!(
            apply("ship it now", 5..7, MarkdownFormat::Strikethrough),
            "ship ~it~ now|it"
        );
        assert_eq!(
            apply("run cargo", 4..9, MarkdownFormat::Code),
            "run `cargo`|cargo"
        );
    }

    #[test]
    fn surrounding_spaces_stay_outside_the_markers() {
        assert_eq!(
            apply("ship it now", 4..8, MarkdownFormat::Bold),
            "ship *it* now|it"
        );
    }

    #[test]
    fn empty_selection_inserts_a_pair_with_the_caret_inside() {
        let (out, caret) = apply_markdown_format("hi ", 3..3, MarkdownFormat::Bold);
        assert_eq!(out, "hi **");
        assert_eq!(caret, 4..4);
    }

    #[test]
    fn applying_twice_unwraps() {
        let (once, selected) = apply_markdown_format("ship it", 5..7, MarkdownFormat::Bold);
        let (twice, again) = apply_markdown_format(&once, selected, MarkdownFormat::Bold);
        assert_eq!(twice, "ship it");
        assert_eq!(&twice[again], "it");
        // Also when the markers themselves are selected.
        assert_eq!(apply("a *b* c", 2..5, MarkdownFormat::Bold), "a b c|b");
    }

    #[test]
    fn code_block_fences_on_lines_of_its_own() {
        assert_eq!(
            apply("see fn main() {} ok", 4..16, MarkdownFormat::CodeBlock),
            "see \n```\nfn main() {}\n```\n ok|fn main() {}"
        );
        assert_eq!(apply("x", 0..1, MarkdownFormat::CodeBlock), "```\nx\n```|x");
    }

    #[test]
    fn quote_prefixes_every_selected_line() {
        assert_eq!(
            apply("one\ntwo\nthree", 1..6, MarkdownFormat::Quote),
            "> one\n> two\nthree|> one\n> two"
        );
        let (out, caret) = apply_markdown_format("hello", 2..2, MarkdownFormat::Quote);
        assert_eq!(out, "> hello");
        assert_eq!(caret, 4..4);
    }

    #[test]
    fn offsets_off_char_boundaries_change_nothing() {
        let (out, _) = apply_markdown_format("é", 1..2, MarkdownFormat::Bold);
        assert_eq!(out, "é");
    }
}
