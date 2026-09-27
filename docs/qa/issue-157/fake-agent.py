#!/usr/bin/env python3
"""Deterministic local ACP peer used only for native attachment delivery QA."""
import base64
import json
import pathlib
import sys

record = pathlib.Path(sys.argv[1])

def send(value):
    print(json.dumps(value), flush=True)

def result(id, value):
    send({"jsonrpc": "2.0", "id": id, "result": value})

for line in sys.stdin:
    message = json.loads(line)
    method, params = message.get("method"), message.get("params", {})
    if method == "initialize":
        result(message["id"], {"protocolVersion": 1, "agentCapabilities": {"promptCapabilities": {"image": True, "audio": True, "embeddedContext": True}}, "authMethods": []})
    elif method == "session/new":
        result(message["id"], {"sessionId": "attachment-native-qa"})
    elif method == "session/prompt":
        blocks = params["prompt"]
        assert blocks[0]["type"] == "text" and "User request:" in blocks[0]["text"]
        counts = {}
        for block in blocks:
            counts[block["type"]] = counts.get(block["type"], 0) + 1
            if block["type"] in ("image", "audio"):
                assert base64.b64decode(block["data"])
        record.write_text(json.dumps({"counts": counts, "params": params}, indent=2))
        send({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "attachment-native-qa", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "References received: native images, audio, embedded files and a video path. Attachment delivery verified."}}}})
        result(message["id"], {"stopReason": "end_turn"})
