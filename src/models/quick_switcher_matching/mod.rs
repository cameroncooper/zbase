//! Text evidence determines placement; affinity still determines navigation ranking.
use super::quick_switcher_model::QuickSwitcherUsernameMatch;
#[cfg(test)]
use crate::util::fuzzy::prepare_fuzzy_query;
use crate::{
    domain::conversation::ConversationSummary,
    util::fuzzy::{
        FuzzyMatch, PreparedFuzzyCandidate, PreparedFuzzyQuery, fuzzy_match_prepared,
        fuzzy_match_word_prefixes_prepared, is_word_boundary, prepare_fuzzy_candidate,
    },
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MatchScope {
    Field,
    Combined,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum CandidateSource {
    Display { offset: usize },
    Username(UsernameMatchSource),
    Hidden,
}

#[derive(Clone, Debug)]
pub(crate) struct QuickSwitcherMatchCandidate {
    text: String,
    scope: MatchScope,
    source: CandidateSource,
    prepared: PreparedFuzzyCandidate,
}

/// Hidden usernames determine whether the winning match needs an explanation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct UsernameMatchSource {
    pub usernames: Vec<String>,
    pub group: bool,
}

impl UsernameMatchSource {
    pub fn evidence(
        &self,
        text: &str,
        ranges: &[(usize, usize)],
    ) -> Option<QuickSwitcherUsernameMatch> {
        let mut evidence = QuickSwitcherUsernameMatch {
            text: String::new(),
            ranges: Vec::new(),
            group: self.group,
        };
        let mut offset = 0;
        let mut hidden_match = false;
        for username in text.split(", ") {
            let end = offset + username.len();
            let intersections: Vec<_> = ranges
                .iter()
                .filter_map(|&(start, stop)| {
                    let start = start.max(offset);
                    let stop = stop.min(end);
                    (start < stop).then_some((start, stop))
                })
                .collect();
            if !intersections.is_empty() {
                hidden_match |= self.usernames.iter().any(|hidden| hidden == username);
                if !evidence.text.is_empty() {
                    evidence.text.push_str(", ");
                }
                evidence.text.push('@');
                let destination = evidence.text.len();
                evidence.text.push_str(username);
                evidence
                    .ranges
                    .extend(intersections.into_iter().map(|(start, stop)| {
                        (destination + start - offset, destination + stop - offset)
                    }));
            }
            offset = end + 2;
        }
        hidden_match.then_some(evidence)
    }
}

pub(super) struct QuickSwitcherNavigationMatch {
    pub matched: FuzzyMatch,
    pub strong: bool,
    pub eligible: bool,
    pub username_match: Option<QuickSwitcherUsernameMatch>,
}

struct MatchQuality {
    pub strong: bool,
    pub bonus: i32,
    pub eligible: bool,
}

fn match_quality(
    query: &PreparedFuzzyQuery,
    text: &str,
    ranges: &[(usize, usize)],
    scope: MatchScope,
) -> MatchQuality {
    // Explicit multiword searches may span team/channel or multiple participants.
    // Single tokens spanning fields must consist of word-boundary fragments.
    // Individual fields remain searchable with the existing fuzzy behavior.
    if scope == MatchScope::Combined {
        let boundary_fragments = ranges
            .iter()
            .all(|&(start, _)| is_word_boundary(text, start));
        return MatchQuality {
            // Word-prefix abbreviations are deliberate navigation evidence,
            // regardless of how many names they span or how far apart they are.
            strong: query.is_multi_word || boundary_fragments,
            bonus: 0,
            eligible: query.is_multi_word || boundary_fragments,
        };
    }
    // Compact fuzzy matches within one field remain useful. These thresholds
    // affect placement only; they never reject a single-field match.
    const MAX_INITIALS: usize = 3;
    const MAX_COMPACT_FRAGMENTS: usize = 2;
    const MAX_COMPACT_GAP: usize = 2;
    const EXACT_NAME_BONUS: i32 = 80;
    const PREFIX_NAME_BONUS: i32 = 20;
    let initials = query.char_len <= MAX_INITIALS
        && ranges.iter().all(|&(start, end)| {
            text[start..end].chars().count() == 1 && is_word_boundary(text, start)
        });
    let gaps: usize = ranges
        .windows(2)
        .map(|pair| text[pair[0].1..pair[1].0].chars().count())
        .sum();
    // Keep useful initials and compact abbreviations above messages. Long,
    // scattered subsequences are still available below literal message hits.
    let strong = query.is_multi_word
        || ranges.len() == 1
        || initials
        || (ranges.len() <= MAX_COMPACT_FRAGMENTS && gaps <= MAX_COMPACT_GAP);
    let normalized = text.to_ascii_lowercase();
    let needle: String = query.chars.iter().collect();
    let bonus = if normalized == needle {
        EXACT_NAME_BONUS
    } else if normalized.starts_with(&needle) {
        PREFIX_NAME_BONUS
    } else {
        0
    };
    MatchQuality {
        strong,
        bonus,
        eligible: true,
    }
}

#[cfg(test)]
pub(super) fn quick_switcher_match_for_dm(
    query: &str,
    raw_title: &str,
    display_label: &str,
    profile_names: &HashMap<String, String>,
) -> Option<QuickSwitcherNavigationMatch> {
    let candidates = quick_switcher_dm_match_candidates(raw_title, display_label, profile_names);
    let prepared_query = prepare_fuzzy_query(query)?;
    let mut rejected = 0usize;
    let mut fuzzy_evaluated = 0usize;
    quick_switcher_match_for_candidates_prepared(
        &prepared_query,
        &candidates,
        &mut rejected,
        &mut fuzzy_evaluated,
    )
}

fn quick_switcher_match_candidate(
    text: String,
    source: CandidateSource,
) -> QuickSwitcherMatchCandidate {
    let prepared = prepare_fuzzy_candidate(&text);
    QuickSwitcherMatchCandidate {
        text,
        prepared,
        scope: MatchScope::Combined,
        source,
    }
}

fn quick_switcher_field_candidate(
    text: String,
    offset: Option<usize>,
) -> QuickSwitcherMatchCandidate {
    let mut candidate = quick_switcher_match_candidate(text, CandidateSource::Hidden);
    candidate.scope = MatchScope::Field;
    candidate.source = offset.map_or(CandidateSource::Hidden, |offset| CandidateSource::Display {
        offset,
    });
    candidate
}

pub(super) fn quick_switcher_channel_match_candidates(
    summary: &ConversationSummary,
    label: &str,
) -> Vec<QuickSwitcherMatchCandidate> {
    let mut candidates = vec![quick_switcher_match_candidate(
        label.to_string(),
        CandidateSource::Display { offset: 0 },
    )];
    candidates.push(quick_switcher_field_candidate(
        summary.title.clone(),
        label.strip_suffix(&summary.title).map(str::len),
    ));
    if let Some(group) = &summary.group {
        candidates.push(quick_switcher_field_candidate(
            group.display_name.clone(),
            label.starts_with(&group.display_name).then_some(0),
        ));
    }
    candidates
}

pub(super) fn quick_switcher_dm_match_candidates(
    raw_title: &str,
    display_label: &str,
    profile_names: &HashMap<String, String>,
) -> Vec<QuickSwitcherMatchCandidate> {
    let usernames = quick_switcher_dm_usernames(raw_title);
    if usernames.is_empty() {
        return vec![quick_switcher_field_candidate(
            display_label.to_string(),
            Some(0),
        )];
    }
    let display_names = quick_switcher_dm_display_names(&usernames, profile_names);
    let full_names = usernames
        .iter()
        .map(|username| {
            profile_names
                .get(&username.to_ascii_lowercase())
                .cloned()
                .unwrap_or_else(|| username.clone())
        })
        .collect();
    dm_candidates_with_names(usernames, display_names, full_names, display_label)
}

fn dm_candidates_with_names(
    usernames: Vec<String>,
    display_names: Vec<String>,
    full_names: Vec<String>,
    display_label: &str,
) -> Vec<QuickSwitcherMatchCandidate> {
    let username_label = usernames.join(", ");
    let source = UsernameMatchSource {
        usernames: usernames
            .iter()
            .zip(&display_names)
            .filter(|(username, name)| !username.eq_ignore_ascii_case(name))
            .map(|(username, _)| username.clone())
            .collect(),
        group: usernames.len() > 1,
    };
    let username_candidate = |text: String, scope: MatchScope| QuickSwitcherMatchCandidate {
        prepared: prepare_fuzzy_candidate(&text),
        text,
        scope,
        source: if source.usernames.is_empty() {
            CandidateSource::Hidden
        } else {
            CandidateSource::Username(source.clone())
        },
    };
    let mut candidates = Vec::new();
    candidates.push(quick_switcher_match_candidate(
        display_label.to_string(),
        CandidateSource::Display { offset: 0 },
    ));
    if username_label != display_label {
        candidates.push(username_candidate(username_label, MatchScope::Combined));
    }
    candidates.extend(
        quick_switcher_permutation_labels(&display_names)
            .into_iter()
            .map(|text| quick_switcher_match_candidate(text, CandidateSource::Hidden)),
    );
    candidates.extend(
        quick_switcher_permutation_labels(&usernames)
            .into_iter()
            .map(|text| username_candidate(text, MatchScope::Combined)),
    );
    // Preserve profile-name lookup when Settings displays people by username.
    if full_names != display_names {
        candidates.push(quick_switcher_match_candidate(
            full_names.join(", "),
            CandidateSource::Hidden,
        ));
        candidates.extend(
            quick_switcher_permutation_labels(&full_names)
                .into_iter()
                .map(|text| quick_switcher_match_candidate(text, CandidateSource::Hidden)),
        );
        candidates.extend(
            full_names
                .into_iter()
                .map(|name| quick_switcher_field_candidate(name, None)),
        );
    }
    let mut offset = 0;
    for name in &display_names {
        candidates.push(quick_switcher_field_candidate(name.clone(), Some(offset)));
        offset += name.len() + 2;
    }
    for username in usernames {
        candidates.push(username_candidate(username, MatchScope::Field));
    }
    dedupe_quick_switcher_match_candidates(candidates)
}

fn dedupe_quick_switcher_match_candidates(
    candidates: Vec<QuickSwitcherMatchCandidate>,
) -> Vec<QuickSwitcherMatchCandidate> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for candidate in candidates {
        if seen.insert((
            candidate.text.clone(),
            candidate.scope,
            candidate.source.clone(),
        )) {
            deduped.push(candidate);
        }
    }
    deduped
}

pub(super) fn quick_switcher_match_for_candidates_prepared(
    query: &PreparedFuzzyQuery,
    candidates: &[QuickSwitcherMatchCandidate],
    rejected_candidates: &mut usize,
    fuzzy_evaluated: &mut usize,
) -> Option<QuickSwitcherNavigationMatch> {
    struct CandidateMatch<'a> {
        candidate: &'a QuickSwitcherMatchCandidate,
        matched: FuzzyMatch,
        eligible: bool,
    }
    let mut best: Option<CandidateMatch<'_>> = None;
    let mut strong = false;
    for candidate in candidates {
        if candidate.prepared.char_len < query.char_len {
            *rejected_candidates = rejected_candidates.saturating_add(1);
            continue;
        }
        if query.char_mask & !candidate.prepared.char_mask != 0 {
            *rejected_candidates = rejected_candidates.saturating_add(1);
            continue;
        }
        *fuzzy_evaluated = fuzzy_evaluated.saturating_add(1);
        let Some(mut matched) = fuzzy_match_prepared(query, &candidate.prepared) else {
            continue;
        };
        let mut quality = match_quality(query, &candidate.text, &matched.ranges, candidate.scope);
        if !quality.eligible
            && let Some(boundary_match) =
                fuzzy_match_word_prefixes_prepared(query, &candidate.prepared)
        {
            matched = boundary_match;
            quality = match_quality(query, &candidate.text, &matched.ranges, candidate.scope);
        }
        strong |= quality.eligible && quality.strong;
        matched.score += quality.bonus;
        match best.as_ref() {
            Some(existing)
                if (existing.eligible && !quality.eligible)
                    || (existing.eligible == quality.eligible
                        && existing.matched.score >= matched.score) => {}
            _ => {
                best = Some(CandidateMatch {
                    eligible: quality.eligible,
                    matched,
                    candidate,
                })
            }
        }
    }
    best.map(|best| {
        let mut matched = best.matched;
        let username_match = match &best.candidate.source {
            CandidateSource::Username(source) => {
                source.evidence(&best.candidate.text, &matched.ranges)
            }
            _ => None,
        };
        match best.candidate.source {
            CandidateSource::Display { offset } => {
                for (start, end) in &mut matched.ranges {
                    *start += offset;
                    *end += offset;
                }
            }
            _ => matched.ranges.clear(),
        }
        QuickSwitcherNavigationMatch {
            matched,
            username_match,
            strong,
            eligible: best.eligible,
        }
    })
}

pub(super) fn quick_switcher_dm_usernames(raw_title: &str) -> Vec<String> {
    raw_title
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

pub(super) fn quick_switcher_dm_display_names(
    usernames: &[String],
    profile_names: &HashMap<String, String>,
) -> Vec<String> {
    usernames
        .iter()
        .map(|username| {
            let full_name = profile_names
                .get(&username.to_ascii_lowercase())
                .map_or("", String::as_str);
            crate::domain::user::shown_name(username, full_name).to_string()
        })
        .collect()
}

fn quick_switcher_permutation_labels(parts: &[String]) -> Vec<String> {
    if parts.len() <= 1 {
        return Vec::new();
    }
    let original = parts.join(", ");
    let mut labels = Vec::new();
    let mut reversed = parts.to_vec();
    reversed.reverse();
    let reversed_label = reversed.join(", ");
    if reversed_label != original {
        labels.push(reversed_label);
    }
    let mut sorted = parts.to_vec();
    sorted.sort_by_key(|value| value.to_ascii_lowercase());
    let sorted_label = sorted.join(", ");
    if sorted_label != original && !labels.contains(&sorted_label) {
        labels.push(sorted_label);
    }
    labels
}

#[cfg(test)]
mod tests;
