//! Offline backend that answers commands from the demo fixtures, so the demo build
//! drives the same store/reducer paths as a real Keybase session.
use crate::{
    domain::{
        backend::{AccountId, BackendCapabilities, BackendId},
        ids::{ConversationId, MessageId, WorkspaceId},
    },
    models::demo,
    services::backends::traits::{BackendError, ChatBackend, RoutedBackendCommand},
    state::{
        event::{BackendEvent, PresencePatch, TeamRoleEntry},
        state::TimelineKey,
    },
    util::formatting::now_unix_ms,
};

mod bots;
pub struct DemoBackend {
    joined_channels: std::collections::HashSet<ConversationId>,
    bots: std::collections::HashMap<ConversationId, crate::domain::bots::BotSnapshot>,
}

impl DemoBackend {
    pub fn new() -> Self {
        Self {
            joined_channels: Default::default(),
            bots: Default::default(),
        }
    }

    pub fn conversation_events(conversation_id: ConversationId) -> Vec<BackendEvent> {
        let typing =
            (conversation_id.0 == demo::DEMO_GENERAL_ID).then(|| BackendEvent::TypingUpdated {
                conversation_id: conversation_id.clone(),
                users: vec![crate::domain::ids::UserId::new(demo::DEMO_TYPING_USER)],
            });
        let mut events = vec![
            BackendEvent::TimelineReplaced {
                messages: demo::demo_conversation_messages(&conversation_id),
                conversation_id: conversation_id.clone(),
                older_cursor: None,
                newer_cursor: None,
            },
            BackendEvent::ReadMarkerUpdated {
                read_upto: demo::demo_read_marker(&conversation_id),
                conversation_id,
            },
        ];
        events.extend(typing);
        events
    }

    pub fn member_events(conversation_id: ConversationId) -> Vec<BackendEvent> {
        let updated_ms = now_unix_ms();
        let mut events = vec![BackendEvent::ConversationMembersUpdated {
            conversation_id: conversation_id.clone(),
            members: demo::demo_conversation_members(&conversation_id),
            updated_ms,
            is_complete: true,
        }];
        if let Some(team_id) = demo::demo_team_id_for_conversation(&conversation_id) {
            events.push(BackendEvent::TeamRolesUpdated {
                conversation_id,
                team_id: team_id.to_string(),
                roles: demo::demo_team_roles()
                    .into_iter()
                    .map(|(user_id, role)| TeamRoleEntry { user_id, role })
                    .collect(),
                updated_ms,
            });
        }
        events
    }

    /// Profiles and presence for every demo person, as a live session would sync them.
    pub fn people_events(account_id: &AccountId) -> Vec<BackendEvent> {
        let updated_ms = now_unix_ms();
        let mut events = demo::demo_people()
            .into_iter()
            .map(|person| BackendEvent::UserProfileUpserted {
                user_id: crate::domain::ids::UserId::new(person.username),
                display_name: person.display_name.to_string(),
                avatar_asset: person.avatar_asset.map(str::to_string),
                updated_ms,
            })
            .collect::<Vec<_>>();
        events.push(BackendEvent::PresenceUpdated {
            account_id: account_id.clone(),
            users: demo::demo_people()
                .into_iter()
                .map(|person| {
                    let user_id = crate::domain::ids::UserId::new(person.username);
                    PresencePatch {
                        presence: demo::demo_user_summary(&user_id).presence,
                        user_id,
                    }
                })
                .collect(),
        });
        events.push(BackendEvent::AffinitySynced {
            affinities: demo::demo_people()
                .into_iter()
                .map(|person| {
                    (
                        crate::domain::ids::UserId::new(person.username),
                        person.affinity,
                    )
                })
                .collect(),
        });
        events
    }
}

impl ChatBackend for DemoBackend {
    fn backend_id(&self) -> BackendId {
        BackendId::new("keybase")
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::keybase_defaults()
    }

    fn connect_account(
        &mut self,
        _account_id: &AccountId,
    ) -> Result<Vec<BackendEvent>, BackendError> {
        Ok(Vec::new())
    }

    fn execute(&mut self, cmd: RoutedBackendCommand) -> Result<Vec<BackendEvent>, BackendError> {
        let events = match cmd {
            RoutedBackendCommand::LoadTeamChannels {
                account_id,
                team_name,
                request_id,
                ..
            } => {
                let mut channels = demo::demo_team_channels(&team_name);
                for channel in &mut channels {
                    channel.joined |= self.joined_channels.contains(&channel.conversation.id);
                }
                let conversation_bindings = channels
                    .iter()
                    .map(|channel| crate::state::ConversationBinding {
                        conversation_id: channel.conversation.id.clone(),
                        backend_id: self.backend_id(),
                        account_id: account_id.clone(),
                        provider_conversation_ref:
                            crate::domain::backend::ProviderConversationRef::new(
                                channel.conversation.id.0.clone(),
                            ),
                    })
                    .collect();
                vec![BackendEvent::TeamChannelsLoaded {
                    request_id,
                    channels,
                    conversation_bindings,
                }]
            }
            RoutedBackendCommand::JoinChannel {
                workspace_id,
                summary,
                ..
            } => {
                self.joined_channels.insert(summary.id.clone());
                vec![BackendEvent::ChannelJoined {
                    workspace_id,
                    conversation_id: summary.id,
                }]
            }
            RoutedBackendCommand::Bots {
                conversation,
                request,
                operation,
                ..
            } => self.bot_events(ConversationId::new(conversation.0), request, operation),
            RoutedBackendCommand::LoadConversation { conversation, .. } => {
                Self::conversation_events(ConversationId::new(conversation.0))
            }
            RoutedBackendCommand::JumpToMessage { conversation, .. } => {
                Self::conversation_events(ConversationId::new(conversation.0))
            }
            RoutedBackendCommand::LoadLinkedMessage {
                conversation,
                message_id,
                ..
            } => {
                let conversation_id = ConversationId::new(conversation.0);
                let message_id = MessageId::new(message_id);
                let message = demo::demo_conversation_messages(&conversation_id)
                    .into_iter()
                    .find(|message| message.id == message_id);
                vec![BackendEvent::LinkedMessageLoaded {
                    conversation_id,
                    message_id,
                    message,
                }]
            }
            RoutedBackendCommand::LoadConversationMembers { conversation, .. } => {
                Self::member_events(ConversationId::new(conversation.0))
            }
            RoutedBackendCommand::LoadThread {
                conversation,
                root_message,
                ..
            } => {
                let conversation_id = ConversationId::new(conversation.0);
                let root_id = MessageId::new(root_message.0);
                vec![BackendEvent::MessagesPrepended {
                    messages: demo::demo_thread_messages(&conversation_id, &root_id),
                    key: TimelineKey::Thread {
                        conversation_id,
                        root_id: root_id.clone(),
                        requested_root_id: root_id,
                    },
                    cursor: None,
                }]
            }
            RoutedBackendCommand::LoadOlderMessages { conversation, .. } => {
                vec![BackendEvent::MessagesPrepended {
                    key: TimelineKey::Conversation(ConversationId::new(conversation.0)),
                    messages: Vec::new(),
                    cursor: None,
                }]
            }
            RoutedBackendCommand::Search {
                workspace_id,
                query_id,
                query,
                filters,
                ..
            } => vec![BackendEvent::SearchResults {
                query_id,
                results: demo::demo_search_results(
                    &query,
                    &filters,
                    &workspace_id.unwrap_or_else(|| WorkspaceId::new(demo::DEMO_WORKSPACE_ID)),
                ),
                is_complete: true,
            }],
            RoutedBackendCommand::SearchUsers {
                query_id, query, ..
            } => vec![BackendEvent::UserSearchResults {
                query_id,
                results: demo::demo_user_search(&query),
            }],
            RoutedBackendCommand::LoadUserProfile {
                account_id,
                user_id,
            } => demo::demo_user_profile(&user_id)
                .map(|profile| BackendEvent::UserProfileLoaded {
                    account_id,
                    profile,
                })
                .into_iter()
                .collect(),
            RoutedBackendCommand::LoadSocialGraphList {
                user_id, list_type, ..
            } => vec![BackendEvent::SocialGraphListLoaded {
                entries: demo::demo_social_graph(&user_id, list_type),
                user_id,
                list_type,
            }],
            // Everything else (sends, reactions, mark-read, calls…) is accepted and ignored.
            _ => Vec::new(),
        };
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::backend::ProviderConversationRef;

    #[test]
    fn team_directory_join_is_bound_and_survives_reload() {
        let mut backend = DemoBackend::new();
        let load = || RoutedBackendCommand::LoadTeamChannels {
            account_id: AccountId::new("demo"),
            workspace_id: WorkspaceId::new(demo::DEMO_WORKSPACE_ID),
            team_name: demo::DEMO_TEAM_ID.into(),
            request_id: 1,
        };
        let mut events = backend.execute(load()).unwrap();
        let BackendEvent::TeamChannelsLoaded {
            channels,
            conversation_bindings,
            ..
        } = events.remove(0)
        else {
            panic!("expected directory");
        };
        let channel = channels
            .into_iter()
            .find(|c| !c.joined)
            .expect("an unjoined channel");
        assert!(
            !demo::demo_channels()
                .iter()
                .any(|c| c.id == channel.conversation.id)
        );
        assert!(
            conversation_bindings
                .iter()
                .any(|b| b.conversation_id == channel.conversation.id)
        );
        let id = channel.conversation.id.clone();
        let events = backend
            .execute(RoutedBackendCommand::JoinChannel {
                account_id: AccountId::new("demo"),
                workspace_id: WorkspaceId::new(demo::DEMO_WORKSPACE_ID),
                conversation: ProviderConversationRef::new(id.0.clone()),
                summary: Box::new(channel.conversation),
            })
            .unwrap();
        assert!(
            matches!(&events[0], BackendEvent::ChannelJoined { conversation_id, .. } if *conversation_id == id)
        );
        let events = backend.execute(load()).unwrap();
        assert!(
            matches!(&events[0], BackendEvent::TeamChannelsLoaded { channels, .. } if channels.iter().any(|c| c.conversation.id == id && c.joined))
        );
    }
}
