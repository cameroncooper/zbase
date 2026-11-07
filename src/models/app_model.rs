use crate::domain::ids::{UserId, WorkspaceId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Connectivity {
    Online,
    Reconnecting,
    Offline,
}

/// What sending does while Keybase can't be reached, worded once so the banner,
/// the composer hint and the send button's tooltip never disagree.
///
/// A send while disconnected is not queued: the adapter can't reach the
/// Keybase service, the message is marked "Not sent" and keeps a Retry. The
/// listener reconnects on its own with backoff, so there is nothing for the
/// user to press; the copy says so instead of offering a "Try again".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SendNotice {
    pub banner_title: &'static str,
    pub banner_detail: &'static str,
    /// Under the focused composer while offline.
    pub composer_hint: Option<&'static str>,
    /// On the send button, in place of "Send".
    pub send_tooltip: Option<&'static str>,
}

impl Connectivity {
    /// `None` while connected.
    pub fn send_notice(&self) -> Option<SendNotice> {
        match self {
            Self::Online => None,
            Self::Reconnecting => Some(SendNotice {
                banner_title: "Connecting to Keybase…",
                banner_detail: "New messages will show up once the connection is back.",
                composer_hint: None,
                send_tooltip: None,
            }),
            Self::Offline => Some(SendNotice {
                banner_title: "You're offline",
                banner_detail: "Messages can't be sent until Keybase reconnects.",
                composer_hint: Some("Offline · messages can't be sent until Keybase reconnects"),
                send_tooltip: Some("Offline · sending fails until Keybase reconnects"),
            }),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AppModel {
    pub open_workspaces: Vec<WorkspaceId>,
    pub active_workspace_id: WorkspaceId,
    pub connectivity: Connectivity,
    pub global_unread_count: u32,
    pub current_user_id: Option<UserId>,
    pub current_user_display_name: String,
    pub current_user_avatar_asset: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::Connectivity;

    #[test]
    fn online_has_no_send_notice() {
        assert_eq!(Connectivity::Online.send_notice(), None);
    }

    #[test]
    fn offline_copy_is_consistent_and_never_promises_a_queue() {
        let notice = Connectivity::Offline.send_notice().expect("offline notice");
        let hint = notice.composer_hint.expect("offline composer hint");
        let tooltip = notice.send_tooltip.expect("offline send tooltip");
        // Sends while disconnected fail (and keep Retry); none of the copy may
        // suggest they are held and delivered later.
        for line in [notice.banner_detail, hint, tooltip] {
            let lower = line.to_lowercase();
            assert!(!lower.contains("will send"), "{line}");
            assert!(!lower.contains("queued"), "{line}");
            assert!(lower.contains("reconnect"), "{line}");
        }
        assert!(hint.starts_with("Offline"));
        assert!(tooltip.starts_with("Offline"));
    }

    #[test]
    fn connecting_does_not_change_the_composer() {
        let notice = Connectivity::Reconnecting
            .send_notice()
            .expect("connecting notice");
        assert_eq!(notice.composer_hint, None);
        assert_eq!(notice.send_tooltip, None);
    }
}
