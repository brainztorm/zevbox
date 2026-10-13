#!/usr/bin/env python3
"""Fake ACP agent used by lifecycle tests (stdlib only). PROTECTED FILE.

Modes (FAKE_AGENT_MODE):
  normal    answers initialize/session/*, tries host access during session/prompt
  stubborn  like normal, but ignores stdin EOF (only `container stop` ends it)
  crash     exits with code 3 right after answering initialize
  garbage   writes a non-JSON line to stdout right after answering initialize
Every received message is appended to FAKE_AGENT_LOG as {"kind": ..., "msg": ...}.
"""
import json
import os
import sys
import time

MODE = os.environ.get("FAKE_AGENT_MODE", "normal")
LOG = os.environ.get("FAKE_AGENT_LOG")


def log(kind, msg):
    if LOG:
        with open(LOG, "a") as f:
            f.write(json.dumps({"kind": kind, "msg": msg}) + "\n")


def send(msg):
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()


INIT_RESULT = {
    "protocolVersion": 1,
    "agentCapabilities": {"loadSession": True},
    "authMethods": [
        {
            "id": "claude-ai-login",
            "name": "Claude Subscription",
            "type": "terminal",
            "args": ["--cli", "auth", "login", "--claudeai"],
            "_meta": {"terminal-auth": {"command": "/usr/local/bin/node", "args": ["x"], "label": "Claude Login"}},
        },
        {"id": "agent-login", "name": "Agent login"},
    ],
}

sys.stderr.write("fake-agent: started mode=%s\n" % MODE)
sys.stderr.flush()

prompt_id = None
waiting = set()

while True:
    line = sys.stdin.readline()
    if not line:
        log("eof", None)
        if MODE == "stubborn":
            while True:
                time.sleep(1)
        sys.exit(0)
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method = msg.get("method")
    if method is None:
        log("response", msg)
        waiting.discard(msg.get("id"))
        if prompt_id is not None and not waiting:
            send({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "s1", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "done"}}}})
            send({"jsonrpc": "2.0", "id": prompt_id, "result": {"stopReason": "end_turn"}})
            prompt_id = None
        continue
    log("request" if "id" in msg else "notification", msg)
    if "id" not in msg:
        continue
    mid = msg["id"]
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": mid, "result": INIT_RESULT})
        if MODE == "crash":
            sys.exit(3)
        if MODE == "garbage":
            sys.stdout.write("this is not json\n")
            sys.stdout.flush()
    elif method in ("session/new", "session/load"):
        send({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": "s1"}})
    elif method == "session/prompt":
        prompt_id = mid
        waiting = {"a1", "a2", "a3"}
        send({"jsonrpc": "2.0", "id": "a1", "method": "fs/read_text_file", "params": {"sessionId": "s1", "path": "/etc/hosts"}})
        send({"jsonrpc": "2.0", "id": "a2", "method": "terminal/create", "params": {"sessionId": "s1", "command": "id"}})
        send({"jsonrpc": "2.0", "id": "a3", "method": "session/request_permission", "params": {"sessionId": "s1", "toolCall": {"toolCallId": "t1"}, "options": []}})
    else:
        send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": "fake: unknown method"}})
