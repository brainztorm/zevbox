#!/usr/bin/env python3
"""Checks `zevbox launch codex` through the real relay, with a synthetic key.

Usage, inside the isolated environment only:
    ( . "$W/env.sh"; python3 docs/codex_annex/verify_launch.py "$W" )
Prints one PASS or FAIL per check, then "verify_launch: ALL OK" or exits 1.
Refuses to run on a real installation: it replaces state/codex/.codex/auth.json.
"""
import glob
import json
import os
import queue
import subprocess
import sys
import threading
import time

W = sys.argv[1]
ROOT = os.environ.get("ZEVBOX_STATE_ROOT", "")
if not W.startswith("/private/tmp/zevbox-iso.") or ROOT != W + "/state":
    sys.exit("refused: run it inside the environment made by isolated_env.sh")

BIN = W + "/zevbox-bin"
PROJECT = W + "/home/code/proj"
CACHE = ROOT + "/state/codex/.codex/auth.json"
KEY = "sk-zevbox-synthetic-7f3a"
MARKER = "ZEVBOX-PROMPT-MARKER-91c2"
ZED_CAPABILITIES = {
    "fs": {"readTextFile": True, "writeTextFile": True},
    "terminal": True,
    "auth": {"terminal": True},
    "elicitation": {"form": {}, "url": {}},
    "session": {"configOptions": {"boolean": {}}},
    "_meta": {"terminal_output": True, "terminal-auth": True},
}
failed = 0


def check(name, ok, detail=""):
    global failed
    print(("PASS " if ok else "FAIL ") + name + ("" if ok else ": " + str(detail)[:300]))
    failed += 0 if ok else 1


def start():
    process = subprocess.Popen(
        [BIN, "launch", "codex", "--project", PROJECT],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        text=True, bufsize=1,
    )
    lines = queue.Queue()
    threading.Thread(target=lambda: [lines.put(l) for l in process.stdout], daemon=True).start()
    return process, lines


def send(process, message):
    process.stdin.write(json.dumps(message) + "\n")
    process.stdin.flush()


def answer(lines, wanted, seconds):
    end = time.time() + seconds
    while time.time() < end:
        try:
            message = json.loads(lines.get(timeout=max(0.1, end - time.time())))
        except queue.Empty:
            return {}
        if message.get("id") == wanted and "method" not in message:
            return message
    return {}


def initialize(version):
    return {"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
        "protocolVersion": version, "clientCapabilities": ZED_CAPABILITIES,
        "clientInfo": {"name": "zed", "version": "verify"}}}


def write_cache():
    os.makedirs(os.path.dirname(CACHE), exist_ok=True)
    with open(CACHE, "w") as file:
        json.dump({"auth_mode": "apikey", "OPENAI_API_KEY": KEY}, file)
    os.chmod(CACHE, 0o600)


if os.path.exists(CACHE):
    os.remove(CACHE)
process, lines = start()
send(process, initialize(1))
reply = answer(lines, 0, 30)
check("no login: Zed is told what to run",
      "Fix: run 'zevbox auth codex'" in json.dumps(reply), reply)
process.stdin.close()
check("no login: launch exits with 3", process.wait(timeout=30) == 3)

write_cache()
process, lines = start()
send(process, initialize(2))
reply = answer(lines, 0, 30)
check("ACP v2 is refused with a diagnostic", "is not filtered by this zevbox" in json.dumps(reply), reply)
process.stdin.close()
process.wait(timeout=40)

process, lines = start()
send(process, initialize(1))
reply = answer(lines, 0, 60)
result = reply.get("result", {})
check("ACP version 1 negotiated", result.get("protocolVersion") == 1, reply)
check("api-key is masked", all(m.get("id") != "api-key" for m in result.get("authMethods", [])), result)
send(process, {"jsonrpc": "2.0", "id": 1, "method": "authenticate", "params": {"methodId": "api-key"}})
reply = answer(lines, 1, 10)
check("api-key login leads to zevbox auth codex", "zevbox auth codex" in json.dumps(reply), reply)
send(process, {"jsonrpc": "2.0", "id": 2, "method": "session/new",
               "params": {"cwd": PROJECT, "mcpServers": []}})
reply = answer(lines, 2, 40)
result = reply.get("result", {})
check("session opens in the native mode", (result.get("modes") or {}).get("currentModeId") == "agent", reply)
send(process, {"jsonrpc": "2.0", "id": 3, "method": "session/prompt", "params": {
    "sessionId": result.get("sessionId"), "prompt": [{"type": "text", "text": MARKER + " say hi"}]}})
reply = answer(lines, 3, 60)
check("refused account leads to zevbox auth codex", "zevbox auth codex" in json.dumps(reply), reply)
started = time.time()
process.stdin.close()
check("launch exits on stdin EOF", process.wait(timeout=40) == 0)
print("INFO stopped in %.1f s" % (time.time() - started))
time.sleep(3)
listed = subprocess.run(["container", "list", "--all"], capture_output=True, text=True).stdout
check("no Codex VM left", "zevbox-codex-" not in listed, listed)

logs = "".join(open(path, errors="replace").read() for path in glob.glob(ROOT + "/logs/*.log"))
check("journals were written", len(logs) > 0)
check("prompt marker absent from the journals", MARKER not in logs)
check("key absent from the journals", KEY not in logs)
os.remove(CACHE)

if failed:
    sys.exit("verify_launch: FAILED")
print("verify_launch: ALL OK")
