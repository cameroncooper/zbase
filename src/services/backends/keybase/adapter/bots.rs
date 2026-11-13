use super::*;
use crate::domain::bots::{BotCommand, BotInfo, BotOperation, BotResult, BotSettings, BotSnapshot};

const PREFIX: &str = "chat.1.local.";

pub(super) fn run(
    sender: Sender<BackendEvent>,
    local_store: Arc<LocalStore>,
    conversation: ProviderConversationRef,
    request: u64,
    operation: BotOperation,
) {
    if let BotOperation::Featured { offset } = operation {
        let result = featured(offset).map_err(|e| e.to_string());
        if let Ok(page) = &result {
            spawn_prefetch_user_profiles(
                &sender,
                &local_store,
                page.bots.iter().map(|bot| bot.username.clone()).collect(),
            );
        }
        let _ = sender.send(BackendEvent::FeaturedBotsResult {
            request,
            offset,
            result,
        });
        return;
    }
    let conversation_id = canonical_conversation_id_from_provider_ref(&conversation);
    let mut result = execute(&conversation, operation).map_err(|error| error.to_string());
    if let Ok(BotResult::Changed { operation, .. }) = result {
        let (snapshot, refresh_error) = match execute(&conversation, BotOperation::Load) {
            Ok(BotResult::Loaded(snapshot)) => (Some(snapshot), None),
            Err(error) => (
                None,
                Some(format!(
                    "Bot updated. Could not refresh its details: {error}"
                )),
            ),
            _ => unreachable!("Load only returns a snapshot"),
        };
        result = Ok(BotResult::Changed {
            operation,
            snapshot,
            refresh_error,
        });
        run_load_conversation_members(sender.clone(), conversation_id.clone());
    }
    let _ = sender.send(BackendEvent::BotsResult {
        conversation_id,
        request,
        result,
    });
}

fn execute(
    conversation: &ProviderConversationRef,
    operation: BotOperation,
) -> io::Result<BotResult> {
    let raw = provider_ref_to_conversation_id_bytes(conversation)
        .ok_or_else(|| io::Error::other("Missing Keybase conversation ID"))?;
    let path = socket_path().ok_or_else(|| io::Error::other("Keybase is not running"))?;
    // Resolve roles off the UI thread, including implicit teams used by group DMs.
    let lookup = matches!(operation, BotOperation::Lookup { .. });
    let roles = if lookup {
        HashMap::new()
    } else {
        let team = resolve_team_id_from_conversation_ref_local(conversation)?.ok_or_else(|| {
            io::Error::other("Bot membership is unavailable for this conversation")
        })?;
        fetch_team_roles_for_team(&team)?
    };
    let runtime = Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(async move {
        let transport = FramedMsgpackTransport::connect(&path).await?;
        let mut client = KeybaseRpcClient::new(transport);
        let base = vec![(Value::from("convID"), Value::Binary(raw.clone()))];
        if let BotOperation::Lookup { username } = &operation {
            let username = username.trim().trim_start_matches('@').to_ascii_lowercase();
            if username.is_empty()
                || !username
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err(io::Error::other("Enter a Keybase username"));
            }
            client
                .call(
                    KEYBASE_LOAD_USER_BY_NAME,
                    vec![Value::Map(vec![
                        (Value::from("sessionID"), Value::from(SESSION_ID)),
                        (Value::from("username"), Value::from(username.clone())),
                    ])],
                )
                .await?;
            let value = call(
                &mut client,
                "listPublicBotCommandsLocal",
                vec![(Value::from("username"), Value::from(username.clone()))],
            )
            .await?;
            let commands = commands(&value);
            return Ok(BotResult::Lookup { username, commands });
        }
        let status = client
            .call(
                KEYBASE_GET_CURRENT_STATUS,
                vec![Value::Map(vec![(
                    Value::from("sessionID"),
                    Value::from(SESSION_ID),
                )])],
            )
            .await?;
        let username = status_username(&status)
            .ok_or_else(|| io::Error::other("Sign in to Keybase to manage bots"))?;
        let inbox = fetch_inbox_for_conversation_id(&mut client, &raw).await?;
        let members_type = extract_team_lookup_params_from_inbox_response(&inbox).map(|(_,kind,_)|kind);
        let can_manage = can_manage_bots(members_type, roles.get(&UserId::new(username)).copied());
        let mutation = !matches!(operation, BotOperation::Load);
        if mutation && !can_manage {
            return Err(io::Error::other(
                "Your Keybase role does not allow managing bots in this conversation",
            ));
        }
        let mut args = base.clone();
        match &operation {
            BotOperation::Add { username, .. } if roles.contains_key(&UserId::new(username)) => {
                return Err(io::Error::other("This account is already a team member. Manage its existing membership instead."));
            }
            BotOperation::Save { username, .. }
                if roles.get(&UserId::new(username)) != Some(&6) =>
            {
                return Err(io::Error::other(
                    "Only restricted bot settings can be edited here",
                ));
            }
            BotOperation::Remove { username }
                if !roles
                    .get(&UserId::new(username))
                    .is_some_and(|r| *r == 5 || *r == 6) =>
            {
                return Err(io::Error::other("This account is not an installed bot"));
            }
            _ => {}
        }
        match &operation {
            BotOperation::Add { username, settings } => {
                args.extend([
                    (Value::from("username"), Value::from(username.clone())),
                    (Value::from("role"), Value::from(6)),
                    (Value::from("botSettings"), settings_value(settings)),
                ]);
                call(&mut client, "addBotMember", args).await?;
            }
            BotOperation::Save { username, settings } => {
                // This RPC edits settings without changing the bot's membership role.
                args.extend([
                    (Value::from("username"), Value::from(username.clone())),
                    (Value::from("botSettings"), settings_value(settings)),
                ]);
                call(&mut client, "setBotMemberSettings", args).await?;
            }
            BotOperation::Remove { username } => {
                args.push((Value::from("username"), Value::from(username.clone())));
                call(&mut client, "removeBotMember", args).await?;
            }
            _ => {}
        }
        // A successful mutation is final even if the subsequent refresh fails.
        if mutation {
            return Ok(BotResult::Changed {
                operation,
                snapshot: None,
                refresh_error: None,
            });
        }
        let value = call(&mut client, "listBotCommandsLocal", base.clone()).await?;
        let mut bots = Vec::new();
        for (user, role) in &roles {
            if *role != 5 && *role != 6 {
                continue;
            }
            let restricted = *role == 6;
            let settings = if restricted {
                let mut args = base.clone();
                args.push((Value::from("username"), Value::from(user.0.clone())));
                call(&mut client, "getBotMemberSettings", args)
                    .await
                    .ok()
                    .map(|value| parse_settings(&value))
            } else {
                None
            };
            bots.push(BotInfo {
                username: user.0.clone(),
                restricted,
                settings,
            });
        }
        bots.sort_by(|a, b| a.username.cmp(&b.username));
        Ok(BotResult::Loaded(BotSnapshot {
            bots,
            commands: commands(&value),
            can_manage,
        }))
    })
}

async fn call(
    client: &mut KeybaseRpcClient,
    method: &str,
    args: Vec<(Value, Value)>,
) -> io::Result<Value> {
    client
        .call(&format!("{PREFIX}{method}"), vec![Value::Map(args)])
        .await
}
fn commands(value: &Value) -> Vec<BotCommand> {
    map_get(value, "commands")
        .and_then(as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| {
            let name = map_get(v, "name").and_then(as_str)?.to_string();
            Some(BotCommand {
                name,
                description: string(v, "description"),
                usage: string(v, "usage"),
                username: string(v, "username"),
            })
        })
        .collect()
}
fn string(value: &Value, key: &str) -> String {
    map_get(value, key)
        .and_then(as_str)
        .unwrap_or_default()
        .to_string()
}
fn strings(value: &Value, key: &str) -> Vec<String> {
    map_get(value, key)
        .and_then(as_array)
        .into_iter()
        .flatten()
        .filter_map(as_str)
        .map(str::to_string)
        .collect()
}
fn parse_settings(value: &Value) -> BotSettings {
    BotSettings {
        commands: map_get(value, "cmds").and_then(as_bool).unwrap_or(false),
        mentions: map_get(value, "mentions")
            .and_then(as_bool)
            .unwrap_or(false),
        triggers: strings(value, "triggers"),
        conversations: strings(value, "convs"),
    }
}
fn settings_value(settings: &BotSettings) -> Value {
    Value::Map(vec![
        (Value::from("cmds"), Value::from(settings.commands)),
        (Value::from("mentions"), Value::from(settings.mentions)),
        (
            Value::from("triggers"),
            Value::Array(settings.triggers.iter().cloned().map(Value::from).collect()),
        ),
        (
            Value::from("convs"),
            Value::Array(
                settings
                    .conversations
                    .iter()
                    .cloned()
                    .map(Value::from)
                    .collect(),
            ),
        ),
    ])
}

fn can_manage_bots(members_type: Option<i64>, role: Option<i64>) -> bool {
    // Every participant can manage bots in implicit teams (DMs and group DMs).
    matches!(members_type, Some(2 | 3)) || matches!(role, Some(3 | 4))
}
fn featured(offset: usize) -> io::Result<crate::domain::bots::FeaturedBotPage> {
    let path = socket_path().ok_or_else(|| io::Error::other("Keybase is not running"))?;
    Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async move {
            let transport = FramedMsgpackTransport::connect(&path).await?;
            let mut client = KeybaseRpcClient::new(transport);
            let result = client
                .call(
                    "keybase.1.featuredBot.featuredBots",
                    vec![Value::Map(vec![
                        (Value::from("limit"), Value::from(5)),
                        (Value::from("offset"), Value::from(offset as u64)),
                        (Value::from("skipCache"), Value::from(false)),
                    ])],
                )
                .await?;
            Ok(parse_featured(&result))
        })
}
fn parse_featured(value: &Value) -> crate::domain::bots::FeaturedBotPage {
    use crate::domain::bots::{FeaturedBot, FeaturedBotPage};
    let bots = map_get(value, "bots")
        .and_then(as_array)
        .into_iter()
        .flatten()
        .filter_map(|bot| {
            let username = string(bot, "botUsername");
            if username.is_empty() {
                return None;
            }
            let name = string(bot, "botAlias");
            let owner = map_get(bot, "ownerTeam")
                .and_then(as_str)
                .or_else(|| map_get(bot, "ownerUser").and_then(as_str))
                .unwrap_or_default()
                .to_string();
            Some(FeaturedBot {
                name: if name.is_empty() {
                    username.clone()
                } else {
                    name
                },
                username,
                description: string(bot, "description"),
                extended_description: string(bot, "extendedDescriptionRaw"),
                owner,
                rank: map_get(bot, "rank").and_then(as_i64).unwrap_or(0),
            })
        })
        .collect();
    FeaturedBotPage {
        bots,
        is_last_page: map_get(value, "isLastPage")
            .and_then(as_bool)
            .unwrap_or(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dm_participants_can_manage_bots_but_team_writers_cannot() {
        assert!(can_manage_bots(Some(2), Some(2)));
        assert!(can_manage_bots(Some(3), Some(2)));
        assert!(!can_manage_bots(Some(1), Some(2)));
        assert!(can_manage_bots(Some(1), Some(3)));
        assert!(!can_manage_bots(None, None));
    }
    #[test]
    fn featured_catalog_parses_alias_owner_description_and_paging() {
        let value = Value::Map(vec![
            (
                Value::from("bots"),
                Value::Array(vec![Value::Map(vec![
                    (Value::from("botUsername"), Value::from("zoombot")),
                    (Value::from("botAlias"), Value::from("Zoom Meetings")),
                    (Value::from("ownerTeam"), Value::from("keybase")),
                    (
                        Value::from("extendedDescriptionRaw"),
                        Value::from("Type !zoom"),
                    ),
                    (Value::from("rank"), Value::from(100)),
                ])]),
            ),
            (Value::from("isLastPage"), Value::from(false)),
        ]);
        let page = parse_featured(&value);
        assert_eq!(page.bots[0].username, "zoombot");
        assert_eq!(page.bots[0].name, "Zoom Meetings");
        assert_eq!(page.bots[0].owner, "keybase");
        assert!(!page.is_last_page);
    }
    #[test]
    fn settings_round_trip_preserves_custom_triggers_and_channel_scope() {
        let settings = BotSettings {
            commands: true,
            mentions: false,
            triggers: vec!["deploy.*".into()],
            conversations: vec!["abc123".into(), "def456".into()],
        };
        assert_eq!(parse_settings(&settings_value(&settings)), settings);
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    #[test]
    #[ignore = "requires a running Keybase daemon and ZBASE_TEST_BOT_CONVERSATION set to a DM"]
    fn reads_live_dm_bot_permissions() {
        let id = std::env::var("ZBASE_TEST_BOT_CONVERSATION").expect("set a DM conversation ID");
        let BotResult::Loaded(snapshot) =
            execute(&ProviderConversationRef::new(id), BotOperation::Load).expect("load DM bots")
        else {
            panic!("expected snapshot")
        };
        assert!(
            snapshot.can_manage,
            "DM participants must be able to add bots"
        );
    }
    #[test]
    #[ignore = "requires a running Keybase daemon"]
    fn reads_live_featured_catalog() {
        let page = featured(0).expect("read featured catalog");
        let zoom = page
            .bots
            .iter()
            .find(|b| b.username == "zoombot")
            .expect("Zoom in featured bots");
        assert!(!zoom.extended_description.is_empty());
        println!(
            "Featured Zoom: {} by {}. {}",
            zoom.name, zoom.owner, zoom.extended_description
        );
        println!(
            "Catalog: {} bots, last page: {}",
            page.bots.len(),
            page.is_last_page
        );
    }
    #[test]
    #[ignore = "requires a running, logged-in Keybase daemon and ZBASE_TEST_BOT_CONVERSATION"]
    fn reads_live_bot_metadata_without_mutating_membership() {
        let id = std::env::var("ZBASE_TEST_BOT_CONVERSATION").expect("set a conversation ID");
        let conversation = ProviderConversationRef::new(id);
        let result = execute(&conversation, BotOperation::Load).expect("load conversation bots");
        let BotResult::Loaded(snapshot) = result else {
            panic!("expected bot snapshot")
        };
        println!(
            "Read {} bots and {} commands",
            snapshot.bots.len(),
            snapshot.commands.len()
        );
        let lookup = execute(
            &conversation,
            BotOperation::Lookup {
                username: "zoombot".into(),
            },
        )
        .expect("look up ZoomBot");
        let BotResult::Lookup { username, commands } = lookup else {
            panic!("expected lookup")
        };
        assert_eq!(username, "zoombot");
        assert!(commands.iter().any(|c| c.name == "zoom"));
    }
}
