use super::*;

pub(super) fn request_leave(state: &mut UiState, conversation_id: ConversationId) -> ReducerOutput {
    if !state
        .workspace
        .channels
        .iter()
        .any(|channel| channel.id == conversation_id && channel.can_leave_channel())
        || !state
            .workspace
            .leaving_conversation_ids
            .insert(conversation_id.clone())
    {
        return ReducerOutput::default();
    }
    state
        .workspace
        .leave_channel_errors
        .remove(&conversation_id);
    let workspace_id = state
        .workspace
        .active_workspace_id
        .clone()
        .unwrap_or_else(|| WorkspaceId::new("ws_primary"));
    ReducerOutput {
        effects: vec![Effect::Backend(BackendCommand::LeaveChannel {
            workspace_id,
            conversation_id,
        })],
    }
}

pub(super) fn channel_left(
    state: &mut UiState,
    workspace_id: WorkspaceId,
    conversation_id: ConversationId,
) -> ReducerOutput {
    state
        .workspace
        .leave_channel_errors
        .remove(&conversation_id);
    state
        .workspace
        .leaving_conversation_ids
        .remove(&conversation_id);
    state
        .workspace
        .left_conversation_ids
        .insert(conversation_id.clone());
    if state.sidebar.retained_unread_conversation.as_ref() == Some(&conversation_id) {
        state.sidebar.retained_unread_conversation = None;
    }
    merge_workspace_conversations(state, Vec::new(), Vec::new());
    refresh_sidebar(state);
    if state.timeline.conversation_id.as_ref() != Some(&conversation_id) {
        return ReducerOutput::default();
    }
    reduce_ui_action(state, UiAction::CloseRightPane);
    if let Some(summary) = state
        .workspace
        .channels
        .iter()
        .chain(&state.workspace.direct_messages)
        .next()
    {
        let route = route_for_summary(summary, &workspace_id);
        return reduce_ui_action(state, UiAction::Navigate(route));
    }
    state.timeline = Default::default();
    let route = Route::WorkspaceHome { workspace_id };
    state.navigation.current_route = Some(route.clone());
    state.sidebar.highlighted_route = Some(route);
    ReducerOutput::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::conversation::{ConversationGroup, ConversationTeamType};

    fn channel(title: &str) -> ConversationSummary {
        ConversationSummary {
            id: ConversationId::new("channel"),
            title: title.into(),
            kind: ConversationKind::Channel,
            topic: String::new(),
            group: Some(ConversationGroup {
                id: "team".into(),
                display_name: "team".into(),
                team_type: ConversationTeamType::Complex,
            }),
            unread_count: 1,
            mention_count: 0,
            muted: false,
            last_activity_ms: 1,
            last_message: None,
        }
    }

    #[test]
    fn confirmed_rejoin_clears_pending_leave_and_error() {
        let mut state = UiState::default();
        state.workspace.channels.push(channel("dev"));
        let id = ConversationId::new("channel");
        request_leave(&mut state, id.clone());
        state
            .workspace
            .leave_channel_errors
            .insert(id.clone(), "old error".into());
        reduce_backend_event(
            &mut state,
            BackendEvent::ChannelJoined {
                workspace_id: WorkspaceId::new("ws"),
                conversation_id: id.clone(),
            },
        );
        assert!(!state.workspace.leaving_conversation_ids.contains(&id));
        assert!(!state.workspace.leave_channel_errors.contains_key(&id));
        assert!(!state.workspace.left_conversation_ids.contains(&id));
    }

    #[test]
    fn leave_is_pending_until_confirmed_and_failure_allows_retry() {
        let mut state = UiState::default();
        state.workspace.channels.push(channel("dev"));
        let id = ConversationId::new("channel");
        let output = request_leave(&mut state, id.clone());
        assert_eq!(output.effects.len(), 1);
        assert_eq!(state.workspace.channels.len(), 1);
        assert!(request_leave(&mut state, id.clone()).effects.is_empty());
        reduce_command_failure(
            &mut state,
            BackendCommand::LeaveChannel {
                workspace_id: WorkspaceId::new("ws"),
                conversation_id: id.clone(),
            },
            "offline".into(),
        );
        assert_eq!(
            state
                .workspace
                .leave_channel_errors
                .get(&id)
                .map(String::as_str),
            Some("offline")
        );
        assert_eq!(request_leave(&mut state, id.clone()).effects.len(), 1);
        assert!(!state.workspace.leave_channel_errors.contains_key(&id));
    }

    #[test]
    fn protects_general_simple_teams_and_unknown_membership() {
        for (title, team_type) in [
            ("general", ConversationTeamType::Complex),
            ("dev", ConversationTeamType::Simple),
            ("dev", ConversationTeamType::Unknown),
        ] {
            let mut state = UiState::default();
            let mut summary = channel(title);
            summary.group.as_mut().unwrap().team_type = team_type;
            state.workspace.channels.push(summary);
            assert!(
                request_leave(&mut state, ConversationId::new("channel"))
                    .effects
                    .is_empty()
            );
        }
        assert!(
            request_leave(&mut UiState::default(), ConversationId::new("preview"))
                .effects
                .is_empty()
        );
    }

    #[test]
    fn success_removes_all_rows_masks_stale_updates_and_join_restores() {
        let mut state = UiState::default();
        let summary = channel("dev");
        let id = summary.id.clone();
        state.workspace.channels.push(summary.clone());
        state.timeline.conversation_id = Some(id.clone());
        state.sidebar.retained_unread_conversation = Some(id.clone());
        state.thread.open = true;
        channel_left(&mut state, WorkspaceId::new("ws"), id.clone());
        assert!(state.workspace.channels.is_empty());
        assert!(state.sidebar.retained_unread_conversation.is_none());
        assert!(state.timeline.conversation_id.is_none());
        assert!(!state.thread.open);
        merge_workspace_conversations(&mut state, vec![summary.clone()], vec![]);
        assert!(state.workspace.channels.is_empty());
        reduce_backend_event(
            &mut state,
            BackendEvent::ChannelJoined {
                workspace_id: WorkspaceId::new("ws"),
                conversation_id: id,
            },
        );
        merge_workspace_conversations(&mut state, vec![summary], vec![]);
        assert_eq!(state.workspace.channels.len(), 1);
    }

    #[test]
    fn completion_does_not_navigate_away_from_another_conversation() {
        let mut state = UiState::default();
        state.timeline.conversation_id = Some(ConversationId::new("other"));
        channel_left(
            &mut state,
            WorkspaceId::new("ws"),
            ConversationId::new("channel"),
        );
        assert_eq!(
            state.timeline.conversation_id,
            Some(ConversationId::new("other"))
        );
    }
}
