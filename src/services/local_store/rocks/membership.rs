use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

const LEFT_PREFIX: &str = "left_conversation:";
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(0);

impl LocalStore {
    pub fn set_conversation_left(&self, id: &ConversationId, left: bool) -> io::Result<()> {
        self.transition_conversation_membership(id, left, None, || {})
            .map(|_| ())
    }

    /// Capture before starting an RPC. Notifications supersede its completion.
    pub fn conversation_membership_generation(&self, id: &ConversationId) -> io::Result<u64> {
        let generations = self
            .membership_writes
            .lock()
            .map_err(|_| io::Error::other("membership lock poisoned"))?;
        Ok(*generations.get(id).unwrap_or(&0))
    }

    /// Serialize persistence and publication so accepted transitions reach the UI
    /// in the same order. The callback must not start another membership write.
    pub fn transition_conversation_membership(
        &self,
        id: &ConversationId,
        left: bool,
        expected_generation: Option<u64>,
        publish: impl FnOnce(),
    ) -> io::Result<bool> {
        let mut generations = self
            .membership_writes
            .lock()
            .map_err(|_| io::Error::other("membership lock poisoned"))?;
        let generation = *generations.get(id).unwrap_or(&0);
        if expected_generation.is_some_and(|expected| expected != generation) {
            return Ok(false);
        }
        let meta = self.cf(CF_META)?;
        let key = format!("{LEFT_PREFIX}{}", id.0);
        let result = if left {
            let token = format!(
                "{:?}:{}",
                SystemTime::now(),
                NEXT_TOKEN.fetch_add(1, Ordering::Relaxed)
            );
            self.db
                .put_cf(&meta, key, token.as_bytes())
                .map_err(io::Error::other)
        } else {
            self.db.delete_cf(&meta, key).map_err(io::Error::other)
        };
        // Even a failed persistence attempt supersedes older RPC completions.
        generations.insert(id.clone(), generation.wrapping_add(1));
        result?;
        publish();
        Ok(true)
    }

    pub fn is_conversation_left(&self, id: &ConversationId) -> io::Result<bool> {
        self.db
            .get_cf(&self.cf(CF_META)?, format!("{LEFT_PREFIX}{}", id.0))
            .map(|value| value.is_some())
            .map_err(io::Error::other)
    }

    /// Capture before requesting the server inbox. A stale response cannot clear a newer leave.
    pub fn left_conversation_snapshot(&self) -> io::Result<HashMap<ConversationId, Vec<u8>>> {
        let _guard = self
            .membership_writes
            .lock()
            .map_err(|_| io::Error::other("membership lock poisoned"))?;
        let meta = self.cf(CF_META)?;
        let mut result = HashMap::new();
        for row in self.db.iterator_cf(
            &meta,
            IteratorMode::From(LEFT_PREFIX.as_bytes(), Direction::Forward),
        ) {
            let (key, value) = row.map_err(io::Error::other)?;
            let Some(id) = key.strip_prefix(LEFT_PREFIX.as_bytes()) else {
                break;
            };
            let id = std::str::from_utf8(id).map_err(io::Error::other)?;
            result.insert(ConversationId::new(id), value.to_vec());
        }
        Ok(result)
    }

    /// Capture before requesting the server inbox. Lists the cached channels whose
    /// membership this process has not seen change, so only the cache says they are joined.
    pub fn unconfirmed_channel_snapshot(&self) -> io::Result<HashSet<ConversationId>> {
        let generations = self
            .membership_writes
            .lock()
            .map_err(|_| io::Error::other("membership lock poisoned"))?;
        let mut result = HashSet::new();
        for id in self.load_cached_conversation_ids(usize::MAX)? {
            if !generations.contains_key(&id)
                && self
                    .get_conversation(&id)?
                    .is_some_and(|summary| summary.kind == ConversationKind::Channel)
            {
                result.insert(id);
            }
        }
        Ok(result)
    }

    /// A leave made where zBase was not listening (another Keybase client, or while
    /// zBase was closed) only shows as a cached channel missing from the active inbox.
    /// Channels whose membership changed after `snapshot` keep that newer state.
    pub fn retire_departed_channels(
        &self,
        snapshot: &HashSet<ConversationId>,
        active_ids: impl IntoIterator<Item = ConversationId>,
        mut publish: impl FnMut(&ConversationId),
    ) -> io::Result<Vec<ConversationId>> {
        let active = active_ids.into_iter().collect::<HashSet<_>>();
        let mut retired = Vec::new();
        for id in snapshot.difference(&active) {
            if self.transition_conversation_membership(id, true, Some(0), || publish(id))? {
                retired.push(id.clone());
            }
        }
        Ok(retired)
    }

    pub fn reconcile_joined_conversations(
        &self,
        snapshot: &HashMap<ConversationId, Vec<u8>>,
        active_ids: impl IntoIterator<Item = ConversationId>,
    ) -> io::Result<Vec<ConversationId>> {
        let mut generations = self
            .membership_writes
            .lock()
            .map_err(|_| io::Error::other("membership lock poisoned"))?;
        let meta = self.cf(CF_META)?;
        let mut joined = Vec::new();
        for id in active_ids {
            let Some(token) = snapshot.get(&id) else {
                continue;
            };
            let key = format!("{LEFT_PREFIX}{}", id.0);
            if self
                .db
                .get_cf(&meta, &key)
                .map_err(io::Error::other)?
                .as_ref()
                == Some(token)
            {
                self.db.delete_cf(&meta, key).map_err(io::Error::other)?;
                let generation = generations.entry(id.clone()).or_default();
                *generation = generation.wrapping_add(1);
                joined.push(id);
            }
        }
        Ok(joined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leave_survives_stale_cache_writes_and_restart_without_deleting_history() {
        let path = std::env::temp_dir().join(format!(
            "zbase-membership-{}-{}",
            std::process::id(),
            NEXT_TOKEN.fetch_add(1, Ordering::Relaxed)
        ));
        let id = ConversationId::new("channel");
        {
            let store = LocalStore::open_at(path.clone()).unwrap();
            let summary = ConversationSummary {
                id: id.clone(),
                title: "dev".into(),
                kind: ConversationKind::Channel,
                topic: String::new(),
                group: None,
                unread_count: 1,
                mention_count: 0,
                muted: false,
                last_activity_ms: 1,
                last_message: None,
            };
            let message = super::super::tests::sample_message(&id, 1);
            let payload = BootstrapPayload {
                workspace_ids: vec![WorkspaceId::new("ws")],
                active_workspace_id: Some(WorkspaceId::new("ws")),
                workspace_name: "test".into(),
                channels: vec![summary.clone()],
                direct_messages: vec![],
                workspace_bindings: vec![],
                conversation_bindings: vec![],
                message_bindings: vec![],
                selected_conversation_id: Some(id.clone()),
                selected_messages: vec![message.clone()],
                unread_marker: None,
                account_display_name: None,
                conversation_notifications: Vec::new(),
            };
            store.persist_bootstrap_payload(&payload).unwrap();
            store.set_conversation_left(&id, true).unwrap();
            store.persist_bootstrap_payload(&payload).unwrap(); // stale network/cache write
            assert!(store.get_conversation(&id).unwrap().is_none());
            assert!(store.load_bootstrap_seed(20, 20).unwrap().is_none());
            assert_eq!(
                store.get_message(&id, &message.id).unwrap().unwrap().id,
                message.id
            );
        }
        {
            let store = LocalStore::open_at(path.clone()).unwrap();
            assert!(store.is_conversation_left(&id).unwrap());
            let snapshot = store.left_conversation_snapshot().unwrap();
            store.set_conversation_left(&id, true).unwrap();
            assert!(
                store
                    .reconcile_joined_conversations(&snapshot, [id.clone()])
                    .unwrap()
                    .is_empty()
            );
            let snapshot = store.left_conversation_snapshot().unwrap();
            assert_eq!(
                store
                    .reconcile_joined_conversations(&snapshot, [id.clone()])
                    .unwrap(),
                vec![id.clone()]
            );
            assert!(!store.is_conversation_left(&id).unwrap());
            store.set_conversation_left(&id, true).unwrap();
            store.set_conversation_left(&id, false).unwrap();
            assert!(!store.is_conversation_left(&id).unwrap());
            assert!(store.get_conversation(&id).unwrap().is_some());
            assert_eq!(
                store
                    .load_bootstrap_seed(20, 20)
                    .unwrap()
                    .unwrap()
                    .channels
                    .len(),
                1
            );
        }
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn channels_missing_from_the_inbox_are_retired_unless_membership_moved_on() {
        let path = std::env::temp_dir().join(format!(
            "zbase-membership-{}-{}",
            std::process::id(),
            NEXT_TOKEN.fetch_add(1, Ordering::Relaxed)
        ));
        let store = LocalStore::open_at(path.clone()).unwrap();
        let summary = |id: &str, kind: ConversationKind| ConversationSummary {
            id: ConversationId::new(id),
            title: id.into(),
            kind,
            topic: String::new(),
            group: None,
            unread_count: 3,
            mention_count: 0,
            muted: false,
            last_activity_ms: 1,
            last_message: None,
        };
        for (id, kind) in [
            ("left_elsewhere", ConversationKind::Channel),
            ("still_joined", ConversationKind::Channel),
            ("joined_this_session", ConversationKind::Channel),
            ("joined_during_fetch", ConversationKind::Channel),
            ("dm", ConversationKind::DirectMessage),
        ] {
            store.persist_conversation(&summary(id, kind), 1).unwrap();
        }
        let id = ConversationId::new;
        store
            .set_conversation_left(&id("joined_this_session"), false)
            .unwrap();
        let snapshot = store.unconfirmed_channel_snapshot().unwrap();
        assert_eq!(
            snapshot,
            HashSet::from([
                id("left_elsewhere"),
                id("still_joined"),
                id("joined_during_fetch")
            ])
        );
        store
            .set_conversation_left(&id("joined_during_fetch"), false)
            .unwrap();

        let mut published = Vec::new();
        let retired = store
            .retire_departed_channels(&snapshot, [id("still_joined")], |id| {
                published.push(id.clone())
            })
            .unwrap();
        assert_eq!(retired, vec![id("left_elsewhere")]);
        assert_eq!(published, retired);
        assert!(
            store
                .get_conversation(&id("left_elsewhere"))
                .unwrap()
                .is_none()
        );
        for kept in [
            "still_joined",
            "joined_this_session",
            "joined_during_fetch",
            "dm",
        ] {
            assert!(store.get_conversation(&id(kept)).unwrap().is_some());
        }

        // Rejoining elsewhere later brings it back through the usual reconcile.
        let left = store.left_conversation_snapshot().unwrap();
        assert_eq!(
            store
                .reconcile_joined_conversations(&left, [id("left_elsewhere")])
                .unwrap(),
            vec![id("left_elsewhere")]
        );
        assert!(
            store
                .get_conversation(&id("left_elsewhere"))
                .unwrap()
                .is_some()
        );
        std::fs::remove_dir_all(path).unwrap();
    }
}
