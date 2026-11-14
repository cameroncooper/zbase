//! Keybase bot metadata. Commands are sent as ordinary chat messages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BotSettings {
    pub commands: bool,
    pub mentions: bool,
    pub triggers: Vec<String>,
    /// Raw Keybase conversation IDs; empty means all channels.
    pub conversations: Vec<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BotCommand {
    pub name: String,
    pub description: String,
    pub usage: String,
    pub username: String,
}
impl BotCommand {
    pub fn text(&self) -> String {
        format!("!{}", self.name.trim_start_matches('!'))
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BotInfo {
    pub username: String,
    pub restricted: bool,
    pub settings: Option<BotSettings>,
}
impl BotInfo {
    pub fn access_summary(&self) -> &'static str {
        if !self.restricted {
            "All messages in accessible channels"
        } else if let Some(s) = &self.settings {
            match (s.commands, s.mentions, s.triggers.is_empty()) {
                (true, true, true) => "Commands and mentions",
                (true, false, true) => "Commands only",
                (false, true, true) => "Mentions only",
                (_, _, false) => "Custom message access",
                _ => "No message triggers",
            }
        } else {
            "Access settings unavailable"
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct BotSnapshot {
    pub bots: Vec<BotInfo>,
    pub commands: Vec<BotCommand>,
    pub can_manage: bool,
    pub team_id: Option<String>,
    pub channels: Vec<crate::domain::conversation::ConversationSummary>,
    pub channels_error: Option<String>,
}
#[derive(Clone, Debug)]
pub enum BotOperation {
    Featured {
        offset: usize,
    },
    Load,
    Lookup {
        username: String,
    },
    Add {
        username: String,
        settings: BotSettings,
        restricted: bool,
    },
    Save {
        username: String,
        settings: BotSettings,
        restricted: bool,
    },
    Remove {
        username: String,
    },
}
#[derive(Clone, Debug)]
pub enum BotResult {
    Loaded(BotSnapshot),
    Changed {
        operation: BotOperation,
        snapshot: Option<BotSnapshot>,
        refresh_error: Option<String>,
    },
    MutationFailed {
        error: String,
        can_manage: Option<bool>,
    },
    Lookup {
        username: String,
        commands: Vec<BotCommand>,
    },
}
#[derive(Clone, Debug, Default)]
pub struct ConversationBots {
    pub snapshot: BotSnapshot,
    pub request: u64,
    pub loading: bool,
    pub mutation_pending: bool,
    pub failed_mutation: Option<(u64, String)>,
    pub invalidated: bool,
    pub error: Option<String>,
    pub lookup: Option<(String, Vec<BotCommand>)>,
    pub completed_request: u64,
}

impl BotSnapshot {
    pub fn command_enabled(&self, command: &BotCommand, conversation: &str) -> bool {
        self.bots
            .iter()
            .find(|bot| bot.username == command.username)
            .is_some_and(|bot| {
                !bot.restricted
                    || bot.settings.as_ref().is_some_and(|settings| {
                        settings.commands
                            && (settings.conversations.is_empty()
                                || settings.conversations.iter().any(|id| id == conversation))
                    })
            })
    }
}

/// Public catalog supplied by Keybase, independent of installed membership.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FeaturedBot {
    pub username: String,
    pub name: String,
    pub description: String,
    pub extended_description: String,
    pub owner: String,
    pub rank: i64,
}
#[derive(Clone, Debug, Default)]
pub struct FeaturedBotPage {
    pub bots: Vec<FeaturedBot>,
    pub is_last_page: bool,
}
#[derive(Clone, Debug, Default)]
pub struct FeaturedBotCatalog {
    pub bots: Vec<FeaturedBot>,
    pub loading: bool,
    pub error: Option<String>,
    pub request: u64,
    pub next_offset: usize,
    pub all_loaded: bool,
}
impl FeaturedBotCatalog {
    pub fn apply_page(&mut self, offset: usize, page: FeaturedBotPage) {
        if offset == 0 {
            self.bots.clear();
        }
        self.next_offset = offset + page.bots.len();
        self.all_loaded = page.is_last_page || page.bots.is_empty();
        for bot in page.bots {
            if let Some(existing) = self.bots.iter_mut().find(|b| b.username == bot.username) {
                *existing = bot;
            } else {
                self.bots.push(bot);
            }
        }
        self.bots.sort_by_key(|b| std::cmp::Reverse(b.rank));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restricted_command_requires_both_command_permission_and_channel_access() {
        let command = BotCommand {
            username: "meetings".into(),
            name: "zoom".into(),
            ..Default::default()
        };
        let mut snapshot = BotSnapshot {
            bots: vec![BotInfo {
                username: "meetings".into(),
                restricted: true,
                settings: Some(BotSettings {
                    commands: true,
                    conversations: vec!["channel-a".into()],
                    ..Default::default()
                }),
            }],
            ..Default::default()
        };
        assert!(snapshot.command_enabled(&command, "channel-a"));
        assert!(!snapshot.command_enabled(&command, "channel-b"));
        snapshot.bots[0].settings.as_mut().unwrap().commands = false;
        assert!(!snapshot.command_enabled(&command, "channel-a"));
        snapshot.bots[0].settings = None;
        assert!(!snapshot.command_enabled(&command, "channel-a"));
        snapshot.bots[0].restricted = false;
        assert!(snapshot.command_enabled(&command, "channel-b"));
    }
    #[test]
    fn featured_pages_merge_by_username_and_keep_keybase_rank_order() {
        let mut catalog = FeaturedBotCatalog::default();
        catalog.apply_page(
            0,
            FeaturedBotPage {
                bots: vec![
                    FeaturedBot {
                        username: "zoombot".into(),
                        rank: 100,
                        ..Default::default()
                    },
                    FeaturedBot {
                        username: "other".into(),
                        rank: 10,
                        ..Default::default()
                    },
                ],
                is_last_page: false,
            },
        );
        catalog.apply_page(
            2,
            FeaturedBotPage {
                bots: vec![
                    FeaturedBot {
                        username: "other".into(),
                        name: "Updated".into(),
                        rank: 20,
                        ..Default::default()
                    },
                    FeaturedBot {
                        username: "third".into(),
                        rank: 5,
                        ..Default::default()
                    },
                ],
                is_last_page: true,
            },
        );
        assert_eq!(
            catalog
                .bots
                .iter()
                .map(|b| b.username.as_str())
                .collect::<Vec<_>>(),
            vec!["zoombot", "other", "third"]
        );
        assert_eq!(catalog.bots[1].name, "Updated");
        assert_eq!(catalog.next_offset, 4);
        assert!(catalog.all_loaded);
    }
}
