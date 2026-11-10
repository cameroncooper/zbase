use crate::domain::{ids::ConversationId, search::SearchResult};

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct FindInChatModel {
    pub open: bool,
    pub query: String,
    pub loading: bool,
    pub conversation_id: Option<ConversationId>,
    pub anchor_timestamp_ms: Option<i64>,
    pub query_seq: u64,
    pub results: Vec<SearchResult>,
    pub selected_index: Option<usize>,
}

impl FindInChatModel {
    pub fn open_for_conversation(
        &mut self,
        conversation_id: ConversationId,
        anchor_timestamp_ms: Option<i64>,
    ) {
        self.open = true;
        self.query.clear();
        self.loading = false;
        self.conversation_id = Some(conversation_id);
        self.anchor_timestamp_ms = anchor_timestamp_ms;
        self.results.clear();
        self.selected_index = None;
    }

    pub fn close(&mut self) {
        // Ignore results still in flight if search is immediately reopened.
        self.query_seq = self.query_seq.wrapping_add(1);
        self.open = false;
        self.query.clear();
        self.loading = false;
        self.conversation_id = None;
        self.anchor_timestamp_ms = None;
        self.results.clear();
        self.selected_index = None;
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.loading = !self.query.trim().is_empty();
        self.results.clear();
        self.selected_index = None;
    }

    pub fn apply_results(&mut self, mut results: Vec<SearchResult>, is_complete: bool) {
        self.sort_and_rotate_results(&mut results);
        self.results = results;
        if self.results.is_empty() {
            self.selected_index = None;
        } else if let Some(selected) = self.selected_index {
            self.selected_index = Some(selected.min(self.results.len().saturating_sub(1)));
        }
        self.loading = !is_complete;
    }

    pub fn select_next(&mut self) -> Option<SearchResult> {
        if self.results.is_empty() {
            self.selected_index = None;
            return None;
        }
        let next = match self.selected_index {
            Some(current) => (current + 1) % self.results.len(),
            None => 0,
        };
        self.selected_index = Some(next);
        self.results.get(next).cloned()
    }

    pub fn select_previous(&mut self) -> Option<SearchResult> {
        if self.results.is_empty() {
            self.selected_index = None;
            return None;
        }
        let prev = match self.selected_index {
            Some(0) | None => self.results.len().saturating_sub(1),
            Some(current) => current.saturating_sub(1),
        };
        self.selected_index = Some(prev);
        self.results.get(prev).cloned()
    }

    fn sort_and_rotate_results(&self, results: &mut [SearchResult]) {
        results.sort_by(|left, right| {
            left.message
                .timestamp_ms
                .unwrap_or(i64::MIN)
                .cmp(&right.message.timestamp_ms.unwrap_or(i64::MIN))
                .then_with(|| left.message.id.0.cmp(&right.message.id.0))
        });
        let Some(anchor) = self.anchor_timestamp_ms else {
            return;
        };
        let Some(rotation_start) = results
            .iter()
            .position(|result| result.message.timestamp_ms.unwrap_or(i64::MIN) > anchor)
        else {
            return;
        };
        results.rotate_left(rotation_start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_and_reopening_find_invalidates_in_flight_results() {
        let mut find = FindInChatModel::default();
        let conversation = ConversationId::new("chat");
        find.open_for_conversation(conversation.clone(), Some(100));
        find.set_query("older message".into());
        let request_sequence = find.query_seq;
        find.selected_index = Some(0);

        find.close();
        assert!(!find.open);
        assert!(!find.loading);
        assert!(find.query.is_empty());
        assert!(find.conversation_id.is_none());
        assert!(find.anchor_timestamp_ms.is_none());
        assert!(find.selected_index.is_none());

        find.open_for_conversation(conversation, Some(200));
        assert_ne!(find.query_seq, request_sequence);
        assert!(find.results.is_empty());
        assert!(find.query.is_empty());
        assert!(!find.loading);
    }
}
