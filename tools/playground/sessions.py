import threading
import time

import config
import sandbox
from session import Session

SESSIONS = {}
LOCK = threading.Lock()


def purge_locked(now):
    expired = [key for key, item in SESSIONS.items() if item.finished and now - item.finished_at > config.SESSION_KEEP_S]
    for key in expired:
        del SESSIONS[key]


def active_locked():
    return [item for item in SESSIONS.values() if not item.finished]


def create(source, client):
    problem = sandbox.validate_source(source)
    if problem:
        return None, 400, problem
    with LOCK:
        purge_locked(time.monotonic())
        active = active_locked()
        if len(active) >= config.MAX_SESSIONS or sum(1 for item in active if item.client == client) >= config.MAX_SESSIONS_PER_CLIENT:
            return None, 503, "busy, try again"
        session = Session(source, client)
        SESSIONS[session.id] = session
    threading.Thread(target=session.run, daemon=True).start()
    return session, 200, None


def get(session_id):
    with LOCK:
        return SESSIONS.get(session_id)
