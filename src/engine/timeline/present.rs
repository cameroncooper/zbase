//! What `TimelineList::render_message` derives from a `MessageRecord` before it
//! draws (views/timeline/mod.rs): the styled body, labels, reactions and
//! attachments. The web view renders these; the main timeline and the thread
//! pane share them.

use std::collections::HashMap;

use serde::Serialize;
use unicode_segmentation::UnicodeSegmentation;

use super::{
    code_highlight::CodeHighlightCache,
    inline_markdown::{InlineMarkdownConfig, apply_inline_markdown, remap_source_byte_range},
    text::{InlineAttachment, LinkRange, Role, Run, StyledRange, build_runs},
};
use crate::{
    domain::{
        affinity::Affinity,
        attachment::{AttachmentKind, AttachmentSource, AttachmentSummary},
        ids::{MessageId, UserId},
        message::{
            BroadcastKind, ChatEvent, EmojiSourceRef, LinkPreview, MessageFragment, MessageRecord,
        },
    },
    models::{
        link_previews::urls_match,
        timeline_model::{InlineEmojiRender, MessageReactionRender, TimelineModel, TimelineRow},
    },
    state::state::UploadProgress,
    util::{
        emoji_aliases::standard_emoji_for_alias,
        formatting::{
            compact_date_label, message_full_timestamp, message_time_label, message_timestamp_label,
        },
    },
};

const EDITED_LABEL: &str = " (edited)";
const CUSTOM_EMOJI_INLINE_SIZE_PX: u16 = 20;
const CUSTOM_EMOJI_EMOJI_ONLY_SIZE_PX: u16 = 48;
const INLINE_ATTACHMENT_PLACEHOLDER: &str = "\u{2003}";

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    /// Text, inline code, emoji, mentions and links in one wrapping paragraph.
    Inline {
        runs: Vec<Run>,
    },
    Code {
        runs: Vec<Run>,
    },
    Quote {
        runs: Vec<Run>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReplyContext {
    pub author: String,
    pub excerpt: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReactionView {
    /// What toggling sends back (`quick_react`).
    pub emoji: String,
    pub unicode: Option<String>,
    pub asset: Option<String>,
    pub count: usize,
    pub mine: bool,
    pub tooltip: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UploadView {
    pub label: String,
    pub fraction: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AttachmentView {
    pub kind: AttachmentKind,
    pub name: String,
    pub size_label: Option<String>,
    /// "[IMG] photo.png (1.2 MB)", the image fallback.
    pub display_label: String,
    /// Inline thumbnail (local path or URL) for images and videos.
    pub image: Option<String>,
    pub animated: bool,
    pub preview_width: Option<u32>,
    pub preview_height: Option<u32>,
    pub full_width: Option<u32>,
    pub full_height: Option<u32>,
    /// Images only: the lightbox opens on a click.
    pub lightbox: bool,
    /// Videos only: opened in the system player.
    pub video_path: Option<String>,
    pub duration_label: Option<String>,
    /// What the download control saves (`downloads::source`), `None` while unavailable.
    pub download_source: Option<String>,
    pub upload: Option<UploadView>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MessageView {
    /// "4:34 PM", beside the name in the main timeline.
    pub time: String,
    /// "4:34 PM • Yesterday", in the thread pane, which has no day dividers.
    pub timestamp: String,
    pub timestamp_tooltip: Option<String>,
    pub blocks: Vec<Block>,
    pub emoji_only: bool,
    /// Image-only or media-link-only messages draw no text body.
    pub hide_body: bool,
    /// "edited" on a line of its own (otherwise it ends the last paragraph).
    pub edited_line: bool,
    pub caption: Option<String>,
    pub reply: Option<ReplyContext>,
    pub reactions: Vec<ReactionView>,
    /// "Last reply Yesterday", under a thread root.
    pub last_reply: Option<String>,
    pub own: bool,
    pub attachments: Vec<AttachmentView>,
    /// Link previews beyond the three shown.
    pub hidden_previews: usize,
}

pub struct Inputs<'a> {
    pub timeline: &'a TimelineModel,
    pub find_query: Option<&'a str>,
    pub upload_progress: &'a HashMap<MessageId, UploadProgress>,
}

pub fn message_view(
    message: &MessageRecord,
    inputs: &Inputs<'_>,
    code_cache: &mut CodeHighlightCache,
) -> MessageView {
    let timeline = inputs.timeline;
    let has_attachments = !message.attachments.is_empty();
    let image_attachment_message = has_attachments
        && message
            .attachments
            .iter()
            .all(|attachment| attachment.kind == AttachmentKind::Image);
    let media_link_only = !image_attachment_message
        && is_media_link_only_message(&message.fragments, &message.link_previews);
    let emoji_only = message_is_emoji_only(&message.fragments, has_attachments);
    let show_edited = message.edited.is_some();
    let edited_inline = show_edited
        && !image_attachment_message
        && !media_link_only
        && message.fragments.last().is_some_and(is_inline_fragment)
        && !emoji_only;
    let hide_body = image_attachment_message || media_link_only;
    let blocks = if hide_body {
        Vec::new()
    } else {
        render_blocks(message, emoji_only, edited_inline, inputs, code_cache)
    };
    let timestamp_tooltip = message
        .timestamp_ms
        .and_then(message_full_timestamp)
        .map(|full| {
            match message
                .edited
                .as_ref()
                .and_then(|edit| edit.edited_at_ms)
                .map(|edited_ms| message_time_label(Some(edited_ms)))
            {
                Some(edited_at) => format!("{full} · Edited {edited_at}"),
                None => full,
            }
        });
    let own = timeline
        .current_user_id
        .as_ref()
        .is_some_and(|me| me.0.eq_ignore_ascii_case(&message.author_id.0));
    let upload = inputs.upload_progress.get(&message.id).copied();
    MessageView {
        time: message_time_label(message.timestamp_ms),
        timestamp: message_timestamp_label(message.timestamp_ms),
        timestamp_tooltip,
        blocks,
        emoji_only,
        hide_body,
        edited_line: show_edited && !edited_inline,
        caption: (image_attachment_message && !message.fragments.is_empty())
            .then(|| message_caption_text(message))
            .flatten(),
        reply: reply_context(message, timeline),
        reactions: reactions(message, timeline),
        last_reply: timeline
            .thread_summaries
            .get(&message.id)
            .and_then(|summary| summary.last_reply_ms)
            .map(|ms| format!("Last reply {}", compact_date_label(Some(ms)))),
        own,
        attachments: message
            .attachments
            .iter()
            .map(|attachment| attachment_view(attachment, upload))
            .collect(),
        hidden_previews: message.link_previews.len().saturating_sub(3),
    }
}

fn is_inline_fragment(fragment: &MessageFragment) -> bool {
    !matches!(
        fragment,
        MessageFragment::Code { .. } | MessageFragment::Quote(_)
    )
}

fn render_blocks(
    message: &MessageRecord,
    emoji_only: bool,
    edited_inline: bool,
    inputs: &Inputs<'_>,
    code_cache: &mut CodeHighlightCache,
) -> Vec<Block> {
    let mut blocks = Vec::new();
    if message.fragments.is_empty() {
        return blocks;
    }
    let mut group: Vec<&MessageFragment> = Vec::new();
    let last_index = message.fragments.len() - 1;
    for (index, fragment) in message.fragments.iter().enumerate() {
        if is_inline_fragment(fragment) {
            group.push(fragment);
            if index == last_index {
                blocks.push(inline_block(&group, emoji_only, edited_inline, inputs));
            }
            continue;
        }
        if !group.is_empty() {
            blocks.push(inline_block(&group, emoji_only, false, inputs));
            group.clear();
        }
        match fragment {
            MessageFragment::Code { text, lang } => {
                let ranges = code_cache.ranges(lang.as_deref(), text);
                blocks.push(Block::Code {
                    runs: build_runs(text, &[], &ranges, &[]),
                });
            }
            MessageFragment::Quote(text) => blocks.push(Block::Quote {
                runs: build_runs(text, &[], &[], &[]),
            }),
            _ => {}
        }
    }
    blocks
}

/// `render_inline_fragments`: one selectable paragraph with markdown applied.
fn inline_block(
    fragments: &[&MessageFragment],
    emoji_only: bool,
    edited_suffix: bool,
    inputs: &Inputs<'_>,
) -> Block {
    let timeline = inputs.timeline;
    let mut combined = String::new();
    let mut links = Vec::new();
    let mut styles = Vec::new();
    let mut attachments = Vec::new();
    for fragment in fragments {
        match fragment {
            MessageFragment::Text(text) => {
                for segment in inline_text_segments(text) {
                    match segment {
                        Segment::Text(value) => combined.push_str(&value),
                        Segment::InlineCode(code) => {
                            push_inline_code(&mut combined, &mut styles, &code)
                        }
                        Segment::Link(url) => {
                            let start = combined.len();
                            combined.push_str(&url);
                            links.push(LinkRange {
                                byte_range: start..combined.len(),
                                url,
                            });
                        }
                    }
                }
            }
            MessageFragment::InlineCode(code) => push_inline_code(&mut combined, &mut styles, code),
            MessageFragment::Emoji { alias, source_ref } => {
                if let Some(render) = resolved_emoji_render(alias, source_ref.as_ref(), timeline) {
                    if let Some(unicode) = render.unicode.as_ref() {
                        combined.push_str(unicode);
                        continue;
                    }
                    if let Some(asset) = render.asset_path.as_ref() {
                        let start = combined.len();
                        combined.push_str(INLINE_ATTACHMENT_PLACEHOLDER);
                        attachments.push(InlineAttachment {
                            byte_range: start..combined.len(),
                            source: asset.clone(),
                            fallback_text: format!(":{alias}:"),
                            size_px: custom_emoji_size(emoji_only),
                        });
                        continue;
                    }
                }
                combined.push_str(&inline_emoji_text(alias, source_ref.as_ref(), timeline));
            }
            MessageFragment::Mention(user_id) => {
                let start = combined.len();
                let (fg, bg) = mention_colors_for_user(
                    &timeline.affinity_index,
                    timeline.current_user_id.as_ref(),
                    user_id,
                );
                push_styled(
                    &mut combined,
                    &mut styles,
                    &format!("@{}", user_id.0),
                    fg,
                    bg,
                );
                links.push(LinkRange {
                    byte_range: start..combined.len(),
                    url: format!("zbase-mention:{}", user_id.0),
                });
            }
            MessageFragment::ChannelMention { name } => {
                let start = combined.len();
                push_styled(
                    &mut combined,
                    &mut styles,
                    &format!("#{name}"),
                    Role::Accent,
                    Role::AccentSoft,
                );
                links.push(LinkRange {
                    byte_range: start..combined.len(),
                    url: format!("zbase-channel:{name}"),
                });
            }
            MessageFragment::BroadcastMention(kind) => push_styled(
                &mut combined,
                &mut styles,
                match kind {
                    BroadcastKind::Here => "@here",
                    BroadcastKind::All => "@channel",
                },
                Role::Warning,
                Role::WarningSoft,
            ),
            MessageFragment::Link { url, display } => {
                let start = combined.len();
                combined.push_str(display);
                links.push(LinkRange {
                    byte_range: start..combined.len(),
                    url: url.clone(),
                });
            }
            MessageFragment::Code { text, .. } | MessageFragment::Quote(text) => {
                combined.push_str(text)
            }
        }
    }
    append_find_query_highlight_ranges(&combined, inputs.find_query, &mut styles);
    let mut parsed = apply_inline_markdown(
        &combined,
        &links,
        &styles,
        InlineMarkdownConfig {
            spoiler_foreground: Some(Role::TextSecondary),
            spoiler_background: Some(Role::PanelAltBg),
        },
    );
    if edited_suffix {
        let start = parsed.text.len();
        parsed.text.push_str(EDITED_LABEL);
        parsed.styled_ranges.push(StyledRange {
            meta: true,
            ..StyledRange::colored(start..parsed.text.len(), Role::TextSecondary, None)
        });
    }
    let attachments = attachments
        .into_iter()
        .filter_map(|attachment| {
            remap_source_byte_range(
                &parsed.source_to_output_mapping,
                combined.len(),
                &attachment.byte_range,
            )
            .map(|byte_range| InlineAttachment {
                byte_range,
                ..attachment
            })
        })
        .collect::<Vec<_>>();
    Block::Inline {
        runs: build_runs(
            &parsed.text,
            &parsed.link_ranges,
            &parsed.styled_ranges,
            &attachments,
        ),
    }
}

fn push_styled(
    combined: &mut String,
    styles: &mut Vec<StyledRange>,
    text: &str,
    fg: Role,
    bg: Role,
) {
    let start = combined.len();
    combined.push_str(text);
    if start < combined.len() {
        styles.push(StyledRange {
            bold: true,
            ..StyledRange::colored(start..combined.len(), fg, Some(bg))
        });
    }
}

fn push_inline_code(combined: &mut String, styles: &mut Vec<StyledRange>, code: &str) {
    let start = combined.len();
    combined.push_str(code);
    if start < combined.len() {
        styles.push(StyledRange::inline_code(start..combined.len()));
    }
}

fn append_find_query_highlight_ranges(
    combined: &str,
    find_query: Option<&str>,
    styles: &mut Vec<StyledRange>,
) {
    let Some(query) = find_query.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    let haystack = combined.to_ascii_lowercase();
    let needle = query.to_ascii_lowercase();
    let mut search_from = 0usize;
    while let Some(relative_start) = haystack[search_from..].find(&needle) {
        let start = search_from + relative_start;
        let end = start + needle.len();
        if start < end && end <= combined.len() {
            styles.push(StyledRange {
                byte_range: start..end,
                color: None,
                background_color: Some(Role::WarningSoft),
                bold: false,
                italic: false,
                monospace: false,
                strikethrough: false,
                meta: false,
            });
        }
        search_from = end;
        if search_from >= haystack.len() {
            break;
        }
    }
}

fn custom_emoji_size(emoji_only: bool) -> u16 {
    if emoji_only {
        CUSTOM_EMOJI_EMOJI_ONLY_SIZE_PX
    } else {
        CUSTOM_EMOJI_INLINE_SIZE_PX
    }
}

fn resolved_emoji_render<'a>(
    alias: &str,
    source_ref: Option<&EmojiSourceRef>,
    timeline: &'a TimelineModel,
) -> Option<&'a InlineEmojiRender> {
    if let Some(source_ref) = source_ref
        && let Some(render) = timeline.emoji_source_index.get(&source_ref.cache_key())
    {
        return Some(render);
    }
    timeline.emoji_index.get(&alias.to_ascii_lowercase())
}

fn inline_emoji_text(
    alias: &str,
    source_ref: Option<&EmojiSourceRef>,
    timeline: &TimelineModel,
) -> String {
    if let Some(unicode) =
        resolved_emoji_render(alias, source_ref, timeline).and_then(|render| render.unicode.clone())
    {
        return unicode;
    }
    if let Some(unicode) = standard_emoji_for_alias(alias) {
        return unicode.to_string();
    }
    format!(":{alias}:")
}

/// `views::mention_colors_for_user`.
fn mention_colors_for_user(
    affinity_index: &HashMap<UserId, Affinity>,
    current_user_id: Option<&UserId>,
    user_id: &UserId,
) -> (Role, Role) {
    if current_user_id.is_some_and(|me| me.0.eq_ignore_ascii_case(&user_id.0)) {
        return (Role::MentionYou, Role::MentionYouSoft);
    }
    let affinity = affinity_index
        .get(user_id)
        .copied()
        .or_else(|| {
            affinity_index.iter().find_map(|(key, affinity)| {
                key.0.eq_ignore_ascii_case(&user_id.0).then_some(*affinity)
            })
        })
        .unwrap_or(Affinity::None);
    match affinity {
        Affinity::None => (Role::Mention, Role::MentionSoft),
        Affinity::Positive => (Role::AffinityPositive, Role::AffinityPositiveSoft),
        Affinity::Broken => (Role::AffinityBroken, Role::AffinityBrokenSoft),
    }
}

enum Segment {
    Text(String),
    InlineCode(String),
    Link(String),
}

/// `inline_text_segments`: backtick code and bare URLs inside a text fragment.
fn inline_text_segments(text: &str) -> Vec<Segment> {
    let chars = text.chars().collect::<Vec<_>>();
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index] == '`' {
            let start = index + 1;
            if let Some(end) = chars[start..].iter().position(|&ch| ch == '`') {
                let end = start + end;
                if end > start {
                    if !current.is_empty() {
                        segments.push(Segment::Text(std::mem::take(&mut current)));
                    }
                    segments.push(Segment::InlineCode(chars[start..end].iter().collect()));
                    index = end + 1;
                    continue;
                }
            }
        }
        if let Some(url_len) = detect_url(&chars, index) {
            if !current.is_empty() {
                segments.push(Segment::Text(std::mem::take(&mut current)));
            }
            segments.push(Segment::Link(
                chars[index..index + url_len].iter().collect(),
            ));
            index += url_len;
            continue;
        }
        current.push(chars[index]);
        index += 1;
    }
    if !current.is_empty() {
        segments.push(Segment::Text(current));
    }
    segments
}

fn detect_url(chars: &[char], start: usize) -> Option<usize> {
    let has_prefix = |prefix: &str| {
        let prefix = prefix.chars().collect::<Vec<_>>();
        chars.len() >= start + prefix.len() && chars[start..start + prefix.len()] == prefix[..]
    };
    let (prefix_len, min_required_end) = if has_prefix("https://") {
        (8, start + 8)
    } else if has_prefix("http://") {
        (7, start + 7)
    } else if has_prefix("keybase://") {
        (10, start + 12)
    } else {
        return None;
    };
    let mut end = start + prefix_len;
    while end < chars.len() && !chars[end].is_whitespace() {
        end += 1;
    }
    while end > start && matches!(chars[end - 1], '.' | ',' | ')' | ']' | ';') {
        end -= 1;
    }
    (end > min_required_end).then_some(end - start)
}

fn is_emoji_only(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let mut has_emoji = false;
    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            continue;
        }
        if ch.is_ascii_alphanumeric() || ch.is_ascii_punctuation() {
            return false;
        }
        has_emoji = true;
    }
    has_emoji
}

fn message_is_emoji_only(fragments: &[MessageFragment], has_attachments: bool) -> bool {
    !fragments.is_empty()
        && !has_attachments
        && fragments.iter().all(|fragment| match fragment {
            MessageFragment::Emoji { .. } => true,
            MessageFragment::Text(text) => is_emoji_only(text),
            _ => false,
        })
}

/// `TimelineList::message_caption_text`.
pub fn message_caption_text(message: &MessageRecord) -> Option<String> {
    let caption = message
        .fragments
        .iter()
        .map(|fragment| match fragment {
            MessageFragment::Text(text)
            | MessageFragment::Code { text, .. }
            | MessageFragment::Quote(text) => text.clone(),
            MessageFragment::InlineCode(text) => format!("`{text}`"),
            MessageFragment::Emoji { alias, .. } => format!(":{alias}:"),
            MessageFragment::Mention(user_id) => format!("@{}", user_id.0),
            MessageFragment::ChannelMention { name } => format!("#{name}"),
            MessageFragment::BroadcastMention(BroadcastKind::Here) => "@here".to_string(),
            MessageFragment::BroadcastMention(BroadcastKind::All) => "@channel".to_string(),
            MessageFragment::Link { display, .. } => display.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let caption = caption.trim();
    (!caption.is_empty()).then(|| caption.to_string())
}

/// `reply_context::cached_reply_context`.
fn reply_context(message: &MessageRecord, timeline: &TimelineModel) -> Option<ReplyContext> {
    let reply_to = message.reply_to.as_ref()?;
    let parent_id = message
        .thread_root_id
        .as_ref()
        .filter(|id| **id != message.id)
        .unwrap_or(reply_to);
    let find_parent = |id: &MessageId| {
        timeline.rows.iter().find_map(|row| match row {
            TimelineRow::Message(row)
                if row.message.id == *id
                    && row.message.conversation_id == message.conversation_id =>
            {
                Some(row)
            }
            _ => None,
        })
    };
    let parent = find_parent(parent_id).or_else(|| {
        (parent_id != reply_to)
            .then(|| find_parent(reply_to))
            .flatten()
    })?;
    if matches!(parent.message.event, Some(ChatEvent::MessageDeleted { .. })) {
        return None;
    }
    let text = message_caption_text(&parent.message)
        .or_else(|| {
            parent
                .message
                .attachments
                .first()
                .map(attachment_display_label)
        })
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut graphemes = text.graphemes(true);
    let mut excerpt = graphemes.by_ref().take(100).collect::<String>();
    if graphemes.next().is_some() {
        excerpt.push('…');
    }
    Some(ReplyContext {
        author: parent.author.display_name.clone(),
        excerpt,
    })
}

fn reactions(message: &MessageRecord, timeline: &TimelineModel) -> Vec<ReactionView> {
    let mut reactions = timeline
        .reaction_index
        .get(&message.id)
        .cloned()
        .unwrap_or_default();
    reactions.sort_by(|left, right| left.emoji.cmp(&right.emoji));
    reactions
        .iter()
        .map(|reaction| {
            let alias = reaction.emoji.trim_matches(':').to_ascii_lowercase();
            let resolved = resolved_emoji_render(&alias, reaction.source_ref.as_ref(), timeline);
            let (unicode, asset) = match resolved {
                Some(render) => (
                    render.unicode.clone(),
                    render
                        .unicode
                        .is_none()
                        .then(|| render.asset_path.clone())
                        .flatten(),
                ),
                None => (standard_emoji_for_alias(&alias).map(str::to_string), None),
            };
            ReactionView {
                emoji: reaction.emoji.clone(),
                unicode,
                asset,
                count: reaction.count,
                mine: reaction.reacted_by_me,
                tooltip: reaction_hover_text(reaction),
            }
        })
        .collect()
}

fn reaction_hover_text(reaction: &MessageReactionRender) -> Option<String> {
    let mut labels = reaction
        .actors
        .iter()
        .map(|actor| {
            if actor.display_name.eq_ignore_ascii_case(&actor.user_id) {
                actor.user_id.clone()
            } else {
                format!("{} (@{})", actor.display_name, actor.user_id)
            }
        })
        .collect::<Vec<_>>();
    labels.sort();
    labels.dedup();
    if labels.is_empty() {
        return None;
    }
    const MAX_NAMES: usize = 8;
    if labels.len() > MAX_NAMES {
        let remaining = labels.len() - MAX_NAMES;
        labels.truncate(MAX_NAMES);
        return Some(format!("{} (+{} more)", labels.join(", "), remaining));
    }
    Some(labels.join(", "))
}

fn trim_url_token(token: &str) -> Option<&str> {
    let mut t = token.trim();
    while let Some(ch) = t.as_bytes().first().copied()
        && matches!(ch, b'(' | b'[' | b'<')
    {
        t = &t[1..];
    }
    while let Some(ch) = t.as_bytes().last().copied()
        && matches!(ch, b'.' | b',' | b')' | b']' | b'>' | b';')
    {
        t = &t[..t.len().saturating_sub(1)];
    }
    (!t.is_empty()).then_some(t)
}

/// `views::is_media_link_only_message`.
fn is_media_link_only_message(fragments: &[MessageFragment], previews: &[LinkPreview]) -> bool {
    if fragments.is_empty() || previews.is_empty() {
        return false;
    }
    let matches_media_preview = |url: &str| {
        previews.iter().any(|p| {
            p.is_media
                && (urls_match(&p.url, url)
                    || p.video_url.as_deref().is_some_and(|v| urls_match(v, url)))
        })
    };
    let mut saw_any_url = false;
    for fragment in fragments {
        match fragment {
            MessageFragment::Link { url, .. } => {
                if !matches_media_preview(url) {
                    return false;
                }
                saw_any_url = true;
            }
            MessageFragment::Text(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let mut local_saw = false;
                for raw in trimmed.split_whitespace() {
                    let Some(token) = trim_url_token(raw) else {
                        continue;
                    };
                    if !(token.starts_with("http://") || token.starts_with("https://"))
                        || !matches_media_preview(token)
                    {
                        return false;
                    }
                    local_saw = true;
                }
                if !local_saw {
                    return false;
                }
                saw_any_url = true;
            }
            _ => return false,
        }
    }
    saw_any_url
}

// Attachment helpers from views/mod.rs.

fn attachment_kind_tag(kind: &AttachmentKind) -> &'static str {
    match kind {
        AttachmentKind::Image => "IMG",
        AttachmentKind::Video => "VID",
        AttachmentKind::Audio => "AUD",
        AttachmentKind::File => "FILE",
    }
}

pub fn attachment_size_text(size_bytes: u64) -> Option<String> {
    if size_bytes == 0 {
        return None;
    }
    if size_bytes < 1024 {
        return Some(format!("{size_bytes} B"));
    }
    const UNITS: &[&str] = &["KB", "MB", "GB", "TB"];
    let mut value = size_bytes as f64 / 1024.0;
    let mut unit_index = 0usize;
    while value >= 1024.0 && unit_index + 1 < UNITS.len() {
        value /= 1024.0;
        unit_index += 1;
    }
    let unit = UNITS[unit_index];
    if value >= 10.0 {
        Some(format!("{value:.0} {unit}"))
    } else {
        Some(format!("{value:.1} {unit}"))
    }
}

fn upload_status_label(sent_bytes: u64, total_bytes: u64) -> String {
    if total_bytes == 0 {
        return "Uploading…".to_string();
    }
    let sent_bytes = sent_bytes.min(total_bytes);
    let percent = (sent_bytes as f64 / total_bytes as f64 * 100.).floor() as u64;
    let Some(total) = attachment_size_text(total_bytes) else {
        return "Uploading…".to_string();
    };
    let (number, unit) = total.split_once(' ').unwrap_or((total.as_str(), ""));
    let divisor = match unit {
        "KB" => 1024f64,
        "MB" => 1024f64.powi(2),
        "GB" => 1024f64.powi(3),
        "TB" => 1024f64.powi(4),
        _ => 1.,
    };
    let sent_value = sent_bytes as f64 / divisor;
    let decimals = if unit == "B" || !number.contains('.') {
        0
    } else {
        1
    };
    format!("Uploading · {sent_value:.decimals$} / {total} · {percent}%")
}

pub fn attachment_display_label(attachment: &AttachmentSummary) -> String {
    let prefix = attachment_kind_tag(&attachment.kind);
    match attachment_size_text(attachment.size_bytes) {
        Some(size) => format!("[{prefix}] {} ({size})", attachment.name),
        None => format!("[{prefix}] {}", attachment.name),
    }
}

fn attachment_is_animated_gif(attachment: &AttachmentSummary) -> bool {
    attachment.name.to_ascii_lowercase().ends_with(".gif")
        || attachment
            .mime_type
            .as_deref()
            .is_some_and(|mime| mime.eq_ignore_ascii_case("image/gif"))
}

fn is_unrenderable_keybase_asset_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://127.0.0.1:") || lower.starts_with("http://localhost:") {
        return true;
    }
    let prefix = if lower.starts_with("https://s3.amazonaws.com/") {
        "https://s3.amazonaws.com/"
    } else if lower.starts_with("http://s3.amazonaws.com/") {
        "http://s3.amazonaws.com/"
    } else {
        return false;
    };
    if lower.contains("x-amz-signature=") || lower.contains("awsaccesskeyid=") {
        return false;
    }
    let first_segment = url[prefix.len()..].split('/').next().unwrap_or_default();
    first_segment.len() == 64 && first_segment.chars().all(|ch| ch.is_ascii_hexdigit())
}

/// `views::normalize_local_source_path`.
pub fn normalize_local_source_path(raw: &str) -> String {
    let value = raw.trim();
    let Some(path) = value.strip_prefix("file://") else {
        return value.to_string();
    };
    let normalized = match path.strip_prefix("localhost/") {
        Some(rest) => format!("/{rest}"),
        None => path.to_string(),
    };
    let bytes = normalized.as_bytes();
    let hex = |ch: u8| (ch as char).to_digit(16).map(|digit| digit as u8);
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            out.push(hi << 4 | lo);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A drawable image location for an attachment source, as `attachment_source_to_image_source`.
fn renderable(source: &AttachmentSource) -> Option<String> {
    match source {
        AttachmentSource::Url(url) => {
            (!is_unrenderable_keybase_asset_url(url)).then(|| url.clone())
        }
        AttachmentSource::LocalPath(path) => {
            let normalized = normalize_local_source_path(path);
            (!normalized.is_empty()).then_some(normalized)
        }
    }
}

fn attachment_image_source(attachment: &AttachmentSummary) -> Option<String> {
    let preview = attachment.preview.as_ref().map(|preview| &preview.source);
    let source = if attachment_is_animated_gif(attachment) {
        attachment.source.as_ref().or(preview)
    } else {
        preview.or(attachment.source.as_ref())
    };
    source.and_then(renderable)
}

/// The full-size source the lightbox shows (`attachment_lightbox_source`).
pub fn attachment_lightbox_source(attachment: &AttachmentSummary) -> Option<AttachmentSource> {
    attachment
        .source
        .as_ref()
        .or_else(|| attachment.preview.as_ref().map(|preview| &preview.source))
        .filter(|source| renderable(source).is_some())
        .cloned()
}

/// `downloads::source`: the local copy, or a URL to fetch.
pub fn download_source(attachment: &AttachmentSummary) -> Option<String> {
    let source = attachment
        .source
        .as_ref()
        .or_else(|| attachment.preview.as_ref().map(|preview| &preview.source))?;
    match source {
        AttachmentSource::LocalPath(path) if !path.trim().is_empty() => Some(path.clone()),
        AttachmentSource::LocalPath(_) => None,
        AttachmentSource::Url(url) => {
            (!is_unrenderable_keybase_asset_url(url)).then(|| url.clone())
        }
    }
}

fn format_duration_ms(ms: u64) -> String {
    let total_seconds = ms / 1000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    if minutes >= 60 {
        format!("{}:{:02}:{seconds:02}", minutes / 60, minutes % 60)
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn attachment_view(
    attachment: &AttachmentSummary,
    upload: Option<UploadProgress>,
) -> AttachmentView {
    let image = matches!(
        attachment.kind,
        AttachmentKind::Image | AttachmentKind::Video
    )
    .then(|| attachment_image_source(attachment))
    .flatten();
    let preview = attachment.preview.as_ref();
    AttachmentView {
        kind: attachment.kind.clone(),
        name: attachment.name.clone(),
        size_label: attachment_size_text(attachment.size_bytes),
        display_label: attachment_display_label(attachment),
        animated: attachment_is_animated_gif(attachment),
        preview_width: preview
            .and_then(|preview| preview.width)
            .or(attachment.width),
        preview_height: preview
            .and_then(|preview| preview.height)
            .or(attachment.height),
        full_width: attachment
            .width
            .or(preview.and_then(|preview| preview.width)),
        full_height: attachment
            .height
            .or(preview.and_then(|preview| preview.height)),
        lightbox: attachment.kind == AttachmentKind::Image
            && image.is_some()
            && attachment_lightbox_source(attachment).is_some(),
        video_path: (attachment.kind == AttachmentKind::Video)
            .then(|| match &attachment.source {
                Some(AttachmentSource::LocalPath(path)) => Some(path.clone()),
                _ => None,
            })
            .flatten(),
        duration_label: attachment.duration_ms.map(format_duration_ms),
        image,
        download_source: download_source(attachment),
        upload: upload.map(|progress| UploadView {
            label: upload_status_label(
                progress.sent_bytes,
                if progress.total_bytes > 0 {
                    progress.total_bytes
                } else {
                    attachment.size_bytes
                },
            ),
            fraction: progress.fraction(),
        }),
    }
}

/// "What are the limits? Any details?" style preview of a pinned message:
/// `compact_pinned_preview_text` (views/conversation).
pub fn compact_pinned_preview_text(raw: &str) -> String {
    let mut lines = raw.lines().map(str::trim).filter(|line| !line.is_empty());
    let preview_lines = lines.by_ref().take(2).collect::<Vec<_>>();
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
