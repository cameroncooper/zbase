use super::*;
use crate::domain::channel_details::TeamChannel;

pub(super) fn start_load(
    sender: Sender<BackendEvent>,
    account_id: AccountId,
    workspace_id: WorkspaceId,
    team_name: String,
    request_id: u64,
) {
    thread::spawn(move || {
        let command = BackendCommand::LoadTeamChannels {
            workspace_id,
            team_name: team_name.clone(),
            request_id,
        };
        let mut outcome = BackendTaskOutcome::new(&sender, command.clone());
        let result = (|| -> io::Result<_> {
            let path = socket_path().ok_or_else(|| io::Error::other("Keybase is unavailable"))?;
            Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(async {
                    tokio::time::timeout(Duration::from_secs(30), async {
                        let transport = FramedMsgpackTransport::connect(&path).await?;
                        let mut client = KeybaseRpcClient::new(transport);
                        load_channels(&mut client, &team_name, &account_id).await
                    })
                    .await
                    .map_err(|_| {
                        io::Error::other("Loading channels timed out. Please try again.")
                    })?
                })
        })();
        let event = match result {
            Ok((channels, conversation_bindings)) => BackendEvent::TeamChannelsLoaded {
                request_id,
                channels,
                conversation_bindings,
            },
            Err(error) => BackendEvent::CommandFailed {
                command: Box::new(command),
                error: format!("Could not load channels: {error}"),
            },
        };
        let _ = sender.send(event);
        outcome.complete();
    });
}

pub(super) async fn load_channels(
    client: &mut KeybaseRpcClient,
    team_name: &str,
    account_id: &AccountId,
) -> io::Result<(Vec<TeamChannel>, Vec<ConversationBinding>)> {
    let response = client
        .call(
            "chat.1.local.getTLFConversationsLocal",
            vec![Value::Map(vec![
                ("tlfName".into(), team_name.into()),
                ("topicType".into(), TOPIC_TYPE_CHAT.into()),
                ("membersType".into(), TEAM_MEMBERS_TYPE.into()),
            ])],
        )
        .await?;
    parse_channels(&response, team_name, account_id)
}

fn parse_channels(
    response: &Value,
    team_name: &str,
    account_id: &AccountId,
) -> io::Result<(Vec<TeamChannel>, Vec<ConversationBinding>)> {
    channel_membership::ensure_online(response)?;
    let convs = map_get(response, "convs")
        .and_then(Value::as_array)
        .ok_or_else(|| io::Error::other("Keybase returned an invalid channel list"))?;
    let mut channels = Vec::new();
    let mut bindings = Vec::new();
    for value in convs {
        let Some(conv) = parse_inbox_ui_item(value, None) else {
            continue;
        };
        if conv.summary.kind != ConversationKind::Channel
            || !conv
                .summary
                .group
                .as_ref()
                .is_some_and(|g| g.id.eq_ignore_ascii_case(team_name))
        {
            continue;
        }
        bindings.push(ConversationBinding {
            conversation_id: conv.summary.id.clone(),
            backend_id: BackendId::new(KEYBASE_BACKEND_ID),
            account_id: account_id.clone(),
            provider_conversation_ref: conv.provider_ref,
        });
        channels.push(TeamChannel {
            conversation: conv.summary,
            joined: conv.can_post,
        });
    }
    Ok((channels, bindings))
}

pub(super) fn start_join(
    sender: Sender<BackendEvent>,
    store: Arc<LocalStore>,
    workspace_id: WorkspaceId,
    conversation: ProviderConversationRef,
    summary: ConversationSummary,
) {
    thread::spawn(move || {
        let id = ConversationId::new(conversation.0.clone());
        let command = BackendCommand::JoinChannel {
            workspace_id: workspace_id.clone(),
            conversation_id: id.clone(),
            summary: Box::new(summary.clone()),
        };
        let mut outcome = BackendTaskOutcome::new(&sender, command.clone());
        let result = (|| -> io::Result<u64> {
            let generation = store.conversation_membership_generation(&id)?;
            let raw_id = provider_ref_to_conversation_id_bytes(&conversation)
                .ok_or_else(|| io::Error::other("Invalid channel ID"))?;
            let path = socket_path().ok_or_else(|| io::Error::other("Keybase is unavailable"))?;
            Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(async {
                    tokio::time::timeout(Duration::from_secs(30), async {
                        let transport = FramedMsgpackTransport::connect(&path).await?;
                        let mut client = KeybaseRpcClient::new(transport);
                        let response =
                            call_join_conversation_by_id_local(&mut client, &raw_id).await?;
                        channel_membership::ensure_online(&response)
                    })
                    .await
                    .map_err(|_| {
                        io::Error::other("Joining the channel timed out. Please try again.")
                    })?
                })?;
            Ok(generation)
        })();
        match result {
            Ok(generation) => {
                // Only cache conversations after an explicit successful join.
                if let Err(error) = store
                    .persist_conversation(&summary, summary.last_activity_ms)
                    .and_then(|_| store.persist_conversation_binding(&id, &conversation))
                {
                    tracing::warn!("Could not cache joined channel: {error}");
                }
                if let Err(error) =
                    store.transition_conversation_membership(&id, false, Some(generation), || {
                        let _ = sender.send(BackendEvent::ChannelJoined {
                            workspace_id,
                            conversation_id: id.clone(),
                        });
                    })
                {
                    let _ = sender.send(BackendEvent::CommandFailed {
                        command: Box::new(command),
                        error: format!(
                            "Channel joined, but local membership could not be saved: {error}"
                        ),
                    });
                }
            }
            Err(error) => {
                let _ = sender.send(BackendEvent::CommandFailed {
                    command: Box::new(command),
                    error: format!("Could not join channel: {error}"),
                });
            }
        }
        outcome.complete();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, team: &str, channel: &str, status: i64) -> Value {
        Value::Map(vec![
            ("convID".into(), id.into()),
            ("name".into(), team.into()),
            ("channel".into(), channel.into()),
            ("topicType".into(), TOPIC_TYPE_CHAT.into()),
            ("membersType".into(), TEAM_MEMBERS_TYPE.into()),
            ("teamType".into(), 2.into()),
            ("memberStatus".into(), status.into()),
        ])
    }

    #[test]
    fn directory_preserves_unjoined_channels_and_binds_ids_without_cross_team_results() {
        let response = Value::Map(vec![
            ("offline".into(), false.into()),
            (
                "convs".into(),
                Value::Array(vec![
                    item("abcd", "acme", "general", 0),
                    item("cdef", "acme", "design", 5),
                    item("eeee", "other", "general", 0),
                ]),
            ),
        ]);
        let account_id = AccountId::new("account");
        let (channels, bindings) = parse_channels(&response, "acme", &account_id).unwrap();
        assert_eq!(channels.len(), 2);
        assert!(channels[0].joined);
        assert!(!channels[1].joined);
        assert_eq!(channels[1].conversation.title, "design");
        assert_eq!(bindings[1].conversation_id.0, "kb_conv:cdef");
        assert_eq!(bindings[1].provider_conversation_ref.0, "kb_conv:cdef");
        assert_eq!(bindings[1].account_id, account_id);
    }

    #[test]
    fn offline_and_malformed_directories_fail_instead_of_appearing_empty() {
        let account = AccountId::new("account");
        for response in [
            Value::Map(vec![
                ("offline".into(), true.into()),
                ("convs".into(), Value::Array(vec![])),
            ]),
            Value::Map(vec![("offline".into(), false.into())]),
            Value::Nil,
        ] {
            assert!(parse_channels(&response, "acme", &account).is_err());
        }
        let empty = Value::Map(vec![
            ("offline".into(), false.into()),
            ("convs".into(), Value::Array(vec![])),
        ]);
        assert!(
            parse_channels(&empty, "acme", &account)
                .unwrap()
                .0
                .is_empty()
        );
    }
}
