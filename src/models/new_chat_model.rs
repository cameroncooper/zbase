use crate::domain::user::UserSummary;

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct NewChatModel {
    pub open: bool,
    pub is_searching: bool,
    pub search_query: String,
    pub search_results: Vec<UserSummary>,
    pub selected_participants: Vec<UserSummary>,
    pub creating: bool,
    pub error: Option<String>,
    /// The candidate Return adds; view-local, reset whenever the query changes.
    pub highlighted_index: usize,
}

impl NewChatModel {
    /// The highlighted candidate, if the list shows any.
    pub fn highlighted_candidate(&self) -> Option<&UserSummary> {
        if self.search_query.trim().is_empty() {
            return None;
        }
        self.search_results.get(
            self.highlighted_index
                .min(self.search_results.len().saturating_sub(1)),
        )
    }

    pub fn move_highlight(&mut self, delta: isize) {
        let count = self.search_results.len();
        if count == 0 {
            self.highlighted_index = 0;
            return;
        }
        let current = self.highlighted_index.min(count - 1) as isize;
        self.highlighted_index = (current + delta).rem_euclid(count as isize) as usize;
    }
}

/// The start button names who you're about to talk to: "Start chat with Sam",
/// "Start group chat · 3 people".
pub fn start_chat_label(selected: &[UserSummary], creating: bool) -> String {
    if creating {
        return "Starting…".to_string();
    }
    match selected {
        [] => "Start chat".to_string(),
        [person] => {
            let name = person
                .display_name
                .split_whitespace()
                .next()
                .unwrap_or(person.id.0.as_str());
            format!("Start chat with {name}")
        }
        people => format!("Start group chat · {} people", people.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        affinity::Affinity,
        ids::UserId,
        presence::{Availability, Presence},
    };

    fn user(id: &str, name: &str) -> UserSummary {
        UserSummary {
            id: UserId::new(id),
            display_name: name.to_string(),
            title: String::new(),
            avatar_asset: None,
            presence: Presence {
                availability: Availability::Unknown,
                status_text: None,
            },
            affinity: Affinity::None,
        }
    }

    #[test]
    fn start_button_names_the_recipients() {
        assert_eq!(start_chat_label(&[], false), "Start chat");
        assert_eq!(
            start_chat_label(&[user("samr", "Sam Rivera")], false),
            "Start chat with Sam"
        );
        assert_eq!(
            start_chat_label(&[user("samr", "")], false),
            "Start chat with samr"
        );
        assert_eq!(
            start_chat_label(&[user("samr", "Sam"), user("alicej", "Alice")], false),
            "Start group chat · 2 people"
        );
        assert_eq!(start_chat_label(&[user("samr", "Sam")], true), "Starting…");
    }

    #[test]
    fn highlight_wraps_and_needs_a_query() {
        let mut model = NewChatModel {
            search_results: vec![user("a", "A"), user("b", "B")],
            ..Default::default()
        };
        assert!(model.highlighted_candidate().is_none());
        model.search_query = "x".to_string();
        assert_eq!(model.highlighted_candidate().unwrap().id.0, "a");
        model.move_highlight(-1);
        assert_eq!(model.highlighted_candidate().unwrap().id.0, "b");
        model.move_highlight(1);
        assert_eq!(model.highlighted_index, 0);
    }
}
