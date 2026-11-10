use crate::domain::{
    ids::{ConversationId, MessageId, UserId},
    route::Route,
};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum QuickSwitcherResultKind {
    UnreadChannel,
    Channel,
    DirectMessage,
    Message,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct QuickSwitcherResult {
    pub label: String,
    pub sublabel: Option<String>,
    pub kind: QuickSwitcherResultKind,
    pub route: Route,
    pub conversation_id: ConversationId,
    pub message_id: Option<MessageId>,
    pub match_ranges: Vec<(usize, usize)>,
    /// For message hits: who wrote it and when, shown like a Search result.
    pub author_id: Option<UserId>,
    pub timestamp_ms: Option<i64>,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct QuickSwitcherModel {
    pub query: String,
    pub results: Vec<QuickSwitcherResult>,
    pub selected_index: usize,
    pub loading_messages: bool,
}
