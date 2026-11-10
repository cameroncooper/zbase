use crate::domain::channel_details::ChannelDetails;
use crate::domain::conversation::ConversationSummary;
use crate::domain::pins::PinnedItem;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct AvatarParticipant {
    pub display_name: String,
    pub asset: Option<String>,
    /// Last known presence; `None` until the backend reports it.
    pub presence: Option<crate::domain::presence::Presence>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ConversationModel {
    pub summary: ConversationSummary,
    pub pinned_message: Option<PinnedItem>,
    pub details: Option<ChannelDetails>,
    pub avatar_asset: Option<String>,
    pub participant_avatars: Vec<AvatarParticipant>,
    pub member_count: u32,
    pub can_post: bool,
    pub is_archived: bool,
}
