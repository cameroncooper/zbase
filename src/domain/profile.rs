use crate::domain::{affinity::Affinity, ids::UserId, presence::Presence};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize)]
pub enum SocialGraphListType {
    #[default]
    Followers,
    Following,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum SocialListLoadState {
    Loading,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum ProofState {
    Verified,
    Broken,
    Pending,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct IdentityProof {
    pub service_name: String,
    pub service_username: String,
    pub proof_url: Option<String>,
    pub site_url: Option<String>,
    pub icon_asset: Option<String>,
    pub state: ProofState,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SocialGraphEntry {
    pub user_id: UserId,
    pub display_name: String,
    pub avatar_asset: Option<String>,
    pub affinity: Affinity,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SocialGraph {
    pub followers_count: Option<u32>,
    pub following_count: Option<u32>,
    pub is_following_you: bool,
    pub you_are_following: bool,
    pub followers: Option<Vec<SocialGraphEntry>>,
    pub following: Option<Vec<SocialGraphEntry>>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct TeamShowcaseEntry {
    pub name: String,
    pub description: String,
    pub is_open: bool,
    pub members_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct CustomField {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum ProfileSection {
    IdentityProofs(Vec<IdentityProof>),
    SocialGraph(SocialGraph),
    TeamShowcase(Vec<TeamShowcaseEntry>),
    CustomFields(Vec<CustomField>),
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct UserProfile {
    pub user_id: UserId,
    pub username: String,
    pub display_name: String,
    pub avatar_asset: Option<String>,
    pub presence: Presence,
    pub affinity: Affinity,
    pub bio: Option<String>,
    pub location: Option<String>,
    pub title: Option<String>,
    pub sections: Vec<ProfileSection>,
}
