use super::quick_switcher_matching::quick_switcher_match_for_dm;
use super::*;

fn channel_summary(id: &str, title: &str) -> ConversationSummary {
    ConversationSummary {
        id: ConversationId::new(id),
        title: title.to_string(),
        kind: ConversationKind::Channel,
        topic: String::new(),
        group: None,
        unread_count: 0,
        mention_count: 0,
        muted: false,
        last_activity_ms: 0,
        last_message: None,
    }
}

fn dm_summary(id: &str, title: &str, kind: ConversationKind) -> ConversationSummary {
    ConversationSummary {
        id: ConversationId::new(id),
        title: title.to_string(),
        kind,
        topic: String::new(),
        group: None,
        unread_count: 0,
        mention_count: 0,
        muted: false,
        last_activity_ms: 0,
        last_message: None,
    }
}

fn channel_corpus(channels: &[ConversationSummary]) -> QuickSwitcherSearchCorpus {
    build_quick_switcher_search_corpus(channels, &[], &HashMap::new(), &WorkspaceId::new("test"), 1)
}

#[test]
fn quick_switcher_exact_channel_is_strong_case_insensitive_and_highlighted() {
    let mut ai = channel_summary("ai", "AI");
    ai.group = Some(crate::domain::conversation::ConversationGroup {
        id: "example_network".into(),
        display_name: "example_network".into(),
        team_type: crate::domain::conversation::ConversationTeamType::Complex,
    });
    let corpus = channel_corpus(&[ai, channel_summary("partial", "alerts-prod-mainnet")]);
    for query in ["ai", "AI", "#ai"] {
        let local = compute_quick_switcher_local_results(query, &corpus, None, &[], None, None, 0);
        let first = &local.results[0];
        assert_eq!(first.conversation_id.0, "ai");
        assert!(first.strong_navigation_match);
        assert_eq!(
            first
                .match_ranges
                .iter()
                .map(|&(start, end)| &first.label[start..end])
                .collect::<String>(),
            "AI"
        );
    }
}

#[test]
fn quick_switcher_affinity_can_still_outweigh_an_unfamiliar_exact_name() {
    let corpus = channel_corpus(&[
        channel_summary("exact", "AI"),
        channel_summary("preferred", "ai-planning"),
    ]);
    let affinity = HashMap::from([("preferred".into(), (100.0, 0))]);
    let local =
        compute_quick_switcher_local_results("ai", &corpus, None, &[], None, Some(&affinity), 0);
    assert_eq!(local.results[0].conversation_id.0, "preferred");
    assert!(local.results.iter().all(|r| r.strong_navigation_match));
}

#[test]
fn quick_switcher_butgen_applies_all_boosts_above_literal_message_hits() {
    let dms = [
        dm_summary(
            "butter_gene",
            "butlerbot, gene",
            ConversationKind::GroupDirectMessage,
        ),
        dm_summary(
            "butter_chris_gene",
            "butlerbot, chris, gene",
            ConversationKind::GroupDirectMessage,
        ),
    ];
    let profiles = HashMap::from([
        ("gene".into(), "Gene Example, Jr.".into()),
        ("chris".into(), "Chris Sample".into()),
    ]);
    for (boost, preferred) in [
        ("affinity", "butter_gene"),
        ("affinity", "butter_chris_gene"),
        ("unread", "butter_chris_gene"),
        ("mention", "butter_chris_gene"),
    ] {
        let mut boosted_dms = dms.clone();
        let target = boosted_dms
            .iter_mut()
            .find(|dm| dm.id.0 == preferred)
            .unwrap();
        target.unread_count = u32::from(boost == "unread");
        target.mention_count = u32::from(boost == "mention");
        let corpus = build_quick_switcher_search_corpus(
            &[],
            &boosted_dms,
            &profiles,
            &WorkspaceId::new("test"),
            1,
        );
        let affinity = if boost == "affinity" {
            HashMap::from([(preferred.to_string(), (100.0, 0))])
        } else {
            HashMap::new()
        };
        let local = compute_quick_switcher_local_results(
            "butgen",
            &corpus,
            None,
            &[],
            None,
            Some(&affinity),
            0,
        );
        assert_eq!(
            local.results[0].conversation_id.0, preferred,
            "{boost} must influence ranking among matching groups"
        );
        assert!(local.results.iter().all(|r| r.strong_navigation_match));
        let mut literal_message = local.results[0].clone();
        literal_message.kind = QuickSwitcherResultKind::Message;
        literal_message.conversation_id = ConversationId::new("unrelated_message_conversation");
        literal_message.message_id = Some(MessageId::new("42"));
        literal_message.sublabel = Some("I use butgen to find Butlerbot and Gene".into());
        let mut model = QuickSwitcherModel {
            query: "butgen".into(),
            results: local.results,
            ..Default::default()
        };
        model.results.push(literal_message);
        model.refresh_search_selection();
        let visible = model.search_sections().indices();
        assert_eq!(model.results[visible[0]].conversation_id.0, preferred);
        assert_eq!(model.selected_index, visible[0]);
        assert_eq!(
            model.results[*visible.last().unwrap()].kind,
            QuickSwitcherResultKind::Message
        );
    }
}

#[test]
fn quick_switcher_group_name_gate_rejects_stitched_interior_fragments() {
    let profiles = HashMap::from([
        ("alice".into(), "Alice W Examples".into()),
        ("owen".into(), "Owen Sample".into()),
        ("casey".into(), "casey".into()),
    ]);
    let dms = [dm_summary(
        "group",
        "alice, owen, casey",
        ConversationKind::GroupDirectMessage,
    )];
    let corpus =
        build_quick_switcher_search_corpus(&[], &dms, &profiles, &WorkspaceId::new("test"), 1);
    let weak = compute_quick_switcher_local_results("awesome", &corpus, None, &[], None, None, 0);
    assert!(weak.results.is_empty());
    assert_eq!(
        weak.matched_entry_indices.len(),
        1,
        "keep text matches available for incremental searches"
    );
    let multi =
        compute_quick_switcher_local_results("alice owen", &corpus, None, &[], None, None, 0);
    assert!(multi.results[0].strong_navigation_match);
}

#[test]
fn quick_switcher_keyboard_follows_visible_sections_and_skips_hidden_results() {
    let mut models = AppModels::empty_with_settings(SettingsModel::default());
    models.workspace.channels = vec![
        channel_summary("weak1", "a___w___e___s___o___m___e"),
        channel_summary("weak2", "a___w___e___s___o___m___e2"),
        channel_summary("weak3", "a___w___e___s___o___m___e3"),
        channel_summary("weak4", "a___w___e___s___o___m___e4"),
        channel_summary("strong", "awesome"),
    ];
    models.rebuild_quick_switcher_local_search_corpus();
    models.update_quick_switcher_query("awesome".into());
    let mut message = models.quick_switcher.results[0].clone();
    message.kind = QuickSwitcherResultKind::Message;
    message.message_id = Some(MessageId::new("1"));
    models.quick_switcher.results.push(message);
    let indices = models.quick_switcher.search_sections().indices();
    assert_eq!(
        indices.len(),
        4,
        "three navigation suggestions and a message"
    );
    assert_eq!(models.quick_switcher.selected_index, indices[0]);
    for index in indices.iter().skip(1) {
        models.move_quick_switcher_selection(1);
        assert_eq!(models.quick_switcher.selected_index, *index);
    }
    models.move_quick_switcher_selection(1);
    assert_eq!(
        models.quick_switcher.selected_index,
        *indices.last().unwrap()
    );
    for index in indices.iter().rev().skip(1) {
        models.move_quick_switcher_selection(-1);
        assert_eq!(models.quick_switcher.selected_index, *index);
    }
}

#[test]
fn quick_switcher_winning_username_is_explained_without_highlighting_the_name() {
    let profiles = HashMap::from([("avig".into(), "Avery Grimes".into())]);
    let matched = quick_switcher_match_for_dm("ai", "avig", "Avery Grimes", &profiles).unwrap();
    assert!(
        matched.matched.ranges.is_empty(),
        "username offsets must not highlight a different display name"
    );
    let evidence = matched.username_match.unwrap();
    assert_eq!(evidence.text, "@avig");
    assert!(!evidence.group);
    let letters: String = evidence
        .ranges
        .iter()
        .map(|&(start, end)| &evidence.text[start..end])
        .collect();
    assert_eq!(letters, "ai");
    let name_match =
        quick_switcher_match_for_dm("avery", "avig", "Avery Grimes", &profiles).unwrap();
    assert!(name_match.username_match.is_none());
    assert!(!name_match.matched.ranges.is_empty());
    let alias_only = quick_switcher_match_for_dm(
        "ai",
        "avig",
        "Other Name",
        &HashMap::from([("avig".into(), "Other Name".into())]),
    )
    .unwrap();
    assert_eq!(
        matched.matched.score, alias_only.matched.score,
        "explaining the match must not change its score"
    );
}

#[test]
fn quick_switcher_group_username_explanation_omits_unmatched_participants() {
    let profiles = HashMap::from([
        ("avig".into(), "Avery Grimes".into()),
        ("bob".into(), "Blake Sample".into()),
        ("carol".into(), "Casey Example".into()),
    ]);
    let matched = quick_switcher_match_for_dm(
        "avig",
        "avig, bob, carol",
        "Avery Grimes, Blake Sample, Casey Example",
        &profiles,
    )
    .unwrap();
    let evidence = matched.username_match.unwrap();
    assert!(evidence.group);
    assert_eq!(evidence.text, "@avig");
}

#[test]
fn quick_switcher_dm_match_supports_name_reordering() {
    let mut profile_names = HashMap::new();
    profile_names.insert("alex".to_string(), "Alex Example".to_string());
    profile_names.insert("helperbot".to_string(), "Helper Bot".to_string());

    let matched = quick_switcher_match_for_dm(
        "alhel",
        "helperbot, alex",
        "Helper Bot, Alex Example",
        &profile_names,
    );
    assert!(matched.expect("expected reordered DM match").eligible);
}

#[test]
fn quick_switcher_backend_completion_is_forwarded_to_the_model() {
    let mut models = AppModels::empty_with_settings(SettingsModel::default());
    models.quick_switcher.begin_query("sample".into(), true);
    models.apply_quick_switcher_message_results(&[], false);
    models.quick_switcher.apply_local_results(vec![]);
    assert!(models.quick_switcher.loading_messages);
    models.apply_quick_switcher_message_results(&[], true);
    assert!(!models.quick_switcher.loading_messages);
}
