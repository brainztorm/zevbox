//! Relay rules added for Codex: ACP version and `api-key`. Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{json, Value};
use zevbox::relay::policy::{decide, Direction, Policy, RelayState, Verdict};

fn init(version: Value) -> Value {
    json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{
        "protocolVersion": version,
        "capabilities": {"auth": {"terminal": {}}},
        "clientCapabilities": {"terminal": true}
    }})
}

fn answer(version: Value) -> Value {
    json!({"jsonrpc":"2.0","id":0,"result":{
        "protocolVersion": version,
        "authMethods": [
            {"id":"api-key","name":"API Key","_meta":{"api-key":{"provider":"openai"}}},
            {"id":"other","name":"Other"}
        ]
    }})
}

fn c2a(p: &Policy, s: &mut RelayState, m: Value) -> Verdict {
    decide(p, s, Direction::ClientToAgent, m)
}

fn a2c(p: &Policy, s: &mut RelayState, m: Value) -> Verdict {
    decide(p, s, Direction::AgentToClient, m)
}

fn codex() -> Policy {
    Policy::new("/p", "codex")
}

#[test]
fn uncovered_client_version_never_reaches_the_agent() {
    for version in [json!(2), json!(0), json!("1"), Value::Null] {
        let mut s = RelayState::default();
        match c2a(&codex(), &mut s, init(version.clone())) {
            Verdict::ReplyToSender(reply) => {
                let text = reply["error"]["message"].as_str().unwrap();
                assert!(text.contains("is not filtered by this zevbox"), "{text}");
                assert!(text.contains("Fix:"), "{text}");
            }
            other => panic!("{version}: {other:?}"),
        }
    }
}

#[test]
fn uncovered_agent_version_becomes_an_error_for_zed() {
    let mut s = RelayState::default();
    assert!(matches!(
        c2a(&codex(), &mut s, init(json!(1))),
        Verdict::Forward(_)
    ));
    match a2c(&codex(), &mut s, answer(json!(2))) {
        Verdict::Forward(reply) => {
            assert!(reply.get("result").is_none(), "{reply}");
            assert!(reply["error"]["message"]
                .as_str()
                .unwrap()
                .contains("is not filtered by this zevbox"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn agent_error_to_initialize_is_forwarded_untouched() {
    let mut s = RelayState::default();
    c2a(&codex(), &mut s, init(json!(1)));
    let error = json!({"jsonrpc":"2.0","id":0,"error":{"code":-32603,"message":"boom"}});
    assert_eq!(
        a2c(&codex(), &mut s, error.clone()),
        Verdict::Forward(error)
    );
}

#[test]
fn only_the_api_key_method_of_codex_is_masked() {
    let mut s = RelayState::default();
    c2a(&codex(), &mut s, init(json!(1)));
    let Verdict::Forward(out) = a2c(&codex(), &mut s, answer(json!(1))) else {
        panic!("not forwarded");
    };
    assert_eq!(
        out["result"]["authMethods"],
        json!([{"id":"other","name":"Other"}])
    );
    let auth =
        json!({"jsonrpc":"2.0","id":1,"method":"authenticate","params":{"methodId":"api-key"}});
    match c2a(&codex(), &mut s, auth) {
        Verdict::ReplyToSender(reply) => assert!(reply["error"]["message"]
            .as_str()
            .unwrap()
            .contains("zevbox auth codex")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn other_agents_keep_their_methods() {
    let claude = Policy::new("/p", "claude");
    let mut s = RelayState::default();
    c2a(&claude, &mut s, init(json!(1)));
    let Verdict::Forward(out) = a2c(&claude, &mut s, answer(json!(1))) else {
        panic!("not forwarded");
    };
    assert_eq!(out["result"]["authMethods"].as_array().unwrap().len(), 2);
}
