//! ACP relay between Zed (stdin/stdout) and the agent container (§7).
//! One task, `tokio::select!`, newline-delimited JSON (F4).

pub mod jsonrpc;
pub mod policy;

use policy::Policy;
use tokio::io::{AsyncRead, AsyncWrite};
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

/// Runs until one side ends. Never panics. Writes only JSON-RPC lines to
/// `client_out` and `agent_in`, flushing after every message.
pub async fn run_relay<CR, CW, AR, AW>(
    client_in: CR,
    client_out: CW,
    agent_out: AR,
    agent_in: AW,
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
    let _ = (
        client_in,
        client_out,
        agent_out,
        agent_in,
        policy,
        max_message_bytes,
        shutdown,
    );
    RelayEnd::Fatal("not implemented yet: relay::run_relay".to_string())
}
