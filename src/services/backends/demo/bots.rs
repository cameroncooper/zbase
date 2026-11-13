use super::*;
use crate::domain::bots::{BotCommand, BotInfo, BotOperation, BotResult};

impl DemoBackend {
    pub(super) fn bot_events(
        &mut self,
        conversation_id: ConversationId,
        request: u64,
        operation: BotOperation,
    ) -> Vec<BackendEvent> {
        if let BotOperation::Featured { offset } = operation {
            return vec![BackendEvent::FeaturedBotsResult {
                request,
                offset,
                result: Ok(demo::demo_featured_bots()),
            }];
        }
        let snapshot = self
            .bots
            .entry(conversation_id.clone())
            .or_insert_with(|| demo::demo_bot_snapshot(&conversation_id));
        let result = match operation {
            BotOperation::Load => Ok(BotResult::Loaded(snapshot.clone())),
            BotOperation::Lookup { username } => {
                let command = public_command(&username);
                command
                    .map(|command| BotResult::Lookup {
                        username,
                        commands: vec![command],
                    })
                    .ok_or_else(|| "No bot matches that username.".to_string())
            }
            operation => {
                match &operation {
                    BotOperation::Add { username, settings } => {
                        snapshot.bots.push(BotInfo {
                            username: username.clone(),
                            restricted: true,
                            settings: Some(settings.clone()),
                        });
                        if let Some(command) = public_command(username) {
                            snapshot.commands.push(command);
                        }
                    }
                    BotOperation::Save { username, settings } => {
                        if let Some(bot) =
                            snapshot.bots.iter_mut().find(|b| &b.username == username)
                        {
                            bot.settings = Some(settings.clone());
                        }
                    }
                    BotOperation::Remove { username } => {
                        snapshot.bots.retain(|b| &b.username != username);
                        snapshot.commands.retain(|c| &c.username != username);
                    }
                    _ => unreachable!(),
                }
                Ok(BotResult::Changed {
                    operation,
                    snapshot: Some(snapshot.clone()),
                    refresh_error: None,
                })
            }
        };
        vec![BackendEvent::BotsResult {
            conversation_id,
            request,
            result,
        }]
    }
}
fn public_command(username: &str) -> Option<BotCommand> {
    let (name, description) = match username {
        "zoombot" => ("zoom", "Create a Zoom meeting"),
        "buildbot" => ("build", "Check the latest build"),
        "reminderbot" => ("remind", "Set a reminder for your team"),
        _ => return None,
    };
    Some(BotCommand {
        name: name.into(),
        username: username.into(),
        description: description.into(),
        usage: format!("!{name}"),
    })
}
