//! JSON-RPC 2.0 helpers for the relay (§7.3).
//!
//! WORKED EXAMPLE (step 4): `classify` is complete.

use serde_json::{json, Value};

pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
/// Generic server error; ACP also uses it for "authentication required".
pub const SERVER_ERROR: i64 = -32000;

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// Has `method` and `id`.
    Request { id: Value, method: String },
    /// Has `method`, no `id`.
    Notification { method: String },
    /// Has `id`, no `method`, and exactly one of `result` / `error`.
    Response { id: Value },
    /// Anything else, including `jsonrpc != "2.0"`, a non-string method,
    /// an `id` that is not a string or a number, or a non-object message.
    Invalid(&'static str),
}

pub fn classify(msg: &Value) -> Kind {
    let Some(obj) = msg.as_object() else {
        // Includes batch arrays: ACP never uses them (§7.2).
        return Kind::Invalid("message is not a JSON object");
    };
    if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Kind::Invalid("jsonrpc must be \"2.0\"");
    }
    let id = obj.get("id");
    if let Some(id) = id {
        if !(id.is_string() || id.is_number()) {
            return Kind::Invalid("id must be a string or a number");
        }
    }
    match (obj.get("method"), id) {
        (Some(Value::String(method)), Some(id)) => Kind::Request {
            id: id.clone(),
            method: method.clone(),
        },
        (Some(Value::String(method)), None) => Kind::Notification {
            method: method.clone(),
        },
        (Some(_), _) => Kind::Invalid("method must be a string"),
        (None, Some(id)) => {
            // Exactly one of result / error.
            if obj.contains_key("result") != obj.contains_key("error") {
                Kind::Response { id: id.clone() }
            } else {
                Kind::Invalid("a response needs exactly one of result or error")
            }
        }
        (None, None) => Kind::Invalid("neither a request, a notification nor a response"),
    }
}

pub fn error_response(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}
