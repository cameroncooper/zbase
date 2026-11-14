//! The newest message of a conversation, as inbox rows preview it: who sent it,
//! when, and what it was, without the rest of the message record.
use crate::domain::{
    attachment::AttachmentKind,
    ids::{MessageId, UserId},
    message::{ChatEvent, MessageFragment, MessageRecord},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessagePreview {
    /// Server id, or the `local:` id of a message still being sent.
    pub message_id: Option<MessageId>,
    /// Sender, when known. Keybase's own snippets carry none.
    pub author_id: Option<UserId>,
    pub timestamp_ms: i64,
    pub body: PreviewBody,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewBody {
    /// Message source text: Markdown and `:shortcode:` emoji, not yet cleaned.
    Text(String),
    /// A shared file. `caption` is the title the sender gave it, if any.
    Attachment {
        kind: AttachmentKind,
        name: String,
        caption: Option<String>,
    },
    /// Something the sender did, phrased to follow their name ("joined the channel").
    Event(String),
    /// The previewed message was deleted and nothing older is known.
    Deleted,
    /// Text Keybase formatted itself (inbox notifications): shown as is, since it
    /// may already start with the sender's name.
    Snippet(String),
}

impl MessagePreview {
    /// Preview of `message`, or `None` for records that never stand for a
    /// conversation's latest activity (deletion markers, empty bodies).
    pub fn from_message(message: &MessageRecord) -> Option<Self> {
        let body = match &message.event {
            Some(event) => PreviewBody::Event(event_phrase(event)?),
            None => message_body(message)?,
        };
        Some(Self {
            message_id: Some(message.id.clone()),
            author_id: Some(message.author_id.clone()),
            timestamp_ms: message.timestamp_ms.unwrap_or_default(),
            body,
        })
    }

    /// Whether `self` describes a later message than `other`, or a newer
    /// version (an edit) of the same one.
    pub fn supersedes(&self, other: &Self) -> bool {
        if other.body == PreviewBody::Deleted {
            return true;
        }
        match (&self.message_id, &other.message_id) {
            (Some(mine), Some(theirs)) if mine == theirs => {
                // Same message: keep the richer, structured version.
                !matches!(self.body, PreviewBody::Snippet(_))
                    || matches!(other.body, PreviewBody::Snippet(_))
            }
            (Some(mine), Some(theirs)) => match (numeric_id(mine), numeric_id(theirs)) {
                (Some(mine), Some(theirs)) => mine > theirs,
                _ => self.timestamp_ms >= other.timestamp_ms,
            },
            _ => self.timestamp_ms >= other.timestamp_ms,
        }
    }

    /// The newer of two optional previews; an update without a preview never
    /// erases a known one.
    pub fn newest(current: Option<Self>, incoming: Option<Self>) -> Option<Self> {
        match (current, incoming) {
            (Some(current), Some(incoming)) => Some(if incoming.supersedes(&current) {
                incoming
            } else {
                current
            }),
            (current, incoming) => incoming.or(current),
        }
    }
}

fn numeric_id(id: &MessageId) -> Option<u64> {
    id.0.parse().ok()
}

fn message_body(message: &MessageRecord) -> Option<PreviewBody> {
    let text = message_text(message);
    if let Some(attachment) = message.attachments.first() {
        // Keybase fills an uncaptioned file's body with a stand-in; only a real
        // title is worth showing in place of the file name.
        let caption = Some(text)
            .filter(|text| {
                !text.is_empty()
                    && *text != attachment.name
                    && !text.starts_with("Attachment: ")
                    && !text.ends_with(" attachments")
            })
            .map(clip);
        return Some(PreviewBody::Attachment {
            kind: attachment.kind.clone(),
            name: attachment.name.trim().to_string(),
            caption,
        });
    }
    (!text.is_empty()).then(|| PreviewBody::Text(clip(text)))
}

/// Previews show one line, so a long message (a pasted log, a code block)
/// keeps only its start. Cut on a char boundary.
const PREVIEW_TEXT_MAX_CHARS: usize = 280;

fn clip(mut text: String) -> String {
    if let Some((cut, _)) = text.char_indices().nth(PREVIEW_TEXT_MAX_CHARS) {
        text.truncate(cut);
    }
    text
}

/// Source text when the record kept it, otherwise rebuilt from its fragments.
fn message_text(message: &MessageRecord) -> String {
    if let Some(source) = message
        .source_text
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        return source.to_string();
    }
    let mut text = String::new();
    for fragment in &message.fragments {
        match fragment {
            MessageFragment::Text(value) | MessageFragment::Quote(value) => text.push_str(value),
            MessageFragment::InlineCode(code) => text.push_str(code),
            MessageFragment::Code { text: code, .. } => text.push_str(code),
            MessageFragment::Emoji { alias, .. } => {
                text.push(':');
                text.push_str(alias.trim_matches(':'));
                text.push(':');
            }
            MessageFragment::Mention(user) => {
                text.push('@');
                text.push_str(&user.0);
            }
            MessageFragment::ChannelMention { name } => {
                text.push('#');
                text.push_str(name);
            }
            MessageFragment::BroadcastMention(kind) => text.push_str(match kind {
                crate::domain::message::BroadcastKind::Here => "@here",
                crate::domain::message::BroadcastKind::All => "@channel",
            }),
            MessageFragment::Link { display, url } => {
                text.push_str(if display.is_empty() { url } else { display });
            }
        }
    }
    text.trim().to_string()
}

/// What a system event says after the actor's name, or `None` for events that
/// only change other messages (deletions), which a preview skips over.
fn event_phrase(event: &ChatEvent) -> Option<String> {
    Some(match event {
        ChatEvent::MemberJoined => "joined the conversation".to_string(),
        ChatEvent::MemberLeft => "left the conversation".to_string(),
        ChatEvent::MembersAdded { user_ids, .. } => match user_ids.as_slice() {
            [one] => format!("added @{}", one.0),
            [] => "added members".to_string(),
            many => format!("added {} people", many.len()),
        },
        ChatEvent::MembersRemoved { user_ids } => match user_ids.as_slice() {
            [one] => format!("removed @{}", one.0),
            _ => "removed members".to_string(),
        },
        ChatEvent::DescriptionChanged { .. } => "changed the description".to_string(),
        ChatEvent::ChannelRenamed { new_name } => format!("renamed the channel to #{new_name}"),
        ChatEvent::AvatarChanged => "changed the avatar".to_string(),
        ChatEvent::MessagePinned { .. } => "pinned a message".to_string(),
        ChatEvent::HistoryCleared => "cleared the history".to_string(),
        ChatEvent::RetentionChanged { summary } => summary.trim().to_string(),
        ChatEvent::ChannelCreated { channel_name, .. } => format!("created #{channel_name}"),
        ChatEvent::Other { text } => text.trim().to_string(),
        ChatEvent::MessageDeleted { .. } => return None,
    })
    .filter(|phrase| !phrase.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        attachment::AttachmentSummary, ids::ConversationId, message::MessageSendState,
    };

    fn record(id: &str, text: &str) -> MessageRecord {
        MessageRecord {
            bot_username: None,
            id: MessageId::new(id),
            conversation_id: ConversationId::new("c"),
            author_id: UserId::new("lena_f"),
            reply_to: None,
            thread_root_id: None,
            timestamp_ms: Some(1_000),
            event: None,
            link_previews: Vec::new(),
            permalink: String::new(),
            fragments: Vec::new(),
            source_text: Some(text.to_string()),
            attachments: Vec::new(),
            reactions: Vec::new(),
            thread_reply_count: 0,
            send_state: MessageSendState::Sent,
            edited: None,
        }
    }

    fn text_preview(id: &str, at: i64) -> MessagePreview {
        MessagePreview {
            message_id: Some(MessageId::new(id)),
            author_id: None,
            timestamp_ms: at,
            body: PreviewBody::Text(id.to_string()),
        }
    }

    #[test]
    fn attachments_keep_a_real_caption_and_drop_keybase_stand_ins() {
        let mut message = record("1", "Attachment: notes.pdf");
        message.attachments.push(AttachmentSummary {
            name: "notes.pdf".into(),
            ..AttachmentSummary::default()
        });
        let caption =
            |message: &MessageRecord| match MessagePreview::from_message(message).unwrap().body {
                PreviewBody::Attachment { caption, name, .. } => (name, caption),
                other => panic!("not an attachment: {other:?}"),
            };
        assert_eq!(caption(&message), ("notes.pdf".into(), None));
        message.source_text = Some("Q3 plan".into());
        assert_eq!(
            caption(&message),
            ("notes.pdf".into(), Some("Q3 plan".into()))
        );
    }

    #[test]
    fn text_falls_back_to_fragments_and_empty_or_deleted_records_are_skipped() {
        let mut message = record("1", "");
        assert_eq!(MessagePreview::from_message(&message), None);
        message.fragments = vec![
            MessageFragment::Mention(UserId::new("sam")),
            MessageFragment::Text(" ship it ".into()),
            MessageFragment::Emoji {
                alias: "rocket".into(),
                source_ref: None,
            },
        ];
        assert_eq!(
            MessagePreview::from_message(&message).unwrap().body,
            PreviewBody::Text("@sam ship it :rocket:".into())
        );
        message.event = Some(ChatEvent::MessageDeleted {
            target_message_id: None,
        });
        assert_eq!(MessagePreview::from_message(&message), None);
        message.event = Some(ChatEvent::MemberJoined);
        assert_eq!(
            MessagePreview::from_message(&message).unwrap().body,
            PreviewBody::Event("joined the conversation".into())
        );
    }

    #[test]
    fn long_messages_keep_only_their_start() {
        let long = "é".repeat(PREVIEW_TEXT_MAX_CHARS + 50);
        let PreviewBody::Text(text) = MessagePreview::from_message(&record("1", &long))
            .unwrap()
            .body
        else {
            panic!("expected text");
        };
        assert_eq!(text.chars().count(), PREVIEW_TEXT_MAX_CHARS);
    }

    #[test]
    fn newer_ids_win_and_edits_replace_the_same_message() {
        let older = text_preview("41", 5_000);
        let newer = text_preview("42", 1_000);
        // Ids order messages even when clocks disagree.
        assert!(newer.supersedes(&older));
        assert!(!older.supersedes(&newer));
        let edited = MessagePreview {
            body: PreviewBody::Text("fixed typo".into()),
            ..newer.clone()
        };
        assert_eq!(
            MessagePreview::newest(Some(newer.clone()), Some(edited.clone())),
            Some(edited.clone())
        );
        // An edit of an older message keeps that older id and loses.
        assert_eq!(
            MessagePreview::newest(Some(edited.clone()), Some(older.clone())),
            Some(edited.clone())
        );
        // A Keybase snippet for the same message never replaces the structured one.
        let snippet = MessagePreview {
            body: PreviewBody::Snippet("lena: fixed typo".into()),
            ..edited.clone()
        };
        assert!(!snippet.supersedes(&edited));
        assert_eq!(
            MessagePreview::newest(Some(edited.clone()), None),
            Some(edited)
        );
        // A pending local send is ordered by time.
        let pending = text_preview("local:abc", 6_000);
        assert!(pending.supersedes(&newer));
        // Anything replaces a "deleted" placeholder.
        let deleted = MessagePreview {
            body: PreviewBody::Deleted,
            ..newer
        };
        assert!(older.supersedes(&deleted));
    }
}
