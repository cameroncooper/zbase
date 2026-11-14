use super::*;
use crate::domain::bots::{BotCommand, BotInfo, BotOperation, BotResult, BotSettings, BotSnapshot};

const PREFIX: &str = "chat.1.local.";

pub(super) fn run(
    sender: Sender<BackendEvent>,
    local_store: Arc<LocalStore>,
    account_id: AccountId,
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
    let mut changed_team = None;
    let mut changed = false;
    let mut result =
        execute(&conversation, operation, &account_id).map_err(|error| error.to_string());
    if let Ok(BotResult::Changed { operation, .. }) = result {
        changed = true;
        changed_team = resolve_team_id_from_conversation_ref_local(&conversation)
            .ok()
            .flatten();
        let (snapshot, refresh_error) =
            match execute(&conversation, BotOperation::Load, &account_id) {
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
        conversation_id: conversation_id.clone(),
        request,
        result,
    });
    if changed {
        let _ = sender.send(BackendEvent::BotsInvalidated {
            team_id: changed_team,
            except: Some(conversation_id.clone()),
        });
        spawn_sync_conversation_team_roles(
            &sender,
            &local_store,
            conversation_id,
            conversation,
            true,
        );
    }
}

fn execute(
    conversation: &ProviderConversationRef,
    operation: BotOperation,
    account_id: &AccountId,
) -> io::Result<BotResult> {
    let raw = provider_ref_to_conversation_id_bytes(conversation)
        .ok_or_else(|| io::Error::other("Missing Keybase conversation ID"))?;
    let path = socket_path().ok_or_else(|| io::Error::other("Keybase is not running"))?;
    // Resolve roles off the UI thread, including implicit teams used by group DMs.
    let lookup = matches!(operation, BotOperation::Lookup { .. });
    let (team_id, roles) = if lookup {
        (None, HashMap::new())
    } else {
        let team = resolve_team_id_from_conversation_ref_local(conversation)?.ok_or_else(|| {
            io::Error::other("Bot membership is unavailable for this conversation")
        })?;
        let roles = fetch_team_roles_for_team(&team)?;
        (Some(team), roles)
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
        let _username = status_username(&status)
            .ok_or_else(|| io::Error::other("Sign in to Keybase to manage bots"))?;
        let inbox = fetch_inbox_for_conversation_id(&mut client, &raw).await?;
        let (team_name, members_type, _) = extract_team_lookup_params_from_inbox_response(&inbox)
            .ok_or_else(|| {
            io::Error::other("Bot membership is unavailable for this conversation")
        })?;
        let can_manage = effective_permissions(&mut client, &team_name, members_type).await?;
        let mutation = !matches!(operation, BotOperation::Load);
        if mutation {
            return mutate(
                &mut client,
                base,
                (&team_name, members_type),
                &roles,
                operation,
                can_manage,
            )
            .await;
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
        let (channels, channels_error) = if members_type == TEAM_MEMBERS_TYPE {
            match team_channels::load_channels(&mut client, &team_name, account_id).await {
                Ok((channels, _)) => (channels.into_iter().map(|c| c.conversation).collect(), None),
                Err(error) => (
                    Vec::new(),
                    Some(format!("Could not load all team channels: {error}")),
                ),
            }
        } else {
            (Vec::new(), None)
        };
        Ok(BotResult::Loaded(BotSnapshot {
            bots,
            commands: commands(&value),
            can_manage,
            team_id,
            channels,
            channels_error,
        }))
    })
}

// The narrow transport seam lets mutation failures and permission races be tested
// without modifying a user's Keybase membership.
trait BotRpc {
    async fn request(&mut self, method: &str, args: Vec<(Value, Value)>) -> io::Result<Value>;
}
impl BotRpc for KeybaseRpcClient {
    async fn request(&mut self, method: &str, args: Vec<(Value, Value)>) -> io::Result<Value> {
        self.call(method, vec![Value::Map(args)]).await
    }
}
async fn call(
    client: &mut impl BotRpc,
    method: &str,
    args: Vec<(Value, Value)>,
) -> io::Result<Value> {
    client.request(&format!("{PREFIX}{method}"), args).await
}
async fn effective_permissions(
    client: &mut impl BotRpc,
    team_name: &str,
    members_type: i64,
) -> io::Result<bool> {
    // All participants manage implicit teams; explicit teams include ancestor admins.
    if matches!(members_type, 2 | 3) {
        return Ok(true);
    }
    let permissions = client
        .request(
            "keybase.1.teams.canUserPerform",
            vec![("name".into(), team_name.into())],
        )
        .await?;
    map_get(&permissions, "manageBots")
        .and_then(as_bool)
        .ok_or_else(|| io::Error::other("Keybase did not return bot-management permissions"))
}
async fn mutate(
    client: &mut impl BotRpc,
    mut args: Vec<(Value, Value)>,
    scope: (&str, i64),
    roles: &HashMap<UserId, i64>,
    operation: BotOperation,
    can_manage: bool,
) -> io::Result<BotResult> {
    let (team_name, members_type) = scope;
    if !can_manage {
        return Ok(BotResult::MutationFailed {
            error: "Your Keybase role does not allow managing bots in this conversation".into(),
            can_manage: Some(false),
        });
    }
    let (method, username) = match &operation {
        BotOperation::Add { username, .. } => {
            if roles.contains_key(&UserId::new(username)) {
                return Err(io::Error::other(
                    "This account is already a team member. Manage its existing membership instead.",
                ));
            }
            ("addBotMember", username)
        }
        BotOperation::Save { username, .. } | BotOperation::Remove { username } => {
            if !roles
                .get(&UserId::new(username))
                .is_some_and(|role| matches!(role, 5 | 6))
            {
                return Err(io::Error::other("This account is not an installed bot"));
            }
            (
                if matches!(operation, BotOperation::Save { .. }) {
                    "editBotMember"
                } else {
                    "removeBotMember"
                },
                username,
            )
        }
        _ => return Err(io::Error::other("Expected a bot membership change")),
    };
    args.push(("username".into(), username.clone().into()));
    if let BotOperation::Add {
        settings,
        restricted,
        ..
    }
    | BotOperation::Save {
        settings,
        restricted,
        ..
    } = &operation
    {
        args.extend([
            ("role".into(), (if *restricted { 6 } else { 5 }).into()),
            (
                "botSettings".into(),
                if *restricted {
                    settings_value(settings)
                } else {
                    Value::Nil
                },
            ),
        ]);
    }
    if let Err(error) = call(client, method, args).await {
        let can_manage = effective_permissions(client, team_name, members_type)
            .await
            .ok();
        return Ok(BotResult::MutationFailed {
            error: error.to_string(),
            can_manage,
        });
    }
    Ok(BotResult::Changed {
        operation,
        snapshot: None,
        refresh_error: None,
    })
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
        let BotResult::Loaded(snapshot) = execute(
            &ProviderConversationRef::new(id),
            BotOperation::Load,
            &AccountId::default(),
        )
        .expect("load DM bots") else {
            panic!("expected snapshot")
        };
        assert!(
            snapshot.can_manage,
            "DM participants must be able to add bots"
        );
    }
    #[test]
    #[ignore = "requires Keybase and ZBASE_TEST_BOT_TEAM_CONVERSATION set to a team channel"]
    fn reads_live_team_permissions_and_full_directory() {
        let id =
            std::env::var("ZBASE_TEST_BOT_TEAM_CONVERSATION").expect("set a team conversation ID");
        let BotResult::Loaded(snapshot) = execute(
            &ProviderConversationRef::new(id),
            BotOperation::Load,
            &AccountId::default(),
        )
        .expect("load team bots") else {
            panic!("expected snapshot")
        };
        assert!(snapshot.team_id.is_some());
        assert!(
            snapshot.channels_error.is_none(),
            "{:?}",
            snapshot.channels_error
        );
        assert!(!snapshot.channels.is_empty());
        println!(
            "Read team permissions, {} bots, and {} directory channels",
            snapshot.bots.len(),
            snapshot.channels.len()
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
        let result = execute(&conversation, BotOperation::Load, &AccountId::default())
            .expect("load conversation bots");
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
            &AccountId::default(),
        )
        .expect("look up ZoomBot");
        let BotResult::Lookup { username, commands } = lookup else {
            panic!("expected lookup")
        };
        assert_eq!(username, "zoombot");
        assert!(commands.iter().any(|c| c.name == "zoom"));
    }
}

#[cfg(test)]
mod mutation_tests;
