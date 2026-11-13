use crate::domain::{
    attachment::AttachmentKind,
    backend::AccountId,
    channel_details::ConversationNotificationSettings,
    conversation::ConversationKind,
    ids::{CallId, ConversationId, MessageId, UserId, WorkspaceId},
    profile::SocialGraphListType,
    search::SearchFilter,
};

use super::{
    action::DraftKey,
    ids::{ClientMessageId, LocalAttachmentId, OpId, QueryId},
    state::TimelineKey,
};

#[derive(Clone, Debug)]
pub enum SearchScope {
    Global,
    Account(AccountId),
    Workspace(WorkspaceId),
    Conversation(ConversationId),
}

#[derive(Clone, Debug)]
pub struct UploadedAttachment {
    pub local_id: LocalAttachmentId,
    pub name: String,
    pub kind: AttachmentKind,
    pub remote_token: String,
}

#[derive(Clone, Debug)]
pub enum BackendCommand {
    Bots {
        conversation_id: ConversationId,
        request: u64,
        operation: crate::domain::bots::BotOperation,
    },
    ConnectAccount {
        account_id: AccountId,
    },
    LoadBootstrap {
        account_id: AccountId,
    },
    LoadWorkspace {
        workspace_id: WorkspaceId,
    },
    LoadConversation {
        conversation_id: ConversationId,
    },
    LoadConversationMembers {
        conversation_id: ConversationId,
    },
    LoadThread {
        conversation_id: ConversationId,
        root_id: MessageId,
    },
    LoadOlderMessages {
        key: TimelineKey,
        cursor: String,
    },
    JumpToMessage {
        conversation_id: ConversationId,
        message_id: MessageId,
    },
    /// Fetch one message for a `keybase://chat/...` link preview; answered by
    /// `BackendEvent::LinkedMessageLoaded`.
    LoadLinkedMessage {
        conversation_id: ConversationId,
        message_id: MessageId,
    },
    SendMessage {
        op_id: OpId,
        draft_key: DraftKey,
        conversation_id: ConversationId,
        client_message_id: ClientMessageId,
        text: String,
        attachments: Vec<UploadedAttachment>,
        reply_to: Option<MessageId>,
    },
    SendAttachment {
        op_id: OpId,
        draft_key: DraftKey,
        conversation_id: ConversationId,
        client_message_id: ClientMessageId,
        local_path: String,
        filename: String,
        caption: String,
    },
    EditMessage {
        op_id: OpId,
        conversation_id: ConversationId,
        message_id: MessageId,
        text: String,
    },
    DeleteMessage {
        op_id: OpId,
        conversation_id: ConversationId,
        message_id: MessageId,
    },
    ReactToMessage {
        op_id: OpId,
        conversation_id: ConversationId,
        message_id: MessageId,
        reaction: String,
    },
    PinMessage {
        op_id: OpId,
        conversation_id: ConversationId,
        message_id: MessageId,
    },
    UnpinMessage {
        op_id: OpId,
        conversation_id: ConversationId,
    },
    MarkRead {
        conversation_id: ConversationId,
        message_id: Option<MessageId>,
    },
    Search {
        query_id: QueryId,
        scope: SearchScope,
        query: String,
        filters: Vec<SearchFilter>,
    },
    SearchUsers {
        query_id: QueryId,
        query: String,
    },
    CreateConversation {
        op_id: OpId,
        participants: Vec<UserId>,
        kind: ConversationKind,
    },
    StartCall {
        op_id: OpId,
        conversation_id: ConversationId,
    },
    LeaveCall {
        op_id: OpId,
        call_id: CallId,
    },
    LoadUserProfile {
        user_id: UserId,
    },
    RefreshParticipants {
        user_id: UserId,
        conversation_id: Option<ConversationId>,
    },
    LoadSocialGraphList {
        user_id: UserId,
        list_type: SocialGraphListType,
    },
    FollowUser {
        user_id: UserId,
    },
    UnfollowUser {
        user_id: UserId,
    },
    ResolveChannel {
        workspace_id: WorkspaceId,
        team_name: String,
        channel_name: String,
    },
    ResolveChannelById {
        workspace_id: WorkspaceId,
        conversation_id: ConversationId,
    },
    LoadTeamChannels {
        workspace_id: WorkspaceId,
        team_name: String,
        request_id: u64,
    },
    JoinChannel {
        workspace_id: WorkspaceId,
        conversation_id: ConversationId,
        summary: Box<crate::domain::conversation::ConversationSummary>,
    },
    LeaveChannel {
        workspace_id: WorkspaceId,
        conversation_id: ConversationId,
    },
    SetConversationNotificationSettings {
        conversation_id: ConversationId,
        settings: ConversationNotificationSettings,
        previous: ConversationNotificationSettings,
    },
}

#[derive(Clone, Debug)]
pub enum Effect {
    Backend(BackendCommand),
    LoadSettings,
    LoadDraft { key: DraftKey },
    PersistDraft { key: DraftKey },
    PersistSettings,
}
