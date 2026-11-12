use super::*;
use crate::util::fuzzy::{fuzzy_match, prepare_fuzzy_query};

fn quality(query: &str, text: &str, field: bool) -> MatchQuality {
    let matched = fuzzy_match(query, text).expect("matching fixture");
    match_quality(
        &prepare_fuzzy_query(query).unwrap(),
        text,
        &matched.ranges,
        if field {
            MatchScope::Field
        } else {
            MatchScope::Combined
        },
    )
}

#[test]
fn full_names_remain_searchable_when_the_display_uses_usernames() {
    let candidates = dm_candidates_with_names(
        vec!["alice".into(), "blake".into()],
        vec!["alice".into(), "blake".into()],
        vec!["Alice Example".into(), "Blake Sample".into()],
        "alice, blake",
    );
    let query = prepare_fuzzy_query("alice example").unwrap();
    let matched =
        quick_switcher_match_for_candidates_prepared(&query, &candidates, &mut 0, &mut 0).unwrap();
    assert!(matched.eligible);
    assert!(
        matched.matched.ranges.is_empty(),
        "profile-name offsets cannot highlight usernames"
    );
    assert!(matched.username_match.is_none());
}

#[test]
fn username_evidence_keeps_only_contributing_usernames_and_rebases_ranges() {
    let source = UsernameMatchSource {
        usernames: vec!["andi".into(), "gene".into()],
        group: true,
    };
    let text = "gene, ignored, andi";
    let andi = text.find("andi").unwrap();
    let evidence = source.evidence(text, &[(0, 2), (andi, andi + 2)]).unwrap();
    assert_eq!(evidence.text, "@gene, @andi");
    let letters: String = evidence
        .ranges
        .iter()
        .map(|&(start, end)| &evidence.text[start..end])
        .collect();
    assert_eq!(letters, "gean");
    assert!(evidence.group);
    assert!(
        source.evidence(text, &[(6, 8)]).is_none(),
        "an unmatched hidden username needs no explanation"
    );
}

#[test]
fn username_evidence_explains_all_parts_of_a_mixed_visible_and_hidden_match() {
    let source = UsernameMatchSource {
        usernames: vec!["gene".into()],
        group: true,
    };
    let text = "butlerbot, gene";
    let gene = text.find("gene").unwrap();
    let evidence = source.evidence(text, &[(0, 3), (gene, gene + 3)]).unwrap();
    assert_eq!(evidence.text, "@butlerbot, @gene");
    let letters: String = evidence
        .ranges
        .iter()
        .map(|&(start, end)| &evidence.text[start..end])
        .collect();
    assert_eq!(letters, "butgen");
    assert!(source.evidence(text, &[(0, 3)]).is_none());
}

#[test]
fn username_evidence_preserves_utf8_offsets() {
    let source = UsernameMatchSource {
        usernames: vec!["élo".into()],
        group: false,
    };
    let evidence = source.evidence("élo", &[(0, 2)]).unwrap();
    assert_eq!(
        &evidence.text[evidence.ranges[0].0..evidence.ranges[0].1],
        "é"
    );
}

#[test]
fn scattered_awesome_matches_are_weak_even_when_they_exist() {
    assert!(
        !quality(
            "awesome",
            "example_network.ent_wallet #alerts-prod-mainnet",
            false
        )
        .strong
    );
    assert!(!quality("awesome", "Alice W Examples, Owen Sample, casey", false).strong);
    assert!(!quality("awesome", "a___w___e___s___o___m___e", true).strong);
    assert!(quality("awesome", "awesome", true).strong);
}

#[test]
fn field_boundaries_distinguish_names_from_combined_labels() {
    assert!(quality("ai", "example_network #AI", false).strong);
    assert!(quality("ai", "AI", true).strong);
    assert!(quality("example ai", "example_network #AI", false).strong);
    assert!(quality("alice owen", "Alice W Examples, Owen Sample", false).strong);
}

#[test]
fn useful_initials_and_compact_abbreviations_remain_strong() {
    assert!(quality("ag", "Alice Green", true).strong);
    assert!(quality("alce", "Alice", true).strong);
    assert!(!quality("alce", "Alxxxxxxxxce", true).strong);
}

#[test]
fn exact_and_prefix_bonuses_are_bounded_and_exclusive() {
    assert_eq!(quality("ai", "AI", true).bonus, 80);
    assert_eq!(quality("ai", "ai-planning", true).bonus, 20);
    assert_eq!(quality("ai", "internal-ai", true).bonus, 0);
    assert_eq!(quality("ai", "AI", false).bonus, 0);
}

#[test]
fn gate_removes_cross_field_interior_stitching_only() {
    for text in [
        "example_network.ent_wallet #alerts-prod-mainnet",
        "example_network.projectnemo #customer-requests",
        "Alice W Examples, Owen Sample, casey",
    ] {
        assert!(!quality("awesome", text, false).eligible, "{text}");
    }
    // No global segment-count or score cutoff on a real field.
    assert!(quality("awesome", "a___w___e___s___o___m___e", true).eligible);
    assert!(quality("alce", "Alxxxxxxxxce", true).eligible);
    assert!(quality("ai", "Avery Grimes", true).eligible);
}

#[test]
fn gate_preserves_prefix_abbreviations_initials_and_multiword_searches() {
    assert!(quality("alhel", "Alex Example, Helper Bot", false).eligible);
    assert!(quality("ao", "Alice Example, Owen Sample", false).eligible);
    assert!(quality("exampleai", "example_network #AI", false).eligible);
    assert!(quality("alice owen", "Alice Example, Owen Sample", false).eligible);
    assert!(quality("example ai", "example_network #AI", false).eligible);
}

#[test]
fn name_prefixes_are_strong_without_a_fragment_count_limit() {
    for (query, text) in [
        ("butgen", "butlerbot, Gene Example, Jr."),
        ("butgen", "butlerbot, Chris Sample, Gene Example, Jr."),
        ("alhel", "Alex Example, Helper Bot"),
        ("ao", "Alice Example, Owen Sample"),
        ("butchrgen", "butlerbot, Chris Sample, Gene Example, Jr."),
    ] {
        let matched = quality(query, text, false);
        assert!(matched.eligible && matched.strong, "{query}: {text}");
    }
    // Initials use the same boundary evidence, without a length cutoff.
    let initials = quality("abcdef", "Alice Bob Chris Dan Eve Frank", false);
    assert!(initials.eligible);
    assert!(initials.strong);
}
