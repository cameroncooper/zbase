use crate::domain::search::{SearchFilter, SearchResult};

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct SearchModel {
    pub query: String,
    pub filters: Vec<SearchFilter>,
    pub results: Vec<SearchResult>,
    /// The query the current results answer, once it has been submitted.
    pub submitted_query: Option<String>,
    pub highlighted_index: Option<usize>,
    pub is_loading: bool,
}
