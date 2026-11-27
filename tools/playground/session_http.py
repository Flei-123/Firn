import json
import re
import select
import socket
import time
from urllib.parse import parse_qs, urlsplit

import config
import sessions
from session import TERMINAL_EVENTS

CREATE_PATH = "/api/session"
ROUTE = re.compile(r"^/api/session/([A-Za-z0-9_-]{16,32})/(events|stdin|stop)$")
WAIT_S = 0.5


def read_json(handler, limit):
    length = int(handler.headers.get("Content-Length") or 0)
    if length > limit:
        handler.send_json(413, {"error": "request too large"})
        return None
    try:
        body = json.loads(handler.rfile.read(length) or b"{}")
    except ValueError:
        body = None
    if not isinstance(body, dict):
        handler.send_json(400, {"error": "invalid json"})
        return None
    return body


def client_of(handler):
    return handler.headers.get("X-Real-IP") or handler.client_address[0]


def create(handler):
    body = read_json(handler, config.MAX_SOURCE_BYTES * 2)
    if body is None:
        return
    session, status, problem = sessions.create(body.get("source"), client_of(handler))
    if session is None:
        return handler.send_json(status, {"error": problem})
    handler.send_json(200, {"id": session.id})


def send_input(handler, session):
    body = read_json(handler, config.MAX_STDIN_REQUEST_BYTES)
    if body is None:
        return
    text = body.get("data", "")
    if not isinstance(text, str):
        return handler.send_json(400, {"error": "data must be text"})
    result = session.send_input(text, body.get("eof") is True)
    status = {"ok": 200, "limit": 413}.get(result, 409)
    handler.send_json(status, {"ok": result == "ok", "state": result})


def stop(handler, session):
    session.terminate("stopped")
    handler.send_json(200, {"ok": True})


def client_gone(handler):
    readable, _, _ = select.select([handler.connection], [], [], 0)
    if not readable:
        return False
    try:
        return handler.connection.recv(1, socket.MSG_PEEK) == b""
    except OSError:
        return True


def first_index(handler, query):
    raw = handler.headers.get("Last-Event-ID") or (query.get("last") or ["0"])[0]
    return int(raw) if raw.isdigit() else 0


def write_event(handler, index, name, data):
    handler.wfile.write(f"id: {index + 1}\nevent: {name}\ndata: {data}\n\n".encode("utf-8"))


def stream(handler, session, query):
    handler.send_response(200)
    for key, value in (("Content-Type", "text/event-stream; charset=utf-8"), ("Cache-Control", "no-store"), ("X-Accel-Buffering", "no"), ("X-Content-Type-Options", "nosniff")):
        handler.send_header(key, value)
    handler.end_headers()
    handler.close_connection = True
    index = first_index(handler, query)
    last_write = time.monotonic()
    session.attach()
    try:
        while not client_gone(handler):
            batch = session.wait_events(index, WAIT_S)
            for name, data in batch:
                write_event(handler, index, name, data)
                index += 1
            if batch and batch[-1][0] in TERMINAL_EVENTS:
                return
            now = time.monotonic()
            if batch:
                last_write = now
            elif now - last_write >= config.SESSION_HEARTBEAT_S:
                handler.wfile.write(b": ping\n\n")
                last_write = now
    except OSError:
        pass
    finally:
        session.detach()


def dispatch(handler, method):
    parts = urlsplit(handler.path)
    if method == "POST" and parts.path == CREATE_PATH:
        return create(handler)
    match = ROUTE.match(parts.path)
    session = sessions.get(match.group(1)) if match else None
    if session is None:
        return handler.send_json(404, {"error": "not found"})
    action = match.group(2)
    if method == "GET" and action == "events":
        return stream(handler, session, parse_qs(parts.query))
    if method == "POST" and action == "stdin":
        return send_input(handler, session)
    if method == "POST" and action == "stop":
        return stop(handler, session)
    handler.send_json(405, {"error": "method not allowed"})
