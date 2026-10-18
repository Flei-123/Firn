#!/usr/bin/env python3
"""Mock server for the mobile Jarvis web app (reference screenshots).

Serves /root/jarvis/public statically (read-only) and answers /api/* with
hard-coded MOCK data. It never talks to the real service, never reads real
user data and needs no login. WebSocket upgrades are refused (the app
shows its 'disconnected' pill, the connection bar is hidden by capture.mjs).

usage: mockserver.py [--port 8765] [--public /root/jarvis/public]
"""
import argparse, json, mimetypes, os, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

NOW = int(time.time() * 1000)
MIN = 60 * 1000

CODE = ("Klar. So baust du das in Rust:\n\n```rust\nfn main() {\n    let xs: Vec<u32> = (1..=5).collect();\n"
        "    let sum: u32 = xs.iter().sum();\n    println!(\"sum = {sum}\");\n}\n```\n\n"
        "- **Ergebnis:** `sum = 15`\n- Läuft ohne Abhängigkeiten.\n")
MSGS = [
    {"role": "user", "text": "Hey, kannst du mir kurz zeigen, wie man in Rust eine Liste aufsummiert?", "at": NOW - 9 * MIN},
    {"role": "assistant", "text": CODE, "at": NOW - 8 * MIN, "genMs": 4200},
    {"role": "user", "text": "Danke! Und wie sieht es mit dem Server aus? Läuft alles?", "at": NOW - 4 * MIN},
    {"role": "assistant", "text": "Alles grün.\n\n- **Load:** 0.8 / 8 Kerne\n- **RAM:** 41 %\n- **Disk:** 63 %\n\nNichts zu tun.", "at": NOW - 3 * MIN, "genMs": 2100},
]
CONVS = [
    {"id": "c-orch", "title": "Leitstand", "kind": "orchestrator", "updatedAt": NOW - 3 * MIN, "jarvisMode": True},
    {"id": "c1", "title": "Rust Liste aufsummieren", "kind": "chat", "updatedAt": NOW - 3 * MIN, "jarvisMode": True},
    {"id": "c2", "title": "Android-App: Layout Probleme", "kind": "chat", "updatedAt": NOW - 90 * MIN, "jarvisMode": True},
    {"id": "c3", "title": "Server aufräumen", "kind": "chat", "updatedAt": NOW - 26 * 60 * MIN, "jarvisMode": True},
    {"id": "c4", "title": "Russisch Vokabeln", "kind": "chat", "updatedAt": NOW - 3 * 24 * 60 * MIN, "jarvisMode": False},
]
ME = {"username": "justin", "admin": True, "imp": None, "impName": None, "deviceToken": "mock", "exeAvailable": False,
      "apkAvailable": True, "settings": {"theme": "fleitec", "ttsRate": 1.05, "ttsEngine": "server", "autoConfirm": True,
      "deepMode": True, "expMode": False, "learnMode": False}}
TASKS = [
    {"id": "t1", "text": "Abgabe Diplomarbeit Kapitel 3 prüfen", "category": "Schule", "due": "2026-10-18T09:00:00+02:00", "repeat": "none", "done": False},
    {"id": "t2", "text": "Firn Volllauf bei Load unter 16 starten", "category": "Projekt", "due": "2026-10-19T08:00:00+02:00", "repeat": "none", "done": False},
    {"id": "t3", "text": "Milch und Brot kaufen", "category": "Einkauf", "due": None, "repeat": "none", "done": False},
    {"id": "t4", "text": "Wöchentliches Backup prüfen", "category": "Privat", "due": "2026-10-21T18:00:00+02:00", "repeat": "weekly", "done": False},
    {"id": "t5", "text": "Zahnarzttermin ausmachen", "category": "Privat", "due": None, "repeat": "none", "done": True},
]
def proj(pid, name, done, total, needs, running, nxt, status="active", metrics=None):
    nl = [{"id": f"n{pid}{i}", "text": t, "options": ["Ja", "Nein"]} for i, t in enumerate(needs)]
    return {"id": pid, "name": name, "status": status, "pct": round(100 * done / total), "done": done, "total": total,
            "open": total - done, "needs": len(needs), "needsList": nl, "running": [{"title": r} for r in running],
            "next": nxt, "nextOpen": [], "last": {"at": "2026-10-18T17:30:00+02:00"}, "metrics": metrics or [], "design": None}
PROJECTS = [
    proj("p1", "Firn", 410, 640, ["Soll der Store-Eintrag veröffentlicht werden?"], ["Worker: fmt2"], "Volllauf bei Load unter 16",
         metrics=[{"key": "Tempo vs Rust", "value": 2.8, "unit": "x", "target": "<= 2", "history": [{"value": 3.1}]},
                  {"key": "Tests", "value": 98, "unit": "%", "history": []}]),
    proj("p2", "fUi", 167, 391, ["Auf dem Handy testen: Textskala 130 %?"], [], "Textskala-Rest r391"),
    proj("p3", "Certus", 331, 574, [], ["Worker: heise-Spin"], "heise.de Layout-Hänger"),
    proj("p4", "OrientOS", 120, 245, [], [], "wlib durch fUi ersetzen"),
    proj("p5", "Daidalos", 90, 90, [], [], "", status="done"),
]
CC = {"sys": {"host": "jarvis-srv", "cpus": 8, "load": [0.8, 1.1, 1.4], "memTotal": 33.5e9, "memFree": 19.7e9,
              "disk": {"total": 500e9, "free": 185e9}, "uptime": 41 * 86400 + 5 * 3600},
      "rate": {"fiveHour": {"util": 38}, "weekly": {"util": 61}},
      "tasks": [{"text": t["text"], "category": t["category"], "due": t["due"], "overdue": False, "repeat": None}
                for i, t in enumerate(TASKS[:4])],
      "devices": ["Dell", "HOLO-LAPTOP-3"],
      "deviceInfo": [{"name": "Dell", "online": True, "type": "Windows"}, {"name": "HOLO-LAPTOP-3", "online": True, "type": "Windows"},
                     {"name": "OrientOS-VM", "online": False, "type": "OrientOS"}],
      "memory": {"count": 3278, "user_profile": {"Name": "Justin"}, "knowledge": [{"at": "2026-10-18T17:50:00+02:00", "text": "Firn r699: f-String-Specs komplett."}]},
      "activity": [{"at": "2026-10-18T18:05:00+02:00", "role": "worker", "text": "fmt2 fertig"}]}

def conv_full(cid):
    c = next((x for x in CONVS if x["id"] == cid), CONVS[1])
    return dict(c, messages=MSGS, total=len(MSGS), from_=0, **{"from": 0}, head=None)

def api(path, q, method, body):
    if path in ("/api/boot",):
        cid = (q.get("c") or ["c1"])[0]
        if cid not in [c["id"] for c in CONVS]: cid = "c1"
        return {"me": ME, "conversations": CONVS, "cid": cid, "conversation": conv_full(cid), "convTotal": len(CONVS), "convRev": "mock", "folders": []}
    if path == "/api/me": return ME
    if path == "/api/conversations": return {"conversations": CONVS, "total": len(CONVS), "rev": "mock", "folders": []}
    if path.startswith("/api/conversations/folders"): return {"folders": []}
    if path.startswith("/api/conversations/"): return {"conversation": conv_full(path.rsplit("/", 1)[-1])}
    if path == "/api/projects": return {"projects": PROJECTS, "stale": [], "linked": {}}
    if path.startswith("/api/projects/"):
        pid = path.split("/")[3]
        return {"project": next((p for p in PROJECTS if p["id"] == pid), PROJECTS[0])}
    if path == "/api/tasks": return {"tasks": TASKS}
    if path == "/api/command-center": return CC
    if path == "/api/settings": return {"settings": ME["settings"]}
    if path.startswith("/api/usage"): return {"fiveHour": {"util": 38}, "weekly": {"util": 61}, "accounts": []}
    if path == "/api/models": return {"models": [], "current": "claude-sonnet-5-5"}
    if path.startswith("/api/staff/tree"): return {"tree": []}
    if path.startswith("/api/theme"):
        return {"theme": {"preset": "fleitec"}, "own": True, "apps": [{"id": "jarvis", "label": "JARVIS"}], "overrides": [],
                "presets": [{"id": "fleitec", "label": "FleiTec Navy", "group": "JARVIS", "bg": "#0c0e1f", "panel": "#15172b", "acc": "#2f86ff", "txt": "#e9ebf5"},
                            {"id": "claude", "label": "Claude", "group": "JARVIS", "bg": "#262624", "panel": "#30302e", "acc": "#c96442", "txt": "#ecebe4"}],
                "accents": [{"id": "#2f86ff", "label": "Blau"}, {"id": "#1bd96a", "label": "Gruen"}], "fonts": []}
    if path == "/api/voices": return {"voices": []}
    return {}

class H(BaseHTTPRequestHandler):
    public = "/root/jarvis/public"
    protocol_version = "HTTP/1.1"
    def log_message(self, *a): pass
    def _send(self, code, data, ctype):
        self.send_response(code); self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data))); self.send_header("Cache-Control", "no-store")
        self.end_headers(); self.wfile.write(data)
    def _handle(self, method):
        u = urlparse(self.path); p = u.path
        n = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(n) if n else b""
        if p.startswith("/api/"):
            if method == "POST": print("mock POST", p, flush=True)
            return self._send(200, json.dumps(api(p, parse_qs(u.query), method, body)).encode(), "application/json; charset=utf-8")
        if p == "/theme.js": return self._send(200, b"window.FleiTheme=null", "application/javascript")
        if p in ("/", "/index.html"): p = "/index.html"
        if p == "/login": p = "/login.html"
        fp = os.path.normpath(os.path.join(self.public, p.lstrip("/")))
        if not fp.startswith(self.public + os.sep) or not os.path.isfile(fp):
            return self._send(404, b"not found", "text/plain")
        ct = mimetypes.guess_type(fp)[0] or "application/octet-stream"
        if ct.startswith("text/") or ct.endswith("javascript"): ct += "; charset=utf-8"
        with open(fp, "rb") as f: data = f.read()
        if p == "/app.js":
            # production bug (tasks modal): inside renderTasks `t` is the task object and shadows the i18n
            # function t(), so t('gen.delete') throws and the whole task list stays empty. Patched here only
            # (in memory) so the reference screenshot shows the intended list.
            data = data.replace(b"del.title = t('gen.delete')", b"del.title = 'Loeschen'")
        self._send(200, data, ct)
    def do_GET(self): self._handle("GET")
    def do_POST(self): self._handle("POST")
    def do_PUT(self): self._handle("PUT")
    def do_PATCH(self): self._handle("PATCH")

if __name__ == "__main__":
    ap = argparse.ArgumentParser(); ap.add_argument("--port", type=int, default=8765)
    ap.add_argument("--public", default="/root/jarvis/public"); a = ap.parse_args()
    H.public = os.path.realpath(a.public)
    ThreadingHTTPServer(("127.0.0.1", a.port), H).serve_forever()
