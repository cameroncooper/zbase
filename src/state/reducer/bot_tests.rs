use super::*;
use crate::domain::bots::{BotCommand, BotOperation, BotResult, BotSettings, BotSnapshot};

fn with_bot_channels() -> UiState {
    use crate::domain::backend::{BackendCapabilities, BackendId, ProviderConversationRef};
    use crate::state::{bindings::ConversationBinding, state::AccountState};
    let mut state = UiState::default();
    let account = AccountId::new("account");
    state.backend.accounts.insert(
        account.clone(),
        AccountState {
            account_id: account.clone(),
            backend_id: BackendId::new("keybase"),
            display_name: "Test".into(),
            avatar: None,
            connection_state: ConnectionState::Connected,
            capabilities: BackendCapabilities::keybase_defaults(),
        },
    );
    for (id, team) in [("a", "team"), ("b", "team"), ("c", "other")] {
        let id = ConversationId::new(id);
        state.backend.conversation_bindings.insert(
            id.clone(),
            ConversationBinding {
                conversation_id: id.clone(),
                backend_id: BackendId::new("keybase"),
                account_id: account.clone(),
                provider_conversation_ref: ProviderConversationRef::new(&id.0),
            },
        );
        state
            .backend
            .conversation_team_ids
            .insert(id.clone(), team.into());
        state.backend.bots.insert(
            id.clone(),
            crate::domain::bots::ConversationBots {
                request: 10,
                snapshot: BotSnapshot {
                    team_id: Some(team.into()),
                    can_manage: true,
                    commands: vec![BotCommand {
                        name: "zoom".into(),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        state
            .backend
            .conversation_members
            .insert(id, Default::default());
    }
    state.timeline.conversation_id = Some(ConversationId::new("a"));
    state
}

#[test]
fn team_invalidation_refreshes_visible_channel_and_rejects_old_reads_in_other_channels() {
    let mut state = with_bot_channels();
    let a = ConversationId::new("a");
    let b = ConversationId::new("b");
    let c = ConversationId::new("c");
    state.backend.bots.get_mut(&b).unwrap().loading = true;
    let output = reduce_backend_event(
        &mut state,
        BackendEvent::BotsInvalidated {
            team_id: Some("team".into()),
            except: None,
        },
    );
    assert_eq!(output.effects.len(), 2);
    assert!(output.effects.iter().any(|effect| matches!(effect, Effect::Backend(BackendCommand::Bots { conversation_id, operation: BotOperation::Load, .. }) if conversation_id == &a)));
    assert!(state.backend.bots[&a].loading);
    assert!(state.backend.bots[&b].invalidated);
    assert!(!state.backend.bots[&b].snapshot.can_manage);
    assert!(state.backend.bots[&b].snapshot.commands.is_empty());
    assert!(!state.backend.conversation_members.contains_key(&a));
    assert!(!state.backend.conversation_members.contains_key(&b));
    assert!(state.backend.conversation_members.contains_key(&c));
    reduce_backend_event(
        &mut state,
        BackendEvent::BotsResult {
            conversation_id: b.clone(),
            request: 10,
            result: Ok(BotResult::Loaded(BotSnapshot {
                can_manage: true,
                ..Default::default()
            })),
        },
    );
    assert!(
        !state.backend.bots[&b].snapshot.can_manage,
        "a late read must not restore stale access"
    );
    assert!(state.backend.bots[&c].snapshot.can_manage);
    reduce_backend_event(
        &mut state,
        BackendEvent::BotsInvalidated {
            team_id: None,
            except: None,
        },
    );
    assert!(
        !state.backend.bots[&c].snapshot.can_manage,
        "name-only notifications invalidate all cached permissions"
    );
}

#[test]
fn notification_during_write_preserves_completion_and_never_replays_a_mutation() {
    let mut state = with_bot_channels();
    let id = ConversationId::new("a");
    let operation = BotOperation::Save {
        username: "zoombot".into(),
        restricted: false,
        settings: BotSettings::default(),
    };
    reduce_ui_action(
        &mut state,
        UiAction::Bots {
            conversation_id: id.clone(),
            request: 11,
            operation: operation.clone(),
        },
    );
    reduce_backend_event(
        &mut state,
        BackendEvent::BotsInvalidated {
            team_id: Some("team".into()),
            except: None,
        },
    );
    assert_eq!(state.backend.bots[&id].request, 11);
    let competing = reduce_ui_action(
        &mut state,
        UiAction::Bots {
            conversation_id: id.clone(),
            request: 12,
            operation: BotOperation::Load,
        },
    );
    assert!(
        competing.effects.is_empty(),
        "a read must not cancel an in-flight write"
    );
    let output = reduce_backend_event(
        &mut state,
        BackendEvent::BotsResult {
            conversation_id: id.clone(),
            request: 11,
            result: Ok(BotResult::Changed {
                operation,
                snapshot: None,
                refresh_error: None,
            }),
        },
    );
    assert_eq!(output.effects.len(), 1);
    assert!(matches!(
        &output.effects[0],
        Effect::Backend(BackendCommand::Bots {
            operation: BotOperation::Load,
            ..
        })
    ));
    let request = state.backend.bots[&id].request;
    reduce_backend_event(
        &mut state,
        BackendEvent::BotsResult {
            conversation_id: id.clone(),
            request,
            result: Ok(BotResult::Loaded(BotSnapshot::default())),
        },
    );
    assert_eq!(
        state.backend.bots[&id].completed_request, 11,
        "metadata reload must preserve the dialog's successful write completion"
    );
}

#[test]
fn failed_write_keeps_its_error_and_revoked_permissions_through_metadata_refresh() {
    let mut state = with_bot_channels();
    let id = ConversationId::new("a");
    for operation in [
        BotOperation::Add {
            username: "newbot".into(),
            restricted: true,
            settings: BotSettings::default(),
        },
        BotOperation::Save {
            username: "bot".into(),
            restricted: false,
            settings: BotSettings::default(),
        },
        BotOperation::Remove {
            username: "bot".into(),
        },
    ] {
        reduce_ui_action(
            &mut state,
            UiAction::Bots {
                conversation_id: id.clone(),
                request: 20,
                operation,
            },
        );
        reduce_backend_event(
            &mut state,
            BackendEvent::BotsInvalidated {
                team_id: Some("team".into()),
                except: None,
            },
        );
        reduce_backend_event(
            &mut state,
            BackendEvent::BotsResult {
                conversation_id: id.clone(),
                request: 20,
                result: Ok(BotResult::MutationFailed {
                    error: "Permission lost".into(),
                    can_manage: Some(false),
                }),
            },
        );
        assert!(!state.backend.bots[&id].snapshot.can_manage);
        assert_ne!(state.backend.bots[&id].completed_request, 20);
        assert!(!state.backend.bots[&id].mutation_pending);
        assert!(
            state.backend.bots[&id].loading,
            "external changes still refresh after a failed write"
        );
        let request = state.backend.bots[&id].request;
        reduce_backend_event(
            &mut state,
            BackendEvent::BotsResult {
                conversation_id: id.clone(),
                request,
                result: Ok(BotResult::Loaded(BotSnapshot::default())),
            },
        );
        assert_eq!(
            state.backend.bots[&id].failed_mutation,
            Some((20, "Permission lost".into()))
        );
    }
}

#[test]
fn stale_responses_cannot_override_a_newer_request_or_another_conversation() {
    let mut state = UiState::default();
    let a = ConversationId::new("a");
    let b = ConversationId::new("b");
    for (conversation_id, request) in [(a.clone(), 1), (b.clone(), 2), (a.clone(), 3)] {
        reduce_ui_action(
            &mut state,
            UiAction::Bots {
                conversation_id,
                request,
                operation: BotOperation::Load,
            },
        );
    }
    reduce_backend_event(
        &mut state,
        BackendEvent::BotsResult {
            conversation_id: a.clone(),
            request: 1,
            result: Ok(BotResult::Loaded(BotSnapshot::default())),
        },
    );
    assert!(state.backend.bots[&a].loading);
    reduce_backend_event(
        &mut state,
        BackendEvent::BotsResult {
            conversation_id: b.clone(),
            request: 2,
            result: Err("offline".into()),
        },
    );
    assert!(!state.backend.bots[&b].loading);
    assert!(state.backend.bots[&a].loading);
    reduce_backend_event(
        &mut state,
        BackendEvent::BotsResult {
            conversation_id: a.clone(),
            request: 3,
            result: Ok(BotResult::Loaded(BotSnapshot {
                commands: vec![BotCommand {
                    name: "zoom".into(),
                    ..Default::default()
                }],
                ..Default::default()
            })),
        },
    );
    assert_eq!(state.backend.bots[&a].snapshot.commands[0].name, "zoom");
    assert_eq!(state.backend.bots[&b].error.as_deref(), Some("offline"));
}
#[test]
fn successful_mutation_is_completed_even_when_refresh_fails() {
    let mut state = UiState::default();
    let id = ConversationId::new("c");
    let operation = BotOperation::Add {
        restricted: true,
        username: "meetings".into(),
        settings: BotSettings {
            commands: true,
            ..Default::default()
        },
    };
    reduce_ui_action(
        &mut state,
        UiAction::Bots {
            conversation_id: id.clone(),
            request: 7,
            operation: operation.clone(),
        },
    );
    let output = reduce_backend_event(
        &mut state,
        BackendEvent::BotsResult {
            conversation_id: id.clone(),
            request: 7,
            result: Ok(BotResult::Changed {
                operation,
                snapshot: None,
                refresh_error: Some("Refresh failed".into()),
            }),
        },
    );
    let bots = &state.backend.bots[&id];
    assert_eq!(bots.completed_request, 7);
    assert!(!bots.loading);
    assert_eq!(bots.snapshot.bots[0].username, "meetings");
    assert!(
        output.effects.is_empty(),
        "never retry a successful membership change"
    );
}
#[test]
fn featured_catalog_requests_do_not_cancel_conversation_metadata() {
    use crate::domain::bots::{FeaturedBot, FeaturedBotPage};
    let mut state = UiState::default();
    let id = ConversationId::new("dm");
    reduce_ui_action(
        &mut state,
        UiAction::Bots {
            conversation_id: id.clone(),
            request: 7,
            operation: BotOperation::Load,
        },
    );
    reduce_ui_action(
        &mut state,
        UiAction::Bots {
            conversation_id: id.clone(),
            request: 8,
            operation: BotOperation::Featured { offset: 0 },
        },
    );
    assert_eq!(state.backend.bots[&id].request, 7);
    reduce_backend_event(
        &mut state,
        BackendEvent::FeaturedBotsResult {
            request: 8,
            offset: 0,
            result: Ok(FeaturedBotPage {
                bots: vec![FeaturedBot {
                    username: "zoombot".into(),
                    ..Default::default()
                }],
                is_last_page: true,
            }),
        },
    );
    assert!(state.backend.bots[&id].loading);
    reduce_backend_event(
        &mut state,
        BackendEvent::BotsResult {
            conversation_id: id.clone(),
            request: 7,
            result: Ok(BotResult::Loaded(BotSnapshot::default())),
        },
    );
    assert!(!state.backend.bots[&id].loading);
    reduce_backend_event(
        &mut state,
        BackendEvent::FeaturedBotsResult {
            request: 6,
            offset: 0,
            result: Ok(FeaturedBotPage::default()),
        },
    );
    assert_eq!(state.backend.featured_bots.bots[0].username, "zoombot");
}
