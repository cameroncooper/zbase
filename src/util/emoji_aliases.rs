//! Shared stock-emoji resolution. Keybase aliases take precedence over library
//! names; conversation-specific custom emoji remain the renderer's first choice.
use emojis::SkinTone;
use std::{collections::HashMap, sync::OnceLock};

/// Keybase appends a tone to a stock emoji's name: `:+1::skin-tone-3:`.
const KEYBASE_SKIN_TONE: &str = "::skin-tone-";

pub fn standard_emoji_for_alias(alias: &str) -> Option<&'static str> {
    static LOOKUP: OnceLock<HashMap<String, &'static str>> = OnceLock::new();
    if let Some((base, tone)) = alias.trim().trim_matches(':').split_once(KEYBASE_SKIN_TONE) {
        let base = emojis::get(standard_emoji_for_alias(base)?)?;
        let toned = skin_tone_for_keybase_number(tone).and_then(|tone| base.with_skin_tone(tone));
        return Some(toned.unwrap_or(base).as_str());
    }
    let key = normalize_emoji_alias(alias);
    if key.is_empty() {
        return None;
    }
    LOOKUP
        .get_or_init(|| {
            let mut lookup = HashMap::new();
            for emoji in emojis::iter() {
                lookup
                    .entry(normalize_emoji_alias(emoji.name()))
                    .or_insert(emoji.as_str());
                for shortcode in emoji.shortcodes() {
                    lookup
                        .entry(normalize_emoji_alias(shortcode))
                        .or_insert(emoji.as_str());
                }
            }
            for (alias, unicode) in keybase_aliases() {
                lookup.insert(normalize_emoji_alias(alias), unicode);
            }
            lookup
        })
        .get(&key)
        .copied()
}

/// A reaction written the way Keybase's clients write it, so ours and theirs
/// count together: `👍` becomes `:+1:` and `👍🏼` becomes `:+1::skin-tone-3:`.
/// Anything Keybase has no name for, like a custom `:alias:`, is kept as is.
pub fn keybase_reaction(emoji: &str) -> String {
    let emoji = emoji.trim();
    keybase_shortcode_for_unicode(emoji).unwrap_or_else(|| emoji.to_string())
}

fn keybase_shortcode_for_unicode(unicode: &str) -> Option<String> {
    static ALIAS_BY_UNICODE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    let emoji = emojis::get(unicode)?;
    let (base, tone) = match emoji.skin_tone() {
        None | Some(SkinTone::Default) => (emoji, None),
        Some(tone) => (
            emoji.with_skin_tone(SkinTone::Default)?,
            Some(keybase_number_for_skin_tone(tone)?),
        ),
    };
    let alias = ALIAS_BY_UNICODE
        .get_or_init(|| {
            let mut aliases = keybase_aliases().iter().collect::<Vec<_>>();
            aliases.sort();
            let mut by_unicode = HashMap::new();
            for (alias, unicode) in aliases {
                if let Some(emoji) = emojis::get(unicode) {
                    by_unicode.entry(emoji.as_str()).or_insert(*alias);
                }
            }
            by_unicode
        })
        .get(base.as_str())?;
    Some(match tone {
        Some(number) => format!(":{alias}{KEYBASE_SKIN_TONE}{number}:"),
        None => format!(":{alias}:"),
    })
}

fn keybase_aliases() -> &'static HashMap<&'static str, &'static str> {
    static ALIASES: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    ALIASES.get_or_init(|| {
        serde_json::from_str(include_str!("../../assets/emoji/keybase-shortcodes.json"))
            .expect("valid embedded Keybase emoji aliases")
    })
}

/// Keybase numbers the five tones 2 to 6, after the Fitzpatrick scale.
const KEYBASE_SKIN_TONES: [(SkinTone, u8); 5] = [
    (SkinTone::Light, 2),
    (SkinTone::MediumLight, 3),
    (SkinTone::Medium, 4),
    (SkinTone::MediumDark, 5),
    (SkinTone::Dark, 6),
];

fn keybase_number_for_skin_tone(tone: SkinTone) -> Option<u8> {
    KEYBASE_SKIN_TONES
        .iter()
        .find(|(candidate, _)| *candidate == tone)
        .map(|(_, number)| *number)
}

fn skin_tone_for_keybase_number(number: &str) -> Option<SkinTone> {
    let number = number.parse::<u8>().ok()?;
    KEYBASE_SKIN_TONES
        .iter()
        .find(|(_, candidate)| *candidate == number)
        .map(|(tone, _)| *tone)
}

pub fn normalize_emoji_alias(alias: &str) -> String {
    let mut out = String::with_capacity(alias.len());
    let mut previous_underscore = false;
    for ch in alias.trim().trim_matches(':').chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            previous_underscore = false;
            Some(ch.to_ascii_lowercase())
        } else if ch == '+' {
            previous_underscore = false;
            Some('+')
        } else if matches!(ch, '_' | '-' | ' ') {
            if previous_underscore {
                None
            } else {
                previous_underscore = true;
                Some('_')
            }
        } else if previous_underscore {
            None
        } else {
            previous_underscore = true;
            Some('_')
        };
        if let Some(value) = normalized {
            out.push(value);
        }
    }
    out.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_keybase_standard_alias_matches_the_embedded_table() {
        let aliases: HashMap<&str, &str> =
            serde_json::from_str(include_str!("../../assets/emoji/keybase-shortcodes.json"))
                .unwrap();
        for (alias, expected) in aliases {
            assert_eq!(standard_emoji_for_alias(alias), Some(expected), "{alias}");
        }
    }

    #[test]
    fn keybase_aliases_and_library_names_share_one_unicode_resolver() {
        for (alias, expected) in [
            (":large_green_circle:", "🟢"),
            ("green_circle", "🟢"),
            ("large_red_square", "🟥"),
            ("large_orange_circle", "🟠"),
            ("rolling_on_the_floor_laughing", "🤣"),
            ("+1", "👍"),
            ("-1", "👎"),
        ] {
            assert_eq!(standard_emoji_for_alias(alias), Some(expected), "{alias}");
        }
        assert_eq!(
            standard_emoji_for_alias("team_specific_unknown_emoji"),
            None
        );
    }

    #[test]
    fn reactions_are_sent_by_keybase_name_with_its_skin_tone_suffix() {
        assert_eq!(keybase_reaction("👍"), ":+1:");
        assert_eq!(keybase_reaction("❤️"), ":heart:");
        assert_eq!(keybase_reaction("\u{2764}"), ":heart:");
        assert_eq!(keybase_reaction("👍🏻"), ":+1::skin-tone-2:");
        assert_eq!(keybase_reaction("👍🏿"), ":+1::skin-tone-6:");
        assert_eq!(keybase_reaction(":party_parrot:"), ":party_parrot:");
        // Keybase has no form for two tones in one emoji.
        assert_eq!(keybase_reaction("🧑🏻‍🤝‍🧑🏿"), "🧑🏻‍🤝‍🧑🏿");
    }

    #[test]
    fn keybase_skin_tone_suffix_resolves_to_the_toned_emoji() {
        assert_eq!(standard_emoji_for_alias(":+1::skin-tone-3:"), Some("👍🏼"));
        assert_eq!(standard_emoji_for_alias("+1::skin-tone-6"), Some("👍🏿"));
        assert_eq!(standard_emoji_for_alias(":heart::skin-tone-3:"), Some("❤️"));
        assert_eq!(standard_emoji_for_alias(":nope::skin-tone-3:"), None);
        for unicode in ["👍", "👍🏽", "🎉", "🫶🏾"] {
            assert_eq!(
                standard_emoji_for_alias(&keybase_reaction(unicode)),
                Some(unicode),
                "{unicode}"
            );
        }
    }
}
