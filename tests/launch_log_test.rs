//! `launch` writes the relay/supervisor journal to `logs/<id>.log` (plan §4 step 5,
//! §7.3) and never message content (I9). Not in the protected manifest.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::TestEnv;
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn journal_has_lifecycle_and_blocked_methods_but_no_content() {
    let env = TestEnv::new();
    let mut l = env.launch("normal");
    l.initialize().await;
    l.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "session/new",
        "params": {"cwd": env.project, "mcpServers": []}
    }))
    .await;
    let s = l.recv().await;
    let session = s["result"]["sessionId"].clone();
    // A method that is on no allow-list: refused and journalled by name.
    l.send(json!({"jsonrpc": "2.0", "id": 2, "method": "zed/unknown_thing", "params": {}}))
        .await;
    let r = l.recv().await;
    assert_eq!(r["error"]["code"], -32601, "{r}");
    // A prompt whose text must never reach the journal.
    l.send(json!({
        "jsonrpc": "2.0", "id": 3, "method": "session/prompt",
        "params": {"sessionId": session, "prompt": [{"type": "text", "text": "SECRET-PROMPT-TEXT-9d3f"}]}
    }))
    .await;
    let _ = l.recv().await;
    l.close_stdin();
    let (code, _) = l.wait(Duration::from_secs(10)).await;
    assert_eq!(code, Some(0));

    let logs: Vec<_> = std::fs::read_dir(env.state_root.join("logs"))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert_eq!(logs.len(), 1, "one journal per launch");
    let text = std::fs::read_to_string(logs[0].path()).unwrap();
    assert!(text.contains("launch agent=claude"), "{text}");
    assert!(
        text.contains("blocked method=zed/unknown_thing dir=client_to_agent"),
        "{text}"
    );
    assert!(text.contains("shutdown relay_end=client_eof"), "{text}");
    assert!(!text.contains("SECRET-PROMPT-TEXT"), "I9 violated: {text}");
}
