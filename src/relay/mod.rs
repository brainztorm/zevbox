//! ACP relay between Zed (stdin/stdout) and the agent container (§7).
//! One task, `tokio::select!`, newline-delimited JSON (F4).
//!
//! State is owned by this single task (no Mutex, no Arc) (§7.1).
//! Enforces invariants I3, I4, I5, I6, I9, I11.

pub mod jsonrpc;
pub mod policy;

use futures_util::StreamExt;
use policy::{decide, Direction, Policy, RelayState, Verdict};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio_util::codec::{FramedRead, LinesCodec, LinesCodecError};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayEnd {
    /// Zed closed our stdin.
    ClientEof,
    /// The agent closed its stdout (exited or crashed).
    AgentEof,
    /// `shutdown` was cancelled (signal, `zevbox stop`).
    Shutdown,
    /// Transport or protocol violation (§7.2). The text never contains message content (I9).
    Fatal(String),
}

/// Serializes `msg` as a single-line JSON string followed by `\n` and flushes (F4, §7.1).
async fn send_json<W>(writer: &mut W, msg: &Value) -> Result<(), std::io::Error>
where
    W: AsyncWrite + Unpin,
{
    let encoded = serde_json::to_string(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    writer.write_all(encoded.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

/// Runs until one side ends. Never panics (I11). Writes only JSON-RPC lines to
/// `client_out` and `agent_in`, flushing after every message (F4, §7.1).
pub async fn run_relay<CR, CW, AR, AW>(
    client_in: CR,
    mut client_out: CW,
    agent_out: AR,
    mut agent_in: AW,
    policy: Policy,
    max_message_bytes: usize,
    shutdown: CancellationToken,
) -> RelayEnd
where
    CR: AsyncRead + Unpin,
    CW: AsyncWrite + Unpin,
    AR: AsyncRead + Unpin,
    AW: AsyncWrite + Unpin,
{
    let mut client_lines = FramedRead::new(
        client_in,
        LinesCodec::new_with_max_length(max_message_bytes),
    );
    let mut agent_lines = FramedRead::new(
        agent_out,
        LinesCodec::new_with_max_length(max_message_bytes),
    );

    let mut state = RelayState::default();

    loop {
        if shutdown.is_cancelled() {
            return RelayEnd::Shutdown;
        }

        tokio::select! {
            _ = shutdown.cancelled() => {
                return RelayEnd::Shutdown;
            }

            client_item = client_lines.next() => {
                match client_item {
                    None => {
                        // EOF on Zed's stdin (§7.2).
                        tracing::debug!("client EOF");
                        return RelayEnd::ClientEof;
                    }
                    Some(Err(err)) => {
                        // Oversized line, non-UTF-8, or I/O failure (§7.2).
                        // I9: Reason never contains message content or tokens.
                        tracing::warn!(dir = ?Direction::ClientToAgent, "transport error from client");
                        let reason = match err {
                            LinesCodecError::MaxLineLengthExceeded => "line length exceeded maximum allowed bytes",
                            LinesCodecError::Io(_) => "I/O or invalid UTF-8 data",
                        };
                        return RelayEnd::Fatal(format!("{reason} from client"));
                    }
                    Some(Ok(line)) => {
                        // Ignore empty lines (§7.1).
                        if line.trim().is_empty() {
                            continue;
                        }
                        let msg: Value = match serde_json::from_str(&line) {
                            Ok(v) => v,
                            Err(_) => {
                                // I9: Log direction + size only, never content.
                                tracing::warn!(dir = ?Direction::ClientToAgent, len = line.len(), "invalid JSON from client");
                                return RelayEnd::Fatal(format!(
                                    "invalid JSON from client (length {})",
                                    line.len()
                                ));
                            }
                        };
                        match decide(&policy, &mut state, Direction::ClientToAgent, msg) {
                            Verdict::Forward(val) => {
                                if let Err(e) = send_json(&mut agent_in, &val).await {
                                    tracing::warn!("failed to send forwarded message to agent: {e}");
                                    return RelayEnd::AgentEof;
                                }
                            }
                            Verdict::ReplyToSender(val) => {
                                if let Err(e) = send_json(&mut client_out, &val).await {
                                    tracing::warn!("failed to send reply to client: {e}");
                                    return RelayEnd::ClientEof;
                                }
                            }
                            Verdict::Drop { reason } => {
                                tracing::debug!(reason, "dropped message from client");
                            }
                            Verdict::Fatal { reason } => {
                                tracing::warn!(reason, "fatal violation from client");
                                return RelayEnd::Fatal(reason.to_string());
                            }
                        }
                    }
                }
            }

            agent_item = agent_lines.next() => {
                match agent_item {
                    None => {
                        // EOF on agent's stdout (§7.2).
                        tracing::debug!("agent EOF");
                        return RelayEnd::AgentEof;
                    }
                    Some(Err(err)) => {
                        // Oversized line, non-UTF-8, or I/O failure (§7.2).
                        // I9: Reason never contains message content or tokens.
                        tracing::warn!(dir = ?Direction::AgentToClient, "transport error from agent");
                        let reason = match err {
                            LinesCodecError::MaxLineLengthExceeded => "line length exceeded maximum allowed bytes",
                            LinesCodecError::Io(_) => "I/O or invalid UTF-8 data",
                        };
                        return RelayEnd::Fatal(format!("{reason} from agent"));
                    }
                    Some(Ok(line)) => {
                        // Ignore empty lines (§7.1).
                        if line.trim().is_empty() {
                            continue;
                        }
                        let msg: Value = match serde_json::from_str(&line) {
                            Ok(v) => v,
                            Err(_) => {
                                // I9: Log direction + size only, never content.
                                tracing::warn!(dir = ?Direction::AgentToClient, len = line.len(), "invalid JSON from agent");
                                return RelayEnd::Fatal(format!(
                                    "invalid JSON from agent (length {})",
                                    line.len()
                                ));
                            }
                        };
                        match decide(&policy, &mut state, Direction::AgentToClient, msg) {
                            Verdict::Forward(val) => {
                                if let Err(e) = send_json(&mut client_out, &val).await {
                                    tracing::warn!("failed to send forwarded message to client: {e}");
                                    return RelayEnd::ClientEof;
                                }
                            }
                            Verdict::ReplyToSender(val) => {
                                if let Err(e) = send_json(&mut agent_in, &val).await {
                                    tracing::warn!("failed to send reply to agent: {e}");
                                    return RelayEnd::AgentEof;
                                }
                            }
                            Verdict::Drop { reason } => {
                                tracing::debug!(reason, "dropped message from agent");
                            }
                            Verdict::Fatal { reason } => {
                                tracing::warn!(reason, "fatal violation from agent");
                                return RelayEnd::Fatal(reason.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
}
