//! §9.2 — Relay transport tests with in-memory pipes. PROTECTED FILE: do not edit.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{json, Value};
use std::time::Duration;
use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use zevbox::relay::policy::Policy;
use zevbox::relay::{run_relay, RelayEnd};

const ROOT: &str = "/Users/test/code/proj";
const MAX: usize = 1024;
const T: Duration = Duration::from_secs(5);

/// Test-side handles: write to `zed_tx` as if we were Zed, read what Zed would
/// receive from `zed_rx`; same for the agent side.
struct Harness {
    zed_tx: DuplexStream,
    zed_rx: BufReader<DuplexStream>,
    agent_tx: DuplexStream,
    agent_rx: BufReader<DuplexStream>,
    shutdown: CancellationToken,
    relay: JoinHandle<RelayEnd>,
}

fn start() -> Harness {
    let (zed_tx, relay_client_in) = duplex(64 * 1024);
    let (relay_client_out, zed_rx) = duplex(64 * 1024);
    let (agent_tx, relay_agent_out) = duplex(64 * 1024);
    let (relay_agent_in, agent_rx) = duplex(64 * 1024);
    let shutdown = CancellationToken::new();
    let token = shutdown.clone();
    let relay = tokio::spawn(async move {
        run_relay(
            relay_client_in,
            relay_client_out,
            relay_agent_out,
            relay_agent_in,
            Policy::new(ROOT, "claude"),
            MAX,
            token,
        )
        .await
    });
    Harness {
        zed_tx,
        zed_rx: BufReader::new(zed_rx),
        agent_tx,
        agent_rx: BufReader::new(agent_rx),
        shutdown,
        relay,
    }
}

async fn read_json_line(r: &mut BufReader<DuplexStream>) -> Value {
    let mut line = String::new();
    let n = timeout(T, r.read_line(&mut line))
        .await
        .expect("timeout")
        .unwrap();
    assert!(n > 0, "unexpected EOF");
    assert!(line.ends_with('\n'), "each message must end with \\n");
    assert_eq!(line.matches('\n').count(), 1, "no embedded newline allowed");
    serde_json::from_str(line.trim_end()).expect("relay must only write JSON lines")
}

async fn assert_nothing_arrives(r: &mut BufReader<DuplexStream>) {
    let mut line = String::new();
    let res = timeout(Duration::from_millis(300), r.read_line(&mut line)).await;
    match res {
        Err(_) => {}
        Ok(Ok(0)) => {}
        Ok(other) => panic!("nothing should arrive, got {other:?}: {line}"),
    }
}

async fn end(h: Harness) -> RelayEnd {
    timeout(T, h.relay)
        .await
        .expect("relay did not end")
        .unwrap()
}

fn prompt(id: i64) -> String {
    format!(
        "{}\n",
        json!({"jsonrpc":"2.0","id":id,"method":"session/prompt","params":{"sessionId":"s","prompt":[]}})
    )
}

#[tokio::test]
async fn fragmented_message_reassembled() {
    let mut h = start();
    let msg = prompt(1);
    let (a, rest) = msg.split_at(10);
    let (b, c) = rest.split_at(15);
    for part in [a, b, c] {
        h.zed_tx.write_all(part.as_bytes()).await.unwrap();
        h.zed_tx.flush().await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let got = read_json_line(&mut h.agent_rx).await;
    assert_eq!(got["id"], 1);
    assert_eq!(got["method"], "session/prompt");
}

#[tokio::test]
async fn coalesced_messages_both_processed() {
    let mut h = start();
    let both = format!("{}{}", prompt(1), prompt(2));
    h.zed_tx.write_all(both.as_bytes()).await.unwrap();
    assert_eq!(read_json_line(&mut h.agent_rx).await["id"], 1);
    assert_eq!(read_json_line(&mut h.agent_rx).await["id"], 2);
}

#[tokio::test]
async fn crlf_accepted() {
    let mut h = start();
    let msg = prompt(3).replace('\n', "\r\n");
    h.zed_tx.write_all(msg.as_bytes()).await.unwrap();
    assert_eq!(read_json_line(&mut h.agent_rx).await["id"], 3);
}

#[tokio::test]
async fn empty_lines_ignored() {
    let mut h = start();
    h.zed_tx.write_all(b"\n\n").await.unwrap();
    h.zed_tx.write_all(prompt(4).as_bytes()).await.unwrap();
    assert_eq!(read_json_line(&mut h.agent_rx).await["id"], 4);
}

#[tokio::test]
async fn invalid_json_ends_fatal_without_panic() {
    let mut h = start();
    h.zed_tx.write_all(b"{not json}\n").await.unwrap();
    assert!(matches!(end(h).await, RelayEnd::Fatal(_)));
}

#[tokio::test]
async fn invalid_json_from_agent_ends_fatal() {
    let mut h = start();
    h.agent_tx.write_all(b"Loading...\n").await.unwrap();
    assert!(matches!(end(h).await, RelayEnd::Fatal(_)));
}

#[tokio::test]
async fn non_utf8_ends_fatal() {
    let mut h = start();
    h.zed_tx
        .write_all(&[0xff, 0xfe, 0x7b, b'\n'])
        .await
        .unwrap();
    assert!(matches!(end(h).await, RelayEnd::Fatal(_)));
}

#[tokio::test]
async fn oversized_line_ends_fatal() {
    let mut h = start();
    let big = format!(
        "{}\n",
        json!({"jsonrpc":"2.0","id":1,"method":"session/prompt","params":{"sessionId":"s","prompt":[{"type":"text","text":"x".repeat(MAX * 2)}]}})
    );
    h.zed_tx.write_all(big.as_bytes()).await.unwrap();
    assert!(matches!(end(h).await, RelayEnd::Fatal(_)));
}

#[tokio::test]
async fn batch_array_ends_fatal() {
    let mut h = start();
    h.zed_tx
        .write_all(b"[{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}]\n")
        .await
        .unwrap();
    assert!(matches!(end(h).await, RelayEnd::Fatal(_)));
}

#[tokio::test]
async fn fatal_reason_never_contains_message_content() {
    let mut h = start();
    h.zed_tx
        .write_all(b"{\"secret\": \"TOKEN-abc123\"\n")
        .await
        .unwrap();
    match end(h).await {
        RelayEnd::Fatal(reason) => assert!(!reason.contains("TOKEN-abc123"), "I9: {reason}"),
        other => panic!("expected Fatal, got {other:?}"),
    }
}

#[tokio::test]
async fn client_eof_returns_client_eof() {
    let h = start();
    let Harness { zed_tx, relay, .. } = h;
    drop(zed_tx);
    assert_eq!(
        timeout(T, relay).await.unwrap().unwrap(),
        RelayEnd::ClientEof
    );
}

#[tokio::test]
async fn agent_eof_returns_agent_eof() {
    let h = start();
    let Harness {
        agent_tx, relay, ..
    } = h;
    drop(agent_tx);
    assert_eq!(
        timeout(T, relay).await.unwrap().unwrap(),
        RelayEnd::AgentEof
    );
}

#[tokio::test]
async fn shutdown_token_stops_loop() {
    let h = start();
    h.shutdown.cancel();
    assert_eq!(end(h).await, RelayEnd::Shutdown);
}

#[tokio::test]
async fn blocked_request_never_reaches_client() {
    let mut h = start();
    let m = format!(
        "{}\n",
        json!({"jsonrpc":"2.0","id":"x1","method":"fs/read_text_file","params":{"sessionId":"s","path":"/etc/passwd"}})
    );
    h.agent_tx.write_all(m.as_bytes()).await.unwrap();
    let reply = read_json_line(&mut h.agent_rx).await;
    assert_eq!(reply["id"], "x1");
    assert_eq!(reply["error"]["code"], -32601);
    assert_nothing_arrives(&mut h.zed_rx).await;
}

#[tokio::test]
async fn rejected_session_answered_to_zed_only() {
    let mut h = start();
    let m = format!(
        "{}\n",
        json!({"jsonrpc":"2.0","id":5,"method":"session/new","params":{"cwd":"/etc","mcpServers":[]}})
    );
    h.zed_tx.write_all(m.as_bytes()).await.unwrap();
    let reply = read_json_line(&mut h.zed_rx).await;
    assert_eq!(reply["id"], 5);
    assert_eq!(reply["error"]["code"], -32602);
    assert_nothing_arrives(&mut h.agent_rx).await;
}

#[tokio::test]
async fn full_round_trip_through_relay() {
    let mut h = start();
    let init = json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{"fs":{"readTextFile":true,"writeTextFile":true},"terminal":true}}});
    h.zed_tx
        .write_all(format!("{init}\n").as_bytes())
        .await
        .unwrap();
    let at_agent = read_json_line(&mut h.agent_rx).await;
    assert_eq!(at_agent["params"]["clientCapabilities"]["terminal"], false);
    let resp = json!({"jsonrpc":"2.0","id":0,"result":{"protocolVersion":1,"authMethods":[{"id":"t","name":"t","type":"terminal","args":[]}]}});
    h.agent_tx
        .write_all(format!("{resp}\n").as_bytes())
        .await
        .unwrap();
    let at_zed = read_json_line(&mut h.zed_rx).await;
    assert_eq!(at_zed["id"], 0);
    assert_eq!(at_zed["result"]["authMethods"], json!([]));
}

#[tokio::test]
async fn stdout_only_contains_json_lines() {
    let mut h = start();
    // Mixed traffic; every line Zed receives must be a JSON-RPC object.
    for i in 0..20 {
        let n = json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":format!("chunk {i}\nwith newline inside string")}}}});
        h.agent_tx
            .write_all(format!("{n}\n").as_bytes())
            .await
            .unwrap();
    }
    for _ in 0..20 {
        let v = read_json_line(&mut h.zed_rx).await;
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["method"], "session/update");
    }
}

/// Deterministic xorshift generator (no extra dependency).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[tokio::test]
async fn fuzz_random_bytes_no_panic() {
    let mut rng = Rng(0x9e3779b97f4a7c15);
    let samples: [&[u8]; 6] = [
        b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"",
        b"\"params\":{\"cwd\":\"/../..\"}",
        b"}",
        b"[",
        b"\xff\xfe",
        b"null",
    ];
    for _ in 0..1000 {
        let mut h = start();
        let mut buf = Vec::new();
        for _ in 0..(rng.next() % 8 + 1) {
            if rng.next() % 2 == 0 {
                buf.extend_from_slice(samples[(rng.next() % samples.len() as u64) as usize]);
            } else {
                for _ in 0..(rng.next() % 16) {
                    buf.push((rng.next() & 0xff) as u8);
                }
            }
        }
        buf.push(b'\n');
        let to_agent_side = rng.next() % 2 == 0;
        if to_agent_side {
            let _ = h.agent_tx.write_all(&buf).await;
        } else {
            let _ = h.zed_tx.write_all(&buf).await;
        }
        drop(h.zed_tx);
        drop(h.agent_tx);
        let res = timeout(T, h.relay).await.expect("relay hung");
        assert!(res.is_ok(), "relay task panicked on input {buf:?}");
    }
}
