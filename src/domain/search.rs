use crate::domain::{
    ids::ConversationId,
    message::{MessageFragment, MessageRecord},
    route::Route,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchFilter {
    FromUser(String),
    InChannel(String),
    HasFile,
    HasLink,
    MentionsMe,
}

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub conversation_id: ConversationId,
    pub route: Route,
    pub snippet: String,
    pub snippet_highlight_ranges: Vec<(usize, usize)>,
    pub message: MessageRecord,
}

/// The one order for message hits, shared by Search and the quick switcher:
/// newest first. Keybase's search has no relevance score to rank by, so
/// recency is the whole policy; ties fall back to the message id (newer ids
/// first) and then the conversation, so the order never depends on arrival.
pub fn compare_message_hits(
    left_timestamp_ms: Option<i64>,
    left_message_id: &str,
    left_conversation: &str,
    right_timestamp_ms: Option<i64>,
    right_message_id: &str,
    right_conversation: &str,
) -> std::cmp::Ordering {
    let numeric = |id: &str| id.parse::<u64>().ok();
    right_timestamp_ms
        .unwrap_or(i64::MIN)
        .cmp(&left_timestamp_ms.unwrap_or(i64::MIN))
        .then_with(
            || match (numeric(left_message_id), numeric(right_message_id)) {
                (Some(left), Some(right)) => right.cmp(&left),
                _ => right_message_id.cmp(left_message_id),
            },
        )
        .then_with(|| left_conversation.cmp(right_conversation))
}

impl SearchResult {
    /// [`compare_message_hits`] for two search results.
    pub fn cmp_for_display(&self, other: &Self) -> std::cmp::Ordering {
        compare_message_hits(
            self.message.timestamp_ms,
            &self.message.id.0,
            &self.conversation_id.0,
            other.message.timestamp_ms,
            &other.message.id.0,
            &other.conversation_id.0,
        )
    }
}

impl SearchFilter {
    /// Whether `message` passes this filter. `self_username` is the signed-in
    /// Keybase user, for "Mentions me". Channel scoping is decided by the search
    /// scope, not per message, so `InChannel` always passes here.
    pub fn matches(&self, message: &MessageRecord, self_username: Option<&str>) -> bool {
        match self {
            Self::FromUser(user) => message
                .author_id
                .0
                .to_ascii_lowercase()
                .contains(&user.trim().to_ascii_lowercase()),
            Self::InChannel(_) => true,
            Self::HasFile => !message.attachments.is_empty(),
            Self::HasLink => {
                !message.link_previews.is_empty()
                    || message
                        .fragments
                        .iter()
                        .any(|fragment| matches!(fragment, MessageFragment::Link { .. }))
            }
            Self::MentionsMe => message.fragments.iter().any(|fragment| match fragment {
                MessageFragment::Mention(user) => {
                    self_username.is_some_and(|me| user.0.eq_ignore_ascii_case(me))
                }
                MessageFragment::BroadcastMention(_) => true,
                _ => false,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        attachment::AttachmentSummary,
        ids::{MessageId, UserId},
        message::BroadcastKind,
    };

    fn message(author: &str, fragments: Vec<MessageFragment>) -> MessageRecord {
        MessageRecord {
            bot_username: None,
            id: MessageId::new("1"),
            conversation_id: ConversationId::new("c"),
            author_id: UserId::new(author),
            reply_to: None,
            thread_root_id: None,
            timestamp_ms: None,
            event: None,
            link_previews: Vec::new(),
            permalink: String::new(),
            fragments,
            source_text: None,
            attachments: Vec::new(),
            reactions: Vec::new(),
            thread_reply_count: 0,
            send_state: crate::domain::message::MessageSendState::Sent,
            edited: None,
        }
    }

    #[test]
    fn hits_sort_newest_first_with_stable_ties() {
        use std::cmp::Ordering;
        assert_eq!(
            compare_message_hits(Some(2), "5", "a", Some(1), "9", "a"),
            Ordering::Less,
            "the newer message comes first"
        );
        assert_eq!(
            compare_message_hits(Some(1), "10", "a", Some(1), "9", "a"),
            Ordering::Less
        );
        assert_eq!(
            compare_message_hits(Some(1), "9", "a", Some(1), "9", "b"),
            Ordering::Less
        );
        assert_eq!(
            compare_message_hits(None, "9", "a", Some(1), "1", "a"),
            Ordering::Greater
        );

        let mut hits = vec![
            SearchResult {
                conversation_id: ConversationId::new("c"),
                route: Route::WorkspaceHome {
                    workspace_id: crate::domain::ids::WorkspaceId::new("w"),
                },
                snippet: String::new(),
                snippet_highlight_ranges: Vec::new(),
                message: MessageRecord {
                    bot_username: None,
                    timestamp_ms: Some(100),
                    ..message("a", Vec::new())
                },
            };
            3
        ];
        hits[1].message.timestamp_ms = Some(300);
        hits[2].message.timestamp_ms = Some(200);
        hits.sort_by(SearchResult::cmp_for_display);
        let order: Vec<_> = hits.iter().map(|hit| hit.message.timestamp_ms).collect();
        assert_eq!(order, [Some(300), Some(200), Some(100)]);
    }

    #[test]
    fn mentions_me_matches_direct_and_broadcast_mentions_only() {
        let direct = message(
            "priya",
            vec![MessageFragment::Mention(UserId::new("Robin"))],
        );
        let other = message("priya", vec![MessageFragment::Mention(UserId::new("samr"))]);
        let broadcast = message(
            "priya",
            vec![MessageFragment::BroadcastMention(BroadcastKind::Here)],
        );
        assert!(SearchFilter::MentionsMe.matches(&direct, Some("robin")));
        assert!(!SearchFilter::MentionsMe.matches(&direct, None));
        assert!(!SearchFilter::MentionsMe.matches(&other, Some("robin")));
        assert!(SearchFilter::MentionsMe.matches(&broadcast, Some("robin")));
    }

    #[test]
    fn has_file_and_has_link_look_at_message_content() {
        let mut with_file = message("a", Vec::new());
        with_file.attachments.push(AttachmentSummary::default());
        let with_link = message(
            "a",
            vec![MessageFragment::Link {
                url: "https://example.com".into(),
                display: "example.com".into(),
            }],
        );
        let plain = message("a", vec![MessageFragment::Text("hi".into())]);
        assert!(SearchFilter::HasFile.matches(&with_file, None));
        assert!(!SearchFilter::HasFile.matches(&plain, None));
        assert!(SearchFilter::HasLink.matches(&with_link, None));
        assert!(!SearchFilter::HasLink.matches(&plain, None));
    }
}
