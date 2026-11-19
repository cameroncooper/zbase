//! Join RPC acknowledgements and live confirmations, which arrive on separate sockets.
use super::{
    BackendEvent, MessageRecord, PendingSendMeta, extract_outbox_id_from_post_response,
    stamp_local_attachment_path,
};
use rmpv::Value;
use std::collections::{HashMap, VecDeque};

#[derive(Default)]
pub(super) struct PendingOutboxSends {
    pub pending: HashMap<String, PendingSendMeta>,
    // Also sees sends from other clients. Bound those unmatched confirmations.
    early_confirmations: VecDeque<(String, MessageRecord)>,
}
impl PendingOutboxSends {
    fn register(&mut self, id: String, meta: PendingSendMeta) -> Option<MessageRecord> {
        if let Some(index) = self
            .early_confirmations
            .iter()
            .position(|(key, _)| key == &id)
        {
            return self
                .early_confirmations
                .remove(index)
                .map(|(_, message)| message);
        }
        self.pending.insert(id, meta);
        None
    }

    pub fn confirm(&mut self, id: String, message: &MessageRecord) -> Option<PendingSendMeta> {
        if let Some(meta) = self.pending.remove(&id) {
            return Some(meta);
        }
        self.early_confirmations.retain(|(key, _)| key != &id);
        self.early_confirmations.push_back((id, message.clone()));
        if self.early_confirmations.len() > 128 {
            self.early_confirmations.pop_front();
        }
        None
    }

    pub fn acknowledge(&mut self, response: &Value, meta: PendingSendMeta) -> Option<BackendEvent> {
        let Some(id) = extract_outbox_id_from_post_response(response) else {
            // Named commands may be run by the daemon, which posts any result
            // under a different outbox and returns no outbox id here.
            if let Some(command) = meta.command {
                return Some(BackendEvent::MessageCommandHandled {
                    op_id: meta.op_id,
                    client_message_id: meta.client_message_id,
                    feedback: command.feedback,
                });
            }
            return Some(BackendEvent::MessageSendFailed {
                op_id: meta.op_id,
                client_message_id: meta.client_message_id,
                error:
                    "Keybase did not return an outbox ID. Check the conversation before retrying."
                        .into(),
            });
        };
        let mut message = self.register(id, meta.clone())?;
        if let Some(path) = &meta.local_attachment_path {
            stamp_local_attachment_path(&mut message, path);
        }
        Some(BackendEvent::MessageSendConfirmed {
            op_id: meta.op_id,
            client_message_id: meta.client_message_id,
            server_message: message,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::PendingCommand;
    use super::*;
    use crate::{
        domain::{
            ids::{ConversationId, MessageId, UserId},
            message::MessageSendState,
        },
        state::ids::{ClientMessageId, OpId},
    };

    fn message() -> MessageRecord {
        MessageRecord {
            bot_username: None,
            id: MessageId::new("server-message"),
            conversation_id: ConversationId::new("conversation"),
            author_id: UserId::new("alice_example"),
            reply_to: None,
            thread_root_id: None,
            timestamp_ms: None,
            event: None,
            link_previews: vec![],
            permalink: String::new(),
            fragments: vec![],
            source_text: None,
            attachments: vec![],
            reactions: vec![],
            thread_reply_count: 0,
            send_state: MessageSendState::Sent,
            edited: None,
        }
    }
    fn meta() -> PendingSendMeta {
        PendingSendMeta {
            op_id: OpId::new("operation"),
            client_message_id: ClientMessageId::new("client-message"),
            local_attachment_path: None,
            command: None,
        }
    }
    fn command_meta(feedback: Option<&str>) -> PendingSendMeta {
        PendingSendMeta {
            command: Some(PendingCommand {
                feedback: feedback.map(str::to_string),
            }),
            ..meta()
        }
    }
    fn response() -> Value {
        Value::Map(vec![(Value::from("outboxID"), Value::from("outbox"))])
    }
    /// A zero-valued `PostLocalNonblockRes`, as returned for handled commands.
    fn empty_response() -> Value {
        Value::Map(vec![
            (Value::from("rateLimits"), Value::Nil),
            (Value::from("outboxID"), Value::Nil),
            (Value::from("identifyFailures"), Value::Nil),
        ])
    }

    #[test]
    fn confirmations_complete_sends_in_either_socket_order() {
        let mut pending = PendingOutboxSends::default();
        let message = message();
        // The notification races ahead of the postNonblock RPC response.
        assert!(pending.confirm("outbox".into(), &message).is_none());
        assert!(
            matches!(pending.acknowledge(&response(), meta()), Some(BackendEvent::MessageSendConfirmed { client_message_id, server_message, .. })
            if client_message_id == meta().client_message_id && server_message.id == message.id)
        );
        assert!(pending.pending.is_empty());
        assert!(pending.early_confirmations.is_empty());
        // The ordinary order still waits for the actual live confirmation.
        assert!(pending.acknowledge(&response(), meta()).is_none());
        assert_eq!(pending.pending.len(), 1);
        assert_eq!(
            pending
                .confirm("outbox".into(), &message)
                .unwrap()
                .client_message_id,
            meta().client_message_id
        );
        assert!(pending.pending.is_empty());
    }

    #[test]
    fn malformed_acknowledgement_fails_instead_of_leaving_a_send_pending() {
        let mut pending = PendingOutboxSends::default();
        assert!(matches!(
            pending.acknowledge(&Value::Nil, meta()),
            Some(BackendEvent::MessageSendFailed { .. })
        ));
        // A real but outbox-less response must still fail for ordinary text,
        // including unknown slash-prefixed text such as `/r/rust`.
        assert!(matches!(
            pending.acknowledge(&empty_response(), meta()),
            Some(BackendEvent::MessageSendFailed { .. })
        ));
        assert!(pending.pending.is_empty());
        for id in 0..256 {
            pending.confirm(id.to_string(), &message());
        }
        assert_eq!(pending.early_confirmations.len(), 128);
    }

    #[test]
    fn a_daemon_handled_command_is_not_a_failed_send() {
        let mut pending = PendingOutboxSends::default();
        assert!(matches!(
            pending.acknowledge(
                &empty_response(),
                command_meta(Some("Sent /mute to Keybase"))
            ),
            Some(BackendEvent::MessageCommandHandled { client_message_id, feedback, .. })
                if client_message_id == meta().client_message_id
                    && feedback.as_deref() == Some("Sent /mute to Keybase")
        ));
        // Commands whose output is a message carry no separate feedback.
        assert!(matches!(
            pending.acknowledge(&empty_response(), command_meta(None)),
            Some(BackendEvent::MessageCommandHandled { feedback: None, .. })
        ));
        assert!(pending.pending.is_empty());
    }
}
