#!/usr/bin/env python3
"""Exercise the running gateway with synthetic Office requests (uses paid upstream).

Reads the active profile and trusted certificate without changing them. On macOS,
reads only the gateway token from Keychain. Else set OFFICE_GATEWAY_TOKEN.
No document content, upstream credentials, or tokens are recorded.
"""

import argparse
import base64
import concurrent.futures
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import ssl
import struct
import subprocess
import sys
import time
import urllib.error
import urllib.request
import zlib


class Gateway:
    def __init__(self, directory, timeout):
        store = json.loads((directory / "settings.json").read_text())
        self.profile = next(
            p for p in store["providers"] if p["id"] == store["active_provider_id"]
        )
        self.config = self.profile["config"]
        self.timeout = timeout
        self.token = os.environ.get("OFFICE_GATEWAY_TOKEN")
        if not self.token and sys.platform == "darwin":
            result = subprocess.run(
                ["security", "find-generic-password", "-s",
                 "com.miofelix.office-gateway", "-a", "gateway-api-key", "-w"],
                capture_output=True, text=True, check=True,
            )
            self.token = result.stdout.strip()
        if not self.token:
            raise RuntimeError("Set OFFICE_GATEWAY_TOKEN for this platform")
        context = ssl.create_default_context(cafile=str(directory / "TLS/localhost.pem"))
        self.opener = urllib.request.build_opener(
            urllib.request.ProxyHandler({}),
            urllib.request.HTTPSHandler(context=context),
        )
        self.url = f"https://localhost:{self.config['port']}"

    def request(self, method, path, body=None, headers=None, auth=True):
        merged = {"Origin": "https://pivot.claude.ai"}
        if auth:
            merged["x-api-key"] = self.token
        if body is not None:
            merged["Content-Type"] = "application/json"
        merged.update(headers or {})
        request = urllib.request.Request(
            self.url + path,
            data=json.dumps(body, ensure_ascii=False).encode() if body is not None else None,
            headers=merged, method=method,
        )
        try:
            return self.opener.open(request, timeout=self.timeout)
        except urllib.error.HTTPError as error:
            return error

    def message(self, messages, streaming, **extra):
        body = {"model": self.config["public_model"], "max_tokens": 2048,
                "messages": messages, "stream": streaming, **extra}
        started = time.monotonic()
        with self.request("POST", "/v1/messages", body) as response:
            request_id = response.headers.get("x-office-gateway-request-id")
            assert response.status == 200, f"HTTP {response.status}: {response.read(600)!r}"
            if not streaming:
                result = json.load(response)
                assert result["type"] == "message"
                assert result["model"] == self.config["public_model"]
                assert isinstance(result["content"], list)
                return result, {"seconds": round(time.monotonic() - started, 2),
                                "gateway_request_id": request_id}
            assert "text/event-stream" in response.headers.get("Content-Type", "")
            events, blocks, arguments, first = [], {}, {}, None
            terminal = False
            stop_reason = None
            usage = {}
            for raw in response:
                if not raw.startswith(b"data:"):
                    continue
                event = json.loads(raw[5:].strip())
                kind = event["type"]
                assert kind != "error", f"SSE error: {event['error']['message']}"
                assert not terminal, "Event after message_stop"
                events.append(kind)
                if kind == "message_start":
                    assert all(name == "ping" for name in events[:-1]), "message_start arrived after content"
                    assert event["message"]["model"] == self.config["public_model"], "Unexpected model alias"
                elif kind == "content_block_start":
                    index = event["index"]
                    assert index not in blocks
                    blocks[index] = event["content_block"]
                elif kind == "content_block_delta":
                    first = first or round(time.monotonic() - started, 2)
                    index, delta = event["index"], event["delta"]
                    assert index in blocks, "Delta before content_block_start"
                    if delta["type"] == "text_delta":
                        blocks[index]["text"] += delta["text"]
                    elif delta["type"] == "input_json_delta":
                        arguments[index] = arguments.get(index, "") + delta["partial_json"]
                elif kind == "content_block_stop":
                    index = event["index"]
                    if blocks[index]["type"] == "tool_use":
                        blocks[index]["input"] = json.loads(arguments.get(index, "{}"))
                        assert isinstance(blocks[index]["input"], dict)
                elif kind == "message_delta":
                    stop_reason = event["delta"]["stop_reason"]
                    usage = event.get("usage", {})
                elif kind == "message_stop":
                    terminal = True
            assert terminal, "Stream ended without message_stop"
            assert stop_reason, "Missing stop_reason"
            assert type(usage.get("output_tokens")) is int and usage["output_tokens"] >= 0, "Invalid output token count"
            # Native Anthropic-compatible usage can also contain nested cache
            # details and metadata. Validate token counts without rejecting them.
            for name, value in usage.items():
                if name.endswith("_tokens"):
                    assert type(value) is int and value >= 0, f"Invalid token count: {name}"
            return {"content": [blocks[i] for i in sorted(blocks)],
                    "stop_reason": stop_reason, "usage": usage}, {
                        "seconds": round(time.monotonic() - started, 2),
                        "first_delta_seconds": first, "event_count": len(events),
                        "gateway_request_id": request_id,
                    }


def text_of(message):
    return "".join(b["text"] for b in message["content"] if b["type"] == "text").strip()


def routing(gateway):
    revision = None
    for path in ["/health", "/v1/models", "/v1/models/" + gateway.config["public_model"]]:
        with gateway.request("GET", path) as response:
            assert response.status == 200, (path, response.status)
            assert response.headers["Access-Control-Allow-Origin"] == "https://pivot.claude.ai"
            assert response.headers.get("x-office-gateway-request-id", "").startswith("og_"), "Missing gateway correlation id"
            payload = json.load(response)
            if path == "/health":
                revision = payload.get("transport_revision")
                assert revision, "Transport revision missing; an older Gateway is still running"
    with gateway.request("GET", "/v1/models", auth=False) as response:
        assert response.status == 401
    with gateway.request("GET", "/health", headers={"Origin": "https://untrusted.invalid"}) as response:
        assert response.status == 403
    with gateway.request("OPTIONS", "/v1/messages", headers={
        "Access-Control-Request-Method": "POST",
        "Access-Control-Request-Headers": "content-type,x-api-key,anthropic-version,x-stainless-timeout",
    }, auth=False) as response:
        assert response.status == 204
    with gateway.request("POST", "/v1/messages", {
        "model": gateway.config["public_model"], "max_tokens": 0,
        "messages": [{"role": "user", "content": "invalid test"}],
    }) as response:
        assert response.status == 400
    return {"checks": 7, "transport_revision": revision}


def plain(gateway, streaming):
    message, metrics = gateway.message(
        [{"role": "user", "content": "只回复以下文字，不添加标点：中文网关测试成功"}], streaming,
    )
    assert text_of(message) == "中文网关测试成功", "Chinese text was missing, changed, or duplicated"
    assert message["stop_reason"] == "end_turn"
    return metrics


def tool_roundtrip(gateway, streaming, choice="tool"):
    messages = [{"role": "user", "content":
                 "Call read_slide exactly once with slide_index=3. "
                 "After its result, reply with its result marker only."}]
    tool = {"type": "custom", "name": "read_slide", "description": "Read a synthetic slide.",
            "input_schema": {"type": "object", "properties": {
                "slide_index": {"type": "integer"}}, "required": ["slide_index"]}}
    message, first = gateway.message(
        messages, streaming, tools=[tool],
        tool_choice=({"type": "tool", "name": "read_slide", "disable_parallel_tool_use": True}
                     if choice == "tool" else {"type": "any", "disable_parallel_tool_use": True}),
        thinking={"type": "adaptive"}, output_config={"effort": "high"},
    )
    calls = [b for b in message["content"] if b["type"] == "tool_use"]
    assert message["stop_reason"] == "tool_use"
    assert len(calls) == 1 and calls[0]["name"] == "read_slide"
    assert calls[0]["input"] == {"slide_index": 3}
    marker = "TOOL-RESULT-37"
    messages.extend([
        {"role": "assistant", "content": message["content"]},
        {"role": "user", "content": [{"type": "tool_result", "tool_use_id": calls[0]["id"],
                                      "content": json.dumps({"marker": marker, "shape_count": 37})}]},
    ])
    result, second = gateway.message(messages, streaming)
    assert text_of(result) == marker, "Tool result did not survive the round trip"
    return {"call": first, "continuation_without_tools": second}


def history_and_image(gateway):
    marker = "RECOVERED-42"
    pixel = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg=="
    messages = [
        {"role": "user", "content": "Read synthetic slides 1 and 2. After results reply only with the result marker from slide 2. Ignore the test pixel."},
        {"role": "assistant", "content": [
            {"type": "tool_use", "id": "call_live_a", "name": "read_slide", "input": {"slide_index": 1}},
            {"type": "tool_use", "id": "call_live_b", "name": "read_slide", "input": {"slide_index": 2}},
        ]},
        {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "call_live_a", "is_error": True, "content": "Synthetic slide 1 not found. Continue with slide 2."},
            {"type": "tool_result", "tool_use_id": "call_live_b", "content": [
                {"type": "text", "text": "Result marker: " + marker},
                {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": pixel}},
            ]},
        ]},
    ]
    result, metrics = gateway.message(messages, True)
    assert text_of(result) == marker, "Parallel/error/image history did not survive"
    return metrics


def complex_tool_arguments(gateway):
    expected = {"slide_id": "slide-中文-2", "title": "反馈结构与建模方式",
                "rows": [["方法", "机理", "数据"], ["温度", "C·dT/dt", "fθ(x)"]],
                "style": {"font_size": 24, "bold": True}, "label": '引号"与换行\n都要保留'}
    schema = {"type": "object", "properties": {
        "slide_id": {"type": "string"}, "title": {"type": "string"},
        "rows": {"type": "array", "items": {"type": "array", "items": {"type": "string"}}},
        "style": {"type": "object", "properties": {
            "font_size": {"type": "integer"}, "bold": {"type": "boolean"}},
            "required": ["font_size", "bold"]},
        "label": {"type": "string"}}, "required": list(expected)}
    message, metrics = gateway.message([{"role": "user", "content":
        "Call edit_slide exactly once with this exact JSON, preserving all strings and types: " +
        json.dumps(expected, ensure_ascii=False)}], True,
        tools=[{"type": "custom", "name": "edit_slide", "description": "Edit a synthetic slide.", "input_schema": schema}],
        tool_choice={"type": "tool", "name": "edit_slide", "disable_parallel_tool_use": True},
        thinking={"type": "adaptive"}, output_config={"effort": "high"})
    calls = [b for b in message["content"] if b["type"] == "tool_use"]
    assert len(calls) == 1 and calls[0]["input"] == expected, "Nested/Unicode tool arguments changed or slide_id missing"
    assert message["stop_reason"] == "tool_use", "Tool terminal reason lost"
    return metrics


def vision(gateway):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)
    width, height = 160, 160
    colors = [(220, 30, 30), (20, 60, 220), (220, 30, 30), (30, 180, 70)]
    rows = []
    for y in range(height):
        row = bytearray(b"\x00")
        for x in range(width):
            inside = 12 <= x % 80 <= 67 and 12 <= y % 80 <= 67
            row.extend(colors[(y // 80) * 2 + x // 80] if inside else (255, 255, 255))
        rows.append(row)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(b"".join(rows))) + chunk(b"IEND", b"")
    message, metrics = gateway.message([{"role": "user", "content": [
        {"type": "text", "text": "Count the red squares in this image. Reply with the digit only."},
        {"type": "image", "source": {"type": "base64", "media_type": "image/png",
                                     "data": base64.b64encode(png).decode()}},
    ]}], True)
    answer = text_of(message)
    assert answer == "2", f"Visual count incorrect or unsupported: {answer[:150]!r}"
    return metrics


def token_count(gateway):
    body = {"model": gateway.config["public_model"], "messages": [{"role": "user", "content": "中文与English"}]}
    with gateway.request("POST", "/v1/messages/count_tokens", body) as response:
        payload = json.load(response)
        if gateway.config["upstream_protocol"] != "anthropic":
            assert response.status == 501, f"Expected explicit unsupported response, got {response.status}"
            return {"supported": False, "explicit_http_501": True}
        assert response.status == 200, f"Token counting returned HTTP {response.status}: {payload}"
        assert type(payload.get("input_tokens")) is int and payload["input_tokens"] > 0, "Invalid token count"
        return {"supported": True, "input_tokens": payload["input_tokens"]}


def system_instruction(gateway):
    message, metrics = gateway.message([{"role": "user", "content": "请确认"}], False,
        system=[{"type": "text", "text": "Reply with exactly this literal string, preserving every character: SYSTEM-中文-OK"}])
    answer = text_of(message)
    assert answer == "SYSTEM-中文-OK", f"System instruction mismatch: {answer[:150]!r}"
    return metrics


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    default_directory = (Path.home() / "Library/Application Support/Claude Office Gateway"
                         if sys.platform == "darwin" else Path(os.environ.get("APPDATA", ".")) / "Claude Office Gateway")
    parser.add_argument("--data-dir", type=Path, default=default_directory)
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--case", action="append", dest="selected_cases", choices=[
        "routing_auth_cors", "json_chinese", "sse_chinese",
        "json_forced_tool_roundtrip", "sse_forced_tool_roundtrip",
        "parallel_error_and_image_history",
        "complex_unicode_tool_arguments", "vision_count", "token_count", "system_instruction",
        "sse_any_tool_roundtrip",
    ], help="Run only the selected case; repeat to select more than one")
    parser.add_argument("--workers", type=int, choices=range(1, 5), default=1,
                        help="Concurrent cases (default: 1 to avoid provider rate limits)")
    parser.add_argument("--repeat", type=int, choices=range(1, 11), default=1,
                        help="Repeat selected cases (uses paid upstream)")
    parser.add_argument("--interval", type=float, default=5,
                        help="Seconds between repeat rounds (default: 5)")
    parser.add_argument("--case-interval", type=float, default=0,
                        help="Seconds after each case to reduce provider load")
    args = parser.parse_args()
    if not all(0 <= value <= 60 for value in (args.interval, args.case_interval)):
        parser.error("Intervals must be between 0 and 60 seconds")
    gateway = Gateway(args.data_dir, args.timeout)
    print("Profile:", gateway.profile["name"], "Protocol:", gateway.config["upstream_protocol"], flush=True)
    cases = {
        "routing_auth_cors": lambda: routing(gateway),
        "json_chinese": lambda: plain(gateway, False),
        "sse_chinese": lambda: plain(gateway, True),
        "json_forced_tool_roundtrip": lambda: tool_roundtrip(gateway, False),
        "sse_forced_tool_roundtrip": lambda: tool_roundtrip(gateway, True),
        "sse_any_tool_roundtrip": lambda: tool_roundtrip(gateway, True, "any"),
        "parallel_error_and_image_history": lambda: history_and_image(gateway),
        "complex_unicode_tool_arguments": lambda: complex_tool_arguments(gateway),
        "vision_count": lambda: vision(gateway),
        "token_count": lambda: token_count(gateway),
        "system_instruction": lambda: system_instruction(gateway),
    }
    if args.selected_cases:
        cases = {name: cases[name] for name in dict.fromkeys(args.selected_cases)}
    results = []
    def run(name, action):
        started = time.monotonic()
        try:
            detail = action()
            result = {"case": name, "passed": True, "detail": detail}
        except Exception as error:
            result = {"case": name, "passed": False,
                      "error": (str(error) or type(error).__name__).replace(gateway.token, "<REDACTED>")[:700]}
        result["seconds"] = round(time.monotonic() - started, 2)
        print(json.dumps(result, ensure_ascii=False), flush=True)
        if args.case_interval:
            time.sleep(args.case_interval)
        return result
    for sample in range(args.repeat):
        if sample:
            time.sleep(args.interval)
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
            futures = [executor.submit(run, name, action) for name, action in cases.items()]
            for future in futures:
                result = future.result()
                if args.repeat > 1:
                    result["sample"] = sample + 1
                results.append(result)
    report = {"tested_at": datetime.now(timezone.utc).isoformat(),
              "profile": gateway.profile["name"], "protocol": gateway.config["upstream_protocol"],
              "model": gateway.config["upstream_model"], "results": results}
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    return 0 if all(result["passed"] for result in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
