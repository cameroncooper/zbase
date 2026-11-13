use super::*;
use std::{collections::VecDeque, future::Future};

#[derive(Default)]
struct FakeRpc {
    responses: VecDeque<io::Result<Value>>,
    calls: Vec<(String, Value)>,
}
impl BotRpc for FakeRpc {
    async fn request(&mut self, method: &str, args: Vec<(Value, Value)>) -> io::Result<Value> {
        self.calls.push((method.into(), Value::Map(args)));
        self.responses.pop_front().expect("unexpected RPC")
    }
}
fn run<T>(future: impl Future<Output = T>) -> T {
    Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}
fn permissions(allowed: bool) -> io::Result<Value> {
    Ok(Value::Map(vec![("manageBots".into(), allowed.into())]))
}
fn settings() -> BotSettings {
    BotSettings {
        commands: true,
        mentions: true,
        triggers: vec!["deploy.*".into()],
        conversations: vec!["unjoined-channel-id".into()],
    }
}
fn operation(kind: &str, restricted: bool) -> BotOperation {
    match kind {
        "add" => BotOperation::Add {
            username: "integration".into(),
            restricted,
            settings: settings(),
        },
        "save" => BotOperation::Save {
            username: "integration".into(),
            restricted,
            settings: settings(),
        },
        "remove" => BotOperation::Remove {
            username: "integration".into(),
        },
        _ => unreachable!(),
    }
}
fn roles(role: Option<i64>) -> HashMap<UserId, i64> {
    role.into_iter()
        .map(|role| (UserId::new("integration"), role))
        .collect()
}

#[test]
fn effective_permissions_include_inherited_admins_and_fail_closed() {
    let mut rpc = FakeRpc::default();
    rpc.responses
        .extend([permissions(true), permissions(false), Ok(Value::Nil)]);
    assert!(run(effective_permissions(&mut rpc, "parent.child", 1)).unwrap());
    assert!(!run(effective_permissions(&mut rpc, "parent.child", 1)).unwrap());
    assert!(run(effective_permissions(&mut rpc, "parent.child", 1)).is_err());
    assert_eq!(rpc.calls[0].0, "keybase.1.teams.canUserPerform");
    assert_eq!(
        map_get(&rpc.calls[0].1, "name").and_then(as_str),
        Some("parent.child")
    );
    // DMs use implicit-team participant permissions without a team-name RPC.
    for kind in [2, 3] {
        assert!(run(effective_permissions(&mut rpc, "dm", kind)).unwrap());
    }
    assert_eq!(rpc.calls.len(), 3);
}

#[test]
fn add_and_role_conversions_send_the_selected_role_and_nullable_settings() {
    for (kind, prior_role) in [("add", None), ("save", Some(5)), ("save", Some(6))] {
        for restricted in [true, false] {
            let mut rpc = FakeRpc::default();
            rpc.responses.push_back(Ok(Value::Nil));
            let result = run(mutate(
                &mut rpc,
                vec![("convID".into(), Value::Binary(vec![1, 2]))],
                ("team", 1),
                &roles(prior_role),
                operation(kind, restricted),
                true,
            ))
            .unwrap();
            assert!(matches!(result, BotResult::Changed { .. }));
            assert_eq!(rpc.calls.len(), 1, "mutation must never be retried");
            assert_eq!(
                rpc.calls[0].0,
                format!(
                    "chat.1.local.{}",
                    if kind == "add" {
                        "addBotMember"
                    } else {
                        "editBotMember"
                    }
                )
            );
            let args = &rpc.calls[0].1;
            assert_eq!(
                map_get(args, "role").and_then(as_i64),
                Some(if restricted { 6 } else { 5 })
            );
            assert_eq!(
                map_get(args, "botSettings"),
                Some(&if restricted {
                    settings_value(&settings())
                } else {
                    Value::Nil
                })
            );
        }
    }
}

#[test]
fn failed_install_edit_and_removal_recheck_permissions_without_retrying_writes() {
    for (kind, role) in [("add", None), ("save", Some(6)), ("remove", Some(5))] {
        for still_allowed in [true, false] {
            let mut rpc = FakeRpc::default();
            rpc.responses.extend([
                Err(io::Error::other("mutation denied")),
                permissions(still_allowed),
            ]);
            let result = run(mutate(
                &mut rpc,
                vec![],
                ("team", 1),
                &roles(role),
                operation(kind, true),
                true,
            ))
            .unwrap();
            assert!(
                matches!(result, BotResult::MutationFailed { error, can_manage: Some(allowed) } if error == "mutation denied" && allowed == still_allowed)
            );
            assert_eq!(rpc.calls.len(), 2);
            assert_eq!(rpc.calls[1].0, "keybase.1.teams.canUserPerform");
        }
    }
}

#[test]
fn denied_preflight_does_not_write_and_existing_humans_are_never_converted() {
    for kind in ["add", "save", "remove"] {
        let mut rpc = FakeRpc::default();
        let result = run(mutate(
            &mut rpc,
            vec![],
            ("team", 1),
            &roles(None),
            operation(kind, true),
            false,
        ))
        .unwrap();
        assert!(matches!(
            result,
            BotResult::MutationFailed {
                can_manage: Some(false),
                ..
            }
        ));
        assert!(rpc.calls.is_empty());
        assert!(
            run(mutate(
                &mut rpc,
                vec![],
                ("team", 1),
                &roles(Some(2)),
                operation(kind, true),
                true
            ))
            .is_err()
        );
        assert!(rpc.calls.is_empty());
    }
}

#[test]
fn failed_permission_refresh_preserves_the_original_mutation_error() {
    let mut rpc = FakeRpc::default();
    rpc.responses.extend([
        Err(io::Error::other("original failure")),
        Err(io::Error::other("offline")),
    ]);
    let result = run(mutate(
        &mut rpc,
        vec![],
        ("team", 1),
        &roles(None),
        operation("add", true),
        true,
    ))
    .unwrap();
    assert!(
        matches!(result, BotResult::MutationFailed { error, can_manage: None } if error == "original failure")
    );
}
