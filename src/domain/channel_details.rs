use crate::domain::{
    affinity::Affinity,
    conversation::{ConversationGroup, ConversationKind},
    ids::{ConversationId, UserId},
    pins::PinnedItem,
};

/// A discovered channel is not necessarily a joined sidebar conversation.
#[derive(Clone, Debug)]
pub struct TeamChannel {
    pub conversation: crate::domain::conversation::ConversationSummary,
    pub joined: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NotificationLevel {
    #[default]
    All,
    MentionsOnly,
    Nothing,
}

impl NotificationLevel {
    pub const ALL: [Self; 3] = [Self::All, Self::MentionsOnly, Self::Nothing];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "On any activity",
            Self::MentionsOnly => "Only when @mentioned",
            Self::Nothing => "Never",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::MentionsOnly => "mentions",
            Self::Nothing => "nothing",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "all" => Some(Self::All),
            "mentions" => Some(Self::MentionsOnly),
            "nothing" => Some(Self::Nothing),
            _ => None,
        }
    }
}

/// Per-conversation notification choices, as in Keybase's conversation
/// Settings tab. `muted` maps to Keybase's MUTED conversation status; the rest
/// live in the conversation's notification settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConversationNotificationSettings {
    pub muted: bool,
    pub ignore_mentions: bool,
    pub desktop: NotificationLevel,
    pub mobile: NotificationLevel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelMemberPreview {
    pub user_id: UserId,
    pub display_name: String,
    pub avatar_asset: Option<String>,
    pub affinity: Affinity,
    pub is_team_admin_or_owner: bool,
    /// Owner rather than admin; only meaningful with `is_team_admin_or_owner`.
    pub is_team_owner: bool,
    pub is_bot: bool,
    /// Last known presence, `None` until the backend reports it.
    pub availability: Option<crate::domain::presence::Availability>,
}

impl ChannelMemberPreview {
    /// The name the app shows for this member (see `NameStyle`).
    pub fn shown_name(&self) -> &str {
        crate::domain::user::shown_name(&self.user_id.0, &self.display_name)
    }

    /// The name [`Self::shown_name`] leaves out.
    pub fn other_name(&self) -> Option<String> {
        crate::domain::user::other_name(&self.user_id.0, &self.display_name)
    }
}

#[derive(Clone, Debug)]
pub struct ChannelDetails {
    pub conversation_id: ConversationId,
    pub title: String,
    pub topic: String,
    pub kind: ConversationKind,
    pub group: Option<ConversationGroup>,
    pub member_count: u32,
    /// Full member list (used by Members pane + @mention autocomplete).
    pub members: Vec<ChannelMemberPreview>,
    /// Small subset used for compact UI previews (e.g. Details pane).
    pub member_preview: Vec<ChannelMemberPreview>,
    pub notifications: ConversationNotificationSettings,
    pub pinned_items: Vec<PinnedItem>,
    pub can_edit_topic: bool,
    pub can_manage_members: bool,
    pub can_archive: bool,
    pub can_leave: bool,
    pub is_leaving: bool,
    pub leave_error: Option<String>,
    pub can_post: bool,
    pub created_at: Option<String>,
    pub description: Option<String>,
    pub is_archived: bool,
}
