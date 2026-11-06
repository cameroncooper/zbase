use crate::domain::{ids::ConversationId, message_preview::MessagePreview};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConversationBadgeCounts {
    pub unread_count: u32,
    pub badge_count: u32,
}

/// A complete snapshot also clears conversations omitted by the provider.
/// Partial updates are used for locally confirmed mark-read operations.
#[derive(Clone, Debug, Default)]
pub struct ConversationBadgeState {
    pub counts: HashMap<ConversationId, ConversationBadgeCounts>,
    pub complete: bool,
}

impl ConversationBadgeState {
    pub fn update(
        &mut self,
        counts: HashMap<ConversationId, ConversationBadgeCounts>,
        complete: bool,
    ) {
        if complete {
            self.counts = counts;
            self.complete = true;
        } else {
            self.counts.extend(counts);
        }
    }

    pub fn apply(&self, summary: &mut ConversationSummary) {
        if let Some(counts) = self
            .counts
            .get(&summary.id)
            .copied()
            .or_else(|| self.complete.then_some(ConversationBadgeCounts::default()))
        {
            summary.unread_count = counts.unread_count;
            // The existing sidebar field represents Keybase's badge count.
            summary.mention_count = counts.badge_count;
        }
    }
}

/// DM titles contain usernames, not display names. Keep only the other people,
/// preserving the original title for a conversation with yourself.
pub fn dm_peer_title(title: &str, self_username: Option<&str>) -> String {
    let peers = title
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .filter(|name| !self_username.is_some_and(|me| name.eq_ignore_ascii_case(me.trim())))
        .collect::<Vec<_>>();
    if peers.is_empty() {
        title.to_string()
    } else {
        peers.join(", ")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConversationKind {
    Channel,
    DirectMessage,
    GroupDirectMessage,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ConversationTeamType {
    Unknown,
    Simple,
    Complex,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConversationGroup {
    pub id: String,
    pub display_name: String,
    pub team_type: ConversationTeamType,
}

impl ConversationSummary {
    /// Keybase only supports leaving individual non-general channels in big teams.
    pub fn can_leave_channel(&self) -> bool {
        self.kind == ConversationKind::Channel
            && !self.title.eq_ignore_ascii_case("general")
            && self
                .group
                .as_ref()
                .is_some_and(|group| group.team_type == ConversationTeamType::Complex)
    }

    pub fn compare_recency(&self, other: &Self) -> std::cmp::Ordering {
        other
            .last_activity_ms
            .cmp(&self.last_activity_ms)
            .then_with(|| self.id.0.cmp(&other.id.0))
    }

    pub fn preserve_known_metadata(&mut self, previous: &Self) {
        self.last_activity_ms = self.last_activity_ms.max(previous.last_activity_ms);
        self.last_message =
            MessagePreview::newest(previous.last_message.clone(), self.last_message.take());
        if let (Some(group), Some(old)) = (&mut self.group, &previous.group)
            && group.id == old.id
            && group.team_type == ConversationTeamType::Unknown
        {
            group.team_type = old.team_type.clone();
        }
    }

    pub fn is_simple_team(&self) -> bool {
        self.group
            .as_ref()
            .is_some_and(|group| group.team_type == ConversationTeamType::Simple)
    }

    pub fn simple_team_label(&self) -> Option<String> {
        self.group
            .as_ref()
            .filter(|_| self.is_simple_team())
            .map(|group| {
                if self.title == "general" {
                    group.display_name.clone()
                } else {
                    format!("{} #{}", group.display_name, self.title)
                }
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationSummary {
    pub id: ConversationId,
    pub title: String,
    pub kind: ConversationKind,
    pub topic: String,
    pub group: Option<ConversationGroup>,
    pub unread_count: u32,
    pub mention_count: u32,
    pub muted: bool,
    pub last_activity_ms: i64,
    /// Newest message, for inbox previews. `None` until a message is known.
    pub last_message: Option<MessagePreview>,
}

#[cfg(test)]
mod tests {
    use super::dm_peer_title;

    #[test]
    fn dm_peer_titles_exclude_self_by_username_and_preserve_self_chats() {
        assert_eq!(
            dm_peer_title(
                "alexandermey,butterbot,cameroncooper",
                Some(" CameronCooper ")
            ),
            "alexandermey, butterbot"
        );
        assert_eq!(
            dm_peer_title("cameroncooper,alexandermey", Some("cameroncooper")),
            "alexandermey"
        );
        assert_eq!(
            dm_peer_title("cameroncooper", Some("cameroncooper")),
            "cameroncooper"
        );
        assert_eq!(
            dm_peer_title("alexandermey,butterbot", Some("cameroncooper")),
            "alexandermey, butterbot"
        );
        assert_eq!(
            dm_peer_title("alexandermey,butterbot,cameroncooper", None),
            "alexandermey, butterbot, cameroncooper"
        );
    }
}
