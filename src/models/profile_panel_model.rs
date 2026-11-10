use crate::domain::{
    ids::UserId,
    profile::{SocialGraphListType, SocialListLoadState, UserProfile},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize)]
pub enum SocialTab {
    #[default]
    Followers,
    Following,
}

impl SocialTab {
    pub fn as_list_type(self) -> SocialGraphListType {
        match self {
            Self::Followers => SocialGraphListType::Followers,
            Self::Following => SocialGraphListType::Following,
        }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct ProfilePanelModel {
    pub user_id: Option<UserId>,
    pub profile: Option<UserProfile>,
    pub loading: bool,
    pub active_social_tab: SocialTab,
    pub social_list_status: Option<SocialListLoadState>,
}
