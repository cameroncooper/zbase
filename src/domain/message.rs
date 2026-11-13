use crate::domain::{
    attachment::AttachmentSummary,
    backend::BackendId,
    github_pr::GithubPrPreview,
    ids::{ConversationId, MessageId, UserId},
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkPreview {
    pub url: String,
    pub video_url: Option<String>,
    pub title: Option<String>,
    pub site: Option<String>,
    pub description: Option<String>,
    pub thumbnail_asset: Option<String>,
    pub is_media: bool,
    pub media_width: Option<u32>,
    pub media_height: Option<u32>,
    pub is_video: bool,
    /// Live PR status for GitHub pull request links. Never persisted.
    pub github_pr: Option<GithubPrPreview>,
    /// The linked message for `keybase://chat/...` links. Never persisted.
    pub chat_message: Option<ChatMessagePreview>,
    /// The viewer folded this media preview away. Never persisted.
    pub collapsed: bool,
}

/// A quote of the message a `keybase://chat/...` link points to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatMessagePreview {
    pub author: String,
    pub author_avatar: Option<String>,
    /// Where the message lives, e.g. "#general · chia_network".
    pub location: String,
    pub excerpt: String,
    pub timestamp_ms: Option<i64>,
}

#[derive(Clone, Debug)]
pub enum BroadcastKind {
    Here,
    All,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EmojiSourceRef {
    pub backend_id: BackendId,
    pub ref_key: String,
}

impl EmojiSourceRef {
    pub fn cache_key(&self) -> String {
        format!("{}:{}", self.backend_id.0, self.ref_key)
    }
}

#[derive(Clone, Debug)]
pub enum MessageFragment {
    Text(String),
    InlineCode(String),
    Emoji {
        alias: String,
        source_ref: Option<EmojiSourceRef>,
    },
    Mention(UserId),
    ChannelMention {
        name: String,
    },
    BroadcastMention(BroadcastKind),
    Link {
        url: String,
        display: String,
    },
    Code {
        text: String,
        lang: Option<String>,
    },
    Quote(String),
}

#[derive(Clone, Debug)]
pub enum ChatEvent {
    MemberJoined,
    MemberLeft,
    MembersAdded {
        user_ids: Vec<UserId>,
        role: Option<String>,
    },
    MembersRemoved {
        user_ids: Vec<UserId>,
    },
    DescriptionChanged {
        description: Option<String>,
    },
    ChannelRenamed {
        new_name: String,
    },
    AvatarChanged,
    MessagePinned {
        target_message_id: Option<MessageId>,
    },
    MessageDeleted {
        target_message_id: Option<MessageId>,
    },
    HistoryCleared,
    RetentionChanged {
        summary: String,
    },
    ChannelCreated {
        channel_name: String,
        conv_id: Option<ConversationId>,
    },
    Other {
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageSendState {
    Sent,
    Pending,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MessageReaction {
    pub emoji: String,
    pub source_ref: Option<EmojiSourceRef>,
    pub actor_ids: Vec<UserId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditMeta {
    pub edit_id: MessageId,
    pub edited_at_ms: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct MessageRecord {
    pub id: MessageId,
    pub conversation_id: ConversationId,
    pub author_id: UserId,
    pub reply_to: Option<MessageId>,
    pub thread_root_id: Option<MessageId>,
    pub timestamp_ms: Option<i64>,
    pub event: Option<ChatEvent>,
    pub link_previews: Vec<LinkPreview>,
    pub permalink: String,
    pub fragments: Vec<MessageFragment>,
    pub source_text: Option<String>,
    pub attachments: Vec<AttachmentSummary>,
    pub reactions: Vec<MessageReaction>,
    pub thread_reply_count: u32,
    pub send_state: MessageSendState,
    pub edited: Option<EditMeta>,
}

/// Server edits have their own revision; the original send timestamp does not
/// change when a message is edited.
pub fn message_revision(message: &MessageRecord) -> (bool, u64, i64) {
    (
        message.edited.is_some(),
        message
            .edited
            .as_ref()
            .and_then(|edit| edit.edit_id.0.parse::<u64>().ok())
            .unwrap_or(0),
        message
            .edited
            .as_ref()
            .and_then(|edit| edit.edited_at_ms)
            .unwrap_or(0),
    )
}

/// A completed download can enrich only the matching attachment, never the text,
/// reactions, thread state, or existence of its original message snapshot.
/// Downloaded paths replace older local paths, including thumbnails and missing files.
pub fn apply_attachment_paths(
    message: &mut MessageRecord,
    downloaded: &[crate::domain::attachment::AttachmentSummary],
) {
    merge_attachment_paths(message, downloaded, true);
}

fn merge_attachment_paths(
    message: &mut MessageRecord,
    downloaded: &[crate::domain::attachment::AttachmentSummary],
    replace_local: bool,
) {
    use crate::domain::attachment::AttachmentSource;
    let local = |source: Option<&AttachmentSource>| matches!(source, Some(AttachmentSource::LocalPath(path)) if !path.trim().is_empty());
    for attachment in &mut message.attachments {
        let Some(patch) = downloaded.iter().find(|patch| {
            patch.name == attachment.name
                && patch.kind == attachment.kind
                && patch.size_bytes == attachment.size_bytes
        }) else {
            continue;
        };
        if (replace_local || !local(attachment.source.as_ref())) && local(patch.source.as_ref()) {
            attachment.source = patch.source.clone();
        }
        if (replace_local || !local(attachment.preview.as_ref().map(|preview| &preview.source)))
            && local(patch.preview.as_ref().map(|preview| &preview.source))
        {
            attachment.preview = patch.preview.clone();
        }
    }
}

/// A Keybase edit of an attachment carries only the new caption, and an edit
/// cannot remove the attachment, so the edit keeps the original's attachments.
pub fn keep_original_attachments(edit: &mut MessageRecord, original: &MessageRecord) {
    if edit.edited.is_some() && edit.attachments.is_empty() {
        edit.attachments = original.attachments.clone();
    }
}

pub fn merge_message_version(existing: &MessageRecord, incoming: &MessageRecord) -> MessageRecord {
    if message_revision(incoming) < message_revision(existing) {
        let mut kept = existing.clone();
        keep_original_attachments(&mut kept, incoming);
        return kept;
    }
    let mut merged = incoming.clone();
    keep_original_attachments(&mut merged, existing);
    if incoming.edited.is_some() {
        // An edit operation has its own timestamp and may omit the original
        // reply header. Keep the original message's position and thread identity.
        merged.timestamp_ms = existing.timestamp_ms;
        merged.reply_to = existing.reply_to.clone().or(merged.reply_to);
        merged.thread_root_id = existing.thread_root_id.clone().or(merged.thread_root_id);
    }
    // Server snapshots retain their own local paths; cached data only fills gaps.
    merge_attachment_paths(&mut merged, &existing.attachments, false);
    merged
}
