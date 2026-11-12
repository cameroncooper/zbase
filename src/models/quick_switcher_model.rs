use crate::domain::{
    ids::{ConversationId, MessageId, UserId},
    route::Route,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuickSwitcherResultKind {
    UnreadChannel,
    Channel,
    DirectMessage,
    Message,
}

#[derive(Clone, Debug)]
pub struct QuickSwitcherUsernameMatch {
    pub text: String,
    pub ranges: Vec<(usize, usize)>,
    pub group: bool,
}

#[derive(Clone, Debug)]
pub struct QuickSwitcherResult {
    pub label: String,
    pub sublabel: Option<String>,
    pub kind: QuickSwitcherResultKind,
    pub route: Route,
    pub conversation_id: ConversationId,
    pub message_id: Option<MessageId>,
    pub match_ranges: Vec<(usize, usize)>,
    pub username_match: Option<QuickSwitcherUsernameMatch>,
    pub strong_navigation_match: bool,
    /// For message hits: who wrote it and when, shown like a Search result.
    pub author_id: Option<UserId>,
    pub timestamp_ms: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct QuickSwitcherModel {
    pub query: String,
    pub results: Vec<QuickSwitcherResult>,
    pub selected_index: usize,
    pub loading_messages: bool,
    pub selection_is_explicit: bool,
}

#[derive(Default)]
pub struct QuickSwitcherSections {
    pub strong: Vec<usize>,
    pub messages: Vec<usize>,
    pub weak: Vec<usize>,
}

impl QuickSwitcherSections {
    pub fn indices(&self) -> Vec<usize> {
        self.strong
            .iter()
            .chain(&self.messages)
            .chain(&self.weak)
            .copied()
            .collect()
    }
}

const NAVIGATION_RESULT_LIMIT: usize = 3;
const MESSAGE_RESULT_LIMIT: usize = 10;

impl QuickSwitcherModel {
    pub fn begin_query(&mut self, query: String, search_messages: bool) {
        self.query = query;
        self.selection_is_explicit = false;
        self.loading_messages = search_messages;
        self.results
            .retain(|result| result.kind != QuickSwitcherResultKind::Message);
        self.refresh_search_selection();
    }

    /// Shared by rendering, default selection, and arrow-key navigation.
    pub fn search_sections(&self) -> QuickSwitcherSections {
        let mut sections = QuickSwitcherSections::default();
        for (index, result) in self.results.iter().enumerate() {
            if result.kind == QuickSwitcherResultKind::Message {
                if sections.messages.len() < MESSAGE_RESULT_LIMIT {
                    sections.messages.push(index);
                }
            } else if result.strong_navigation_match
                && sections.strong.len() < NAVIGATION_RESULT_LIMIT
            {
                sections.strong.push(index);
            }
        }
        for (index, result) in self.results.iter().enumerate() {
            if result.kind != QuickSwitcherResultKind::Message
                && !result.strong_navigation_match
                && sections.strong.len() + sections.weak.len() < NAVIGATION_RESULT_LIMIT
            {
                sections.weak.push(index);
            }
        }
        sections
    }

    pub fn refresh_search_selection(&mut self) {
        let visible = self.search_sections().indices();
        if !self.selection_is_explicit || !visible.contains(&self.selected_index) {
            self.selected_index = visible.first().copied().unwrap_or_default();
            self.selection_is_explicit = false;
        }
    }

    pub fn apply_local_results(&mut self, mut results: Vec<QuickSwitcherResult>) {
        // Local results never change the remote request's loading state.
        self.update_results(|current| {
            results.extend(
                current
                    .drain(..)
                    .filter(|r| r.kind == QuickSwitcherResultKind::Message),
            );
            *current = results;
        });
    }

    pub fn apply_message_results(&mut self, results: Vec<QuickSwitcherResult>, is_complete: bool) {
        self.update_results(|current| {
            current.retain(|r| r.kind != QuickSwitcherResultKind::Message);
            current.extend(results);
        });
        self.loading_messages = !is_complete;
    }

    fn update_results(&mut self, update: impl FnOnce(&mut Vec<QuickSwitcherResult>)) {
        let selected = self
            .results
            .get(self.selected_index)
            .map(|r| (r.conversation_id.clone(), r.message_id.clone()));
        update(&mut self.results);
        if let Some(index) = selected.and_then(|selected| {
            self.results
                .iter()
                .position(|r| (r.conversation_id.clone(), r.message_id.clone()) == selected)
        }) {
            self.selected_index = index;
        } else {
            self.selection_is_explicit = false;
        }
        self.refresh_search_selection();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: &str, strong: bool, message: bool) -> QuickSwitcherResult {
        QuickSwitcherResult {
            label: id.into(),
            sublabel: None,
            kind: if message {
                QuickSwitcherResultKind::Message
            } else {
                QuickSwitcherResultKind::Channel
            },
            route: Route::WorkspaceHome {
                workspace_id: crate::domain::ids::WorkspaceId::new("w"),
            },
            conversation_id: ConversationId::new(id),
            message_id: message.then(|| MessageId::new("1")),
            match_ranges: vec![],
            username_match: None,
            strong_navigation_match: strong,
            author_id: None,
            timestamp_ms: None,
        }
    }

    #[test]
    fn sections_put_literal_messages_before_weak_navigation() {
        let model = QuickSwitcherModel {
            results: vec![
                hit("weak", false, false),
                hit("strong", true, false),
                hit("message", false, true),
            ],
            ..Default::default()
        };
        assert_eq!(model.search_sections().indices(), vec![1, 2, 0]);
    }

    #[test]
    fn strong_suggestions_cannot_be_crowded_out_by_three_weak_ones() {
        let model = QuickSwitcherModel {
            results: vec![
                hit("w1", false, false),
                hit("w2", false, false),
                hit("w3", false, false),
                hit("s", true, false),
            ],
            ..Default::default()
        };
        assert_eq!(model.search_sections().indices(), vec![3, 0, 1]);
    }

    #[test]
    fn late_local_results_keep_messages_and_explicit_selection() {
        let mut model = QuickSwitcherModel {
            results: vec![hit("message", false, true)],
            selection_is_explicit: true,
            ..Default::default()
        };
        model.apply_local_results(vec![hit("strong", true, false), hit("weak", false, false)]);
        assert_eq!(model.search_sections().indices(), vec![0, 2, 1]);
        assert_eq!(model.selected_index, 2);
        model.selection_is_explicit = false;
        model.refresh_search_selection();
        assert_eq!(model.selected_index, 0);
    }

    #[test]
    fn partial_message_search_stays_loading_when_local_results_arrive() {
        let mut model = QuickSwitcherModel::default();
        model.begin_query("sample".into(), true);
        model.apply_message_results(vec![hit("message", false, true)], false);
        model.apply_local_results(vec![hit("channel", true, false)]);
        assert!(model.loading_messages);
        assert_eq!(model.results.len(), 2);
        assert_eq!(model.results[model.selected_index].label, "channel");
        model.apply_message_results(vec![hit("message", false, true)], true);
        assert!(!model.loading_messages);
    }

    #[test]
    fn completed_message_search_stays_complete_when_local_results_arrive() {
        let mut model = QuickSwitcherModel::default();
        model.begin_query("sample".into(), true);
        model.apply_message_results(vec![], true);
        model.apply_local_results(vec![hit("channel", true, false)]);
        assert!(!model.loading_messages);
    }

    #[test]
    fn new_query_discards_old_messages_and_explicit_selection() {
        let mut model = QuickSwitcherModel::default();
        model.begin_query("old".into(), true);
        model.apply_local_results(vec![hit("channel", true, false)]);
        model.apply_message_results(vec![hit("message", false, true)], true);
        model.selected_index = 1;
        model.selection_is_explicit = true;
        model.begin_query("new".into(), true);
        assert!(model.loading_messages);
        assert!(!model.selection_is_explicit);
        assert_eq!(model.results.len(), 1);
        assert_eq!(model.selected_index, 0);
    }
}
