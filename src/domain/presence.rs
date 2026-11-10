#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Availability {
    Active,
    Away,
    DoNotDisturb,
    Offline,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Presence {
    pub availability: Availability,
    pub status_text: Option<String>,
}
