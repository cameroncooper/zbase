#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub enum Affinity {
    #[default]
    None,
    Positive,
    Broken,
}
