"""Run bounded, read-only REA Android inspections in an isolated MCP process.

Requires explicit REA_JADX_MCP_JAR and JAVA_HOME. This script never executes APKs
or communicates with a device. Evidence destinations must remain private.
"""

import argparse
import json
import os
import queue
import subprocess
import sys
import threading
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--actions", default="[]", help="JSON list of MCP tool calls")
    parser.add_argument("--interactive", action="store_true", help="Read further JSON call lists from stdin")
    parser.add_argument("--apk", help="Default APK path for Android inspection calls")
    parser.add_argument("--export", help="Private evidence JSON destination")
    args = parser.parse_args()
    env = os.environ.copy()
    if not env.get("REA_JADX_MCP_JAR") or not env.get("JAVA_HOME"):
        parser.error("Set REA_JADX_MCP_JAR and JAVA_HOME explicitly")
    process = subprocess.Popen(
        ["rea.cmd", "--mcp"], env=env, stdin=subprocess.PIPE,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )
    output = queue.Queue()

    def collect(stream, destination):
        for line in stream:
            destination.put(line)

    threading.Thread(target=collect, args=(process.stdout, output), daemon=True).start()
    errors = queue.Queue()
    threading.Thread(target=collect, args=(process.stderr, errors), daemon=True).start()
    sequence = 0

    def rpc(method, params=None, notification=False):
        nonlocal sequence
        sequence += 1
        packet = {"jsonrpc": "2.0", "method": method, "params": params or {}}
        if not notification:
            packet["id"] = sequence
        process.stdin.write(json.dumps(packet) + "\n")
        process.stdin.flush()
        if notification:
            return None
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            try:
                line = output.get(timeout=1)
            except queue.Empty:
                if process.poll() is not None:
                    raise RuntimeError("REA process exited")
                continue
            try:
                result = json.loads(line)
            except json.JSONDecodeError:
                continue
            if result.get("id") == sequence:
                if "error" in result:
                    raise RuntimeError(json.dumps(result["error"]))
                return result.get("result")
        raise TimeoutError(method)

    def tool(name, arguments):
        return rpc("tools/call", {"name": name, "arguments": arguments})

    try:
        rpc("initialize", {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "sound-connect-research", "version": "1"}})
        rpc("notifications/initialized", notification=True)
        def perform(action):
            if args.apk and "android" in action["name"]:
                action.setdefault("arguments", {}).setdefault("path", os.path.abspath(args.apk))
            result = tool(action["name"], action.get("arguments", {}))
            evidence = result.get("structuredContent")
            if evidence is None:
                for content in result.get("content", []):
                    if content.get("type") == "text":
                        try:
                            evidence = json.loads(content["text"])
                            break
                        except json.JSONDecodeError:
                            pass
            evidence = evidence or result
            normalized = evidence.get("normalized_result", evidence)
            source = normalized.get("source", {})
            if len(source.get("text", "")) > 5000:
                source = dict(source)
                source["text"] = source["text"][:5000]
                source["console_preview_truncated"] = True
                normalized = dict(normalized, source=source)
            print(json.dumps({"tool": action["name"], "evidence_id": evidence.get("evidence_id"), "result": normalized}), flush=True)
        for action in json.loads(args.actions):
            perform(action)
        if args.interactive:
            print("REA_READY", flush=True)
            for line in sys.stdin:
                if not line.strip():
                    continue
                if line.strip() == "quit":
                    break
                for action in json.loads(line):
                    perform(action)
        if args.export:
            print(json.dumps(tool("export_evidence_bundle", {"path": os.path.abspath(args.export), "overwrite": True})), flush=True)
    finally:
        try:
            tool("close_binary", {})
        except Exception:
            pass
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()


if __name__ == "__main__":
    main()
