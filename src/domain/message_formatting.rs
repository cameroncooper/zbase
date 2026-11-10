use super::message::MessageFragment;

/// Local and server messages share code/quote structure before server decorations arrive.
pub fn parse_message_text(body: &str) -> Vec<MessageFragment> {
    let mut fragments = Vec::new();
    push_text_fragment(&mut fragments, body);
    fragments
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub enum CodeLanguage {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Json,
    Yaml,
    Toml,
    Bash,
    Sql,
    Go,
}

impl CodeLanguage {
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag.trim().to_ascii_lowercase().as_str() {
            "rs" | "rust" => Some(Self::Rust),
            "py" | "python" => Some(Self::Python),
            "js" | "javascript" => Some(Self::JavaScript),
            "ts" | "typescript" => Some(Self::TypeScript),
            "json" => Some(Self::Json),
            "yml" | "yaml" => Some(Self::Yaml),
            "toml" => Some(Self::Toml),
            "sh" | "bash" | "shell" | "zsh" => Some(Self::Bash),
            "sql" => Some(Self::Sql),
            "go" | "golang" => Some(Self::Go),
            _ => None,
        }
    }
}

pub(crate) fn parse_code_span_end(body: &str, backtick_index: usize) -> Option<usize> {
    parse_code_fragment_at(body, backtick_index).map(|(_, span_end)| span_end)
}

fn parse_code_fragment_at(body: &str, backtick_index: usize) -> Option<(MessageFragment, usize)> {
    let value = &body[backtick_index..];
    if value.starts_with("```") {
        let start = backtick_index + 3;
        if start >= body.len() {
            return None;
        }
        let relative_end = body[start..].find("```")?;
        let end = start + relative_end;
        if end <= start {
            return None;
        }
        let fence_body = &body[start..end];
        let (lang, content_start) = if let Some(nl_rel) = fence_body.find('\n') {
            let info = fence_body[..nl_rel].trim();
            if info.is_empty() {
                (None, start + nl_rel + 1)
            } else {
                let candidate = info.split_whitespace().next().unwrap_or("");
                if CodeLanguage::from_tag(candidate).is_some() {
                    (Some(candidate.to_ascii_lowercase()), start + nl_rel + 1)
                } else {
                    (None, start)
                }
            }
        } else {
            (None, start)
        };
        let content_end = if end > content_start && body[..end].ends_with('\n') {
            end - 1
        } else {
            end
        };
        return Some((
            MessageFragment::Code {
                text: body[content_start..content_end].to_string(),
                lang,
            },
            end + 3,
        ));
    }

    let start = backtick_index + 1;
    if start >= body.len() {
        return None;
    }
    let relative_end = body[start..].find('`')?;
    let end = start + relative_end;
    if end <= start {
        return None;
    }
    Some((
        MessageFragment::InlineCode(body[start..end].to_string()),
        end + 1,
    ))
}

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize)]
enum PendingKind {
    Text,
    Quote,
}

pub(crate) fn push_text_fragment(fragments: &mut Vec<MessageFragment>, text: &str) {
    if text.is_empty() {
        return;
    }
    // Quote lines are literal: do not parse inline code fragments on quote lines.
    let mut cursor = 0usize;
    let mut continued_quote_line = quote_line_continues_from_fragments(fragments);
    while cursor < text.len() {
        let line_end = text[cursor..]
            .find('\n')
            .map(|relative| cursor + relative)
            .unwrap_or(text.len());
        let has_newline = line_end < text.len();
        let line = &text[cursor..line_end];

        let is_quote_line = strip_quote_prefix(line).is_some() || continued_quote_line;
        if is_quote_line {
            // Treat the entire line literally; quoting is handled in push_plain_text_fragment.
            push_plain_text_fragment(fragments, line);
        } else {
            // Non-quote line: allow inline code parsing.
            let mut inner_cursor = cursor;
            while inner_cursor < line_end {
                let Some(relative_start) = text[inner_cursor..line_end].find('`') else {
                    push_plain_text_fragment(fragments, &text[inner_cursor..line_end]);
                    break;
                };
                let marker_start = inner_cursor + relative_start;
                push_plain_text_fragment(fragments, &text[inner_cursor..marker_start]);

                if let Some((fragment, span_end)) = parse_code_fragment_at(text, marker_start) {
                    fragments.push(fragment);
                    inner_cursor = span_end;
                    continue;
                }

                push_plain_text_fragment(fragments, &text[marker_start..marker_start + 1]);
                inner_cursor = marker_start + 1;
            }

            if inner_cursor > line_end {
                cursor = inner_cursor;
                continue;
            }
        }

        if has_newline {
            // Preserve newline via plain fragment, which also resets quote-continuation state.
            push_plain_text_fragment(fragments, "\n");
            continued_quote_line = false;
            cursor = line_end + 1;
        } else {
            break;
        }
    }
}

pub(crate) fn push_plain_text_fragment(fragments: &mut Vec<MessageFragment>, text: &str) {
    if text.is_empty() {
        return;
    }
    let mut continued_quote_line = quote_line_continues_from_fragments(fragments);
    if !continued_quote_line && !text.contains('>') {
        push_raw_text_fragment(fragments, text);
        return;
    }

    let mut pending_kind: Option<PendingKind> = None;
    let mut pending = String::new();
    let mut cursor = 0usize;
    while cursor < text.len() {
        let line_end = text[cursor..]
            .find('\n')
            .map(|relative| cursor + relative)
            .unwrap_or(text.len());
        let has_newline = line_end < text.len();
        let line = &text[cursor..line_end];
        let (kind, value) = if let Some(quote) = strip_quote_prefix(line) {
            (PendingKind::Quote, quote)
        } else if continued_quote_line {
            (PendingKind::Quote, line)
        } else {
            (PendingKind::Text, line)
        };

        let should_flush = match (&pending_kind, &kind) {
            (Some(PendingKind::Text), PendingKind::Text)
            | (Some(PendingKind::Quote), PendingKind::Quote) => false,
            (Some(_), _) => true,
            (None, _) => false,
        };
        if should_flush {
            flush_pending_plain_or_quote(fragments, pending_kind.take(), &mut pending);
        }
        pending_kind = Some(kind);
        pending.push_str(value);
        if has_newline {
            pending.push('\n');
            continued_quote_line = false;
            cursor = line_end + 1;
        } else {
            break;
        }
    }
    flush_pending_plain_or_quote(fragments, pending_kind, &mut pending);
}

fn quote_line_continues_from_fragments(fragments: &[MessageFragment]) -> bool {
    for fragment in fragments.iter().rev() {
        match fragment {
            MessageFragment::Quote(text) => return !text.ends_with('\n'),
            MessageFragment::Text(_) => return false,
            MessageFragment::Code { text, .. } | MessageFragment::InlineCode(text) => {
                if text.contains('\n') {
                    return false;
                }
            }
            MessageFragment::Emoji { .. }
            | MessageFragment::Mention(_)
            | MessageFragment::ChannelMention { .. }
            | MessageFragment::BroadcastMention(_)
            | MessageFragment::Link { .. } => {}
        }
    }
    false
}

pub(crate) fn strip_quote_prefix(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix("> ") {
        return Some(rest);
    }
    line.strip_prefix('>')
}

fn flush_pending_plain_or_quote(
    fragments: &mut Vec<MessageFragment>,
    kind: Option<PendingKind>,
    pending: &mut String,
) {
    if pending.is_empty() {
        return;
    }
    let Some(kind) = kind else {
        pending.clear();
        return;
    };
    let content = std::mem::take(pending);
    match kind {
        PendingKind::Text => push_raw_text_fragment(fragments, &content),
        PendingKind::Quote => {
            if let Some(MessageFragment::Quote(existing)) = fragments.last_mut() {
                existing.push_str(&content);
            } else {
                fragments.push(MessageFragment::Quote(content));
            }
        }
    }
}

fn push_raw_text_fragment(fragments: &mut Vec<MessageFragment>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(MessageFragment::Text(existing)) = fragments.last_mut() {
        existing.push_str(text);
    } else {
        fragments.push(MessageFragment::Text(text.to_string()));
    }
}
