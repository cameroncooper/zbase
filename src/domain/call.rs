use crate::domain::{
    ids::{CallId, UserId},
    user::UserSummary,
};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum CallStatus {
    Idle,
    Ringing,
    Connecting,
    ActiveAudio,
    ActiveVideo,
    SharingScreen,
    Reconnecting,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ParticipantSummary {
    pub user_id: UserId,
    pub muted: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct CallSessionSummary {
    pub call_id: CallId,
    pub status: CallStatus,
    pub participants: Vec<UserSummary>,
}
