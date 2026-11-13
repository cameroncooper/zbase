#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutocompleteTriggerKind {
    Mention,
    BotCommand,
    Emoji,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerMatch {
    pub kind: AutocompleteTriggerKind,
    pub trigger_offset: usize,
    pub query: String,
}

const MIN_EMOJI_QUERY_LEN: usize = 2;

pub fn detect_autocomplete_trigger(text: &str, cursor: usize) -> Option<TriggerMatch> {
    let cursor = cursor.min(text.len());
    if cursor_is_in_code(text, cursor) {
        return None;
    }
    let before_cursor = &text[..cursor];
    for (offset, ch) in before_cursor.char_indices().rev() {
        if ch.is_whitespace() {
            return None;
        }
        if ch != '@' && ch != ':' && ch != '!' {
            continue;
        }
        if offset > 0 {
            let prev = text[..offset].chars().next_back();
            if prev.is_some_and(|prev| !prev.is_whitespace()) {
                return None;
            }
        }
        let query = text[offset + ch.len_utf8()..cursor].to_string();
        if ch == '!' {
            // Keybase bot commands start the message, never a word in prose.
            if !text[..offset].trim().is_empty() {
                return None;
            }
            return Some(TriggerMatch {
                kind: AutocompleteTriggerKind::BotCommand,
                trigger_offset: offset,
                query,
            });
        }
        if ch == '@' {
            return Some(TriggerMatch {
                kind: AutocompleteTriggerKind::Mention,
                trigger_offset: offset,
                query,
            });
        }
        if query.len() < MIN_EMOJI_QUERY_LEN || !emoji_query_valid(&query) {
            return None;
        }
        return Some(TriggerMatch {
            kind: AutocompleteTriggerKind::Emoji,
            trigger_offset: offset,
            query,
        });
    }
    None
}

/// The edit that accepts a completion: the byte range to replace and its
/// replacement. The range runs from the trigger through the rest of the word
/// under the caret, so completing `@al|ice` leaves no `ice` behind.
/// `completion` ends in a space; it is dropped when whitespace already
/// follows the word.
pub fn completion_edit(
    text: &str,
    trigger_offset: usize,
    cursor: usize,
    completion: &str,
) -> (std::ops::Range<usize>, String) {
    let cursor = cursor.min(text.len());
    let start = trigger_offset.min(cursor);
    let after = &text[cursor..];
    let end = cursor + after.find(char::is_whitespace).unwrap_or(after.len());
    let replacement = if text[end..].starts_with(char::is_whitespace) {
        completion.trim_end()
    } else {
        completion
    };
    (start..end, replacement.to_string())
}

pub fn cursor_is_in_code(text: &str, cursor: usize) -> bool {
    let cursor = cursor.min(text.len());
    let before_cursor = &text[..cursor];
    if in_fenced_code_block(before_cursor) {
        return true;
    }
    let line_start = before_cursor.rfind('\n').map_or(0, |index| index + 1);
    let current_line = &before_cursor[line_start..];
    in_inline_code(current_line)
}

fn in_fenced_code_block(before_cursor: &str) -> bool {
    let mut in_fence = false;
    for line in before_cursor.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
        }
    }
    in_fence
}

fn in_inline_code(current_line: &str) -> bool {
    let mut in_code = false;
    let mut index = 0usize;
    let bytes = current_line.as_bytes();
    while index < bytes.len() {
        if bytes[index] != b'`' {
            index += 1;
            continue;
        }
        while index < bytes.len() && bytes[index] == b'`' {
            index += 1;
        }
        in_code = !in_code;
    }
    in_code
}

fn emoji_query_valid(query: &str) -> bool {
    query
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '+' || ch == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_commands_only_complete_at_the_start_outside_code() {
        assert_eq!(
            detect_autocomplete_trigger("!", 1).unwrap().kind,
            AutocompleteTriggerKind::BotCommand
        );
        assert_eq!(detect_autocomplete_trigger("!zo", 3).unwrap().query, "zo");
        for text in [
            "hello !zoom",
            "wow!zoom",
            "`!zoom",
            "```\n!zoom",
            "!zoom argument",
        ] {
            assert!(
                detect_autocomplete_trigger(text, text.len()).is_none(),
                "{text}"
            );
        }
    }
    #[test]
    fn detects_mention_trigger() {
        let matched = detect_autocomplete_trigger("hello @cam", "hello @cam".len());
        assert_eq!(
            matched,
            Some(TriggerMatch {
                kind: AutocompleteTriggerKind::Mention,
                trigger_offset: 6,
                query: "cam".to_string()
            })
        );
    }

    #[test]
    fn detects_emoji_trigger_with_min_length() {
        assert!(detect_autocomplete_trigger(":s", 2).is_none());
        let matched = detect_autocomplete_trigger(":sm", 3);
        assert_eq!(
            matched,
            Some(TriggerMatch {
                kind: AutocompleteTriggerKind::Emoji,
                trigger_offset: 0,
                query: "sm".to_string()
            })
        );
    }

    fn complete(text_with_caret: &str, completion: &str) -> String {
        let cursor = text_with_caret.find('|').unwrap();
        let text = text_with_caret.replace('|', "");
        let trigger = detect_autocomplete_trigger(&text, cursor).unwrap();
        let (range, replacement) =
            completion_edit(&text, trigger.trigger_offset, cursor, completion);
        let mut out = text.clone();
        out.replace_range(range, &replacement);
        out
    }

    #[test]
    fn completion_replaces_the_whole_word_under_the_caret() {
        assert_eq!(complete("hi @al|", "@alice "), "hi @alice ");
        assert_eq!(complete("hi @al|ice", "@alice "), "hi @alice ");
        assert_eq!(complete("@al|ix!", "@alice "), "@alice ");
    }

    #[test]
    fn completion_does_not_double_an_existing_space() {
        assert_eq!(complete("@al| said hi", "@alice "), "@alice said hi");
        assert_eq!(complete("ok :smi|\nnext", "😄 "), "ok 😄\nnext");
    }

    #[test]
    fn suppresses_inside_inline_code() {
        let text = "typing `@cam` now";
        let cursor = text.find("@cam").unwrap_or_default() + 4;
        assert!(cursor_is_in_code(text, cursor));
        assert!(detect_autocomplete_trigger(text, cursor).is_none());
    }

    #[test]
    fn suppresses_inside_fenced_code_block() {
        let text = "```rust\nlet a = @cam;\n";
        assert!(cursor_is_in_code(text, text.len()));
        assert!(detect_autocomplete_trigger(text, text.len()).is_none());
    }
}
