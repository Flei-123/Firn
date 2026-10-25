#!/usr/bin/env python3
"""Deterministic data generator for the UI comparison (fixed seed). Output: data/"""
import json
import os
import random
import datetime as dt

OUT = os.path.join(os.environ.get("UIVGL_DIR", os.path.dirname(os.path.abspath(__file__))), "data")
os.makedirs(OUT, exist_ok=True)
R = random.Random(20261021)
ROWS = int(os.environ.get("GEN_ROWS", "100000"))
BASE = dt.datetime(2026, 10, 21, 12, 0, 0)


def wj(name, obj):
    with open(os.path.join(OUT, name), "w", encoding="utf-8") as f:
        json.dump(obj, f, indent=1, ensure_ascii=False)
        f.write("\n")


# ---- profiles ----
profiles = []
spec = [
    ("Survival Pro", "1.21.4", "fabric", 142, 6144, 0),
    ("Vanilla Classic", "1.20.6", "vanilla", 0, 2048, 1),
    ("Tech Pack", "1.20.1", "forge", 187, 8192, 3),
    ("Speedrun", "1.16.1", "vanilla", 0, 2048, 26),
    ("Cozy Farm", "1.21.1", "quilt", 64, 4096, 5),
    ("Shader Test", "1.21.4", "fabric", 38, 6144, 2),
    ("Skyblock", "1.19.4", "forge", 96, 4096, 48),
    ("Creative Build", "1.21.1", "fabric", 55, 4096, 120),
]
for i, (n, v, l, m, r, h) in enumerate(spec):
    profiles.append({
        "id": i + 1, "name": n, "mc_version": v, "loader": l, "mods": m, "ram_mb": r,
        "last_played": (BASE - dt.timedelta(hours=h)).strftime("%Y-%m-%dT%H:%M:%S"),
    })
wj("profiles.json", {"selected": 1, "profiles": profiles})

# ---- chat ----
contacts = [
    {"id": 1, "name": "Anna Berger", "initials": "AB", "color": "#e57373", "unread": 2},
    {"id": 2, "name": "Max Huber", "initials": "MH", "color": "#64b5f6", "unread": 0},
    {"id": 3, "name": "Lena Gruber", "initials": "LG", "color": "#81c784", "unread": 5},
    {"id": 4, "name": "Tom Fischer", "initials": "TF", "color": "#ffb74d", "unread": 0},
    {"id": 5, "name": "Sophie Wagner", "initials": "SW", "color": "#ba68c8", "unread": 1},
    {"id": 6, "name": "Team Fleitec", "initials": "TE", "color": "#4db6ac", "unread": 0},
]
lines = [
    "Hey, are you there?", "Yep, what's up?", "Did you see the new build?", "Yes, looks good :)",
    "When do we meet?", "Tomorrow at 3 pm?", "Works, see you then!", "Can you send the photo?",
    "Sure, right away", "Thank you!", "Up for Minecraft tonight?", "Always, 8 pm?",
    "I'll bring the server up", "Nice :D", "Did you get the file?", "Yes, everything arrived",
    "Great, let's continue", "Short break, back soon", "Okay", "See you later",
]
messages = []
t = BASE - dt.timedelta(days=2, hours=3)
for i in range(40):
    cid = [1, 1, 1, 1, 3, 3, 2, 2, 4, 5, 6, 1][i % 12] if i < 36 else 1
    t += dt.timedelta(minutes=R.randint(2, 240))
    messages.append({
        "id": i + 1, "contact": cid, "from_me": (i % 3 == 1),
        "text": lines[i % len(lines)], "ts": t.strftime("%Y-%m-%dT%H:%M:%S"),
    })
wj("chat.json", {"selected": 1, "contacts": contacts, "messages": messages})

# ---- files.tsv (100000 rows) + tree.json ----
TYPES = ["txt", "png", "jpg", "pdf", "rs", "fi", "py", "js", "zip", "mp4", "mp3", "json", "md", "log", "bin"]
WORDS = ["report", "image", "backup", "build", "data", "notes", "draft", "final", "archive", "photo",
         "video", "config", "test", "log", "export", "import", "temp", "scan", "doc", "demo"]
DIRS = ["Documents", "Pictures", "Music", "Videos", "Projects", "Downloads", "Backups", "Config"]
dir_counts = {d: 0 for d in DIRS}
rows = []
for i in range(ROWS):
    typ = TYPES[R.randrange(len(TYPES))]
    name = "%s_%s_%05d.%s" % (WORDS[R.randrange(20)], WORDS[R.randrange(20)], i, typ)
    size = int(10 ** R.uniform(2, 8.5))
    d = BASE - dt.timedelta(minutes=R.randint(0, 60 * 24 * 900))
    folder = DIRS[i % len(DIRS)]
    dir_counts[folder] += 1
    rows.append((name, size, d.strftime("%Y-%m-%d %H:%M"), typ, folder))
with open(os.path.join(OUT, "files.tsv"), "w", encoding="utf-8") as f:
    f.write("name\tsize\tdate\ttype\tfolder\n")
    for r in rows:
        f.write("%s\t%d\t%s\t%s\t%s\n" % r)
tree = {"name": "Home", "children": [{"name": d, "files": dir_counts[d], "children": []} for d in DIRS]}
for c in tree["children"]:
    if c["name"] == "Projects":
        c["children"] = [{"name": n, "files": 0, "children": []} for n in ("firn", "fui", "launcher")]
    if c["name"] == "Pictures":
        c["children"] = [{"name": n, "files": 0, "children": []} for n in ("2025", "2026")]
wj("tree.json", tree)

# ---- notes ----
notes_md = """# Project Notes

Welcome to the **fUi** comparison notes. This file tests *Markdown* rendering.

## Todo

- Finish the **harness**
- Measure *startup* time
- Compare screenshots
- Write the report

## Code

```
fn main() -> i32 {
    return 0
}
```

Use `measure.py` to run the benchmarks. See [the docs](https://example.com/docs) for details.

## Numbers

1. first item
2. second item
3. third item

Plain closing paragraph with a **bold** word and a [link](https://example.com).
"""
with open(os.path.join(OUT, "notes.md"), "w", encoding="utf-8") as f:
    f.write(notes_md)
notes_list = [
    {"id": 1, "title": "Project Notes", "file": "notes.md", "modified": "2026-10-21T09:30:00"},
    {"id": 2, "title": "Shopping list", "file": "", "modified": "2026-10-20T18:05:00"},
    {"id": 3, "title": "Meeting 19.10.", "file": "", "modified": "2026-10-19T11:00:00"},
    {"id": 4, "title": "Ideas", "file": "", "modified": "2026-10-15T22:40:00"},
    {"id": 5, "title": "Server setup", "file": "", "modified": "2026-10-12T08:15:00"},
]
wj("notes_list.json", {"selected": 1, "notes": notes_list})

# ---- metrics ----
days = []
v = 1000.0
for i in range(90):
    v = max(200.0, v + R.uniform(-60, 75))
    days.append({"date": (BASE - dt.timedelta(days=89 - i)).strftime("%Y-%m-%d"), "value": round(v, 1)})
metrics = {
    "kpis": [
        {"name": "Revenue", "value": 48210, "unit": "EUR", "delta_pct": 12.4},
        {"name": "Users", "value": 8421, "unit": "", "delta_pct": 5.1},
        {"name": "Orders", "value": 1297, "unit": "", "delta_pct": -2.3},
        {"name": "Conversion", "value": 3.8, "unit": "%", "delta_pct": 0.6},
    ],
    "line_90d": days,
    "bars_12": [{"label": m, "value": R.randint(20, 100)} for m in
                ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]],
    "donut": [
        {"label": "Direct", "share": 38}, {"label": "Search", "share": 27},
        {"label": "Social", "share": 20}, {"label": "Referral", "share": 15},
    ],
    "events": [
        {"ts": (BASE - dt.timedelta(minutes=5 + 37 * i)).strftime("%Y-%m-%d %H:%M"), "type": k, "text": s}
        for i, (k, s) in enumerate([
            ("order", "Order #1297 paid"), ("signup", "New user anna@example.com"),
            ("error", "Payment gateway timeout"), ("order", "Order #1296 shipped"),
            ("deploy", "Release 2.4.1 deployed"), ("signup", "New user max@example.com"),
            ("order", "Order #1295 paid"), ("alert", "CPU above 85 percent"),
        ])
    ],
    "ranges": ["7d", "30d", "90d"],
    "default_range": "90d",
}
wj("metrics.json", metrics)

# ---- settings defaults + form test data ----
wj("settings.json", {
    "pages": ["General", "Appearance", "Account", "Network", "Advanced"],
    "general": {"language": "English", "autostart": True, "notifications": True, "auto_update": False,
                "languages": ["English", "Deutsch", "Francais", "Espanol"]},
    "appearance": {"theme": "dark", "accent": "#3ddc84", "font_size": 14, "animations": True, "scale_pct": 100},
    "account": {"username": "justin", "email": "justin@example.com", "display_name": "Justin"},
    "network": {"proxy_enabled": False, "proxy_host": "127.0.0.1", "proxy_port": "8080", "mode": "auto",
                "modes": ["auto", "manual", "offline"]},
    "advanced": {"debug_log": False, "cache_mb": 512, "telemetry": False},
})
wj("form.json", {
    "steps": ["Person", "Address", "Payment", "Summary"],
    "valid": {
        "first_name": "Justin", "last_name": "Muster", "email": "justin@example.com",
        "street": "Hauptstrasse 1", "zip": "6020", "city": "Innsbruck",
        "card_holder": "Justin Muster", "card_number": "4111111111111111", "card_exp": "12/28",
    },
    "invalid": {"first_name": "", "email": "no-at-sign", "zip": "12"},
    "rules": {"required": ["first_name", "last_name", "email", "street", "zip", "city"],
              "email": "must contain @ and a dot after it", "zip": "exactly 4 or 5 digits"},
})

with open(os.path.join(OUT, "README.md"), "w", encoding="utf-8") as f:
    f.write("""# Shared data (generated by ../gen_data.py, fixed seed 20261021, deterministic)

All apps read these files instead of embedding data. Regenerate: `python3 gen_data.py`.

| File | Format |
|---|---|
| profiles.json | `{selected, profiles:[{id,name,mc_version,loader(fabric/forge/quilt/vanilla),mods,ram_mb,last_played ISO}]}` 8 profiles |
| chat.json | `{selected, contacts:[{id,name,initials,color,unread}] x6, messages:[{id,contact,from_me,text,ts ISO}] x40}` |
| files.tsv | header + 100000 rows, tab separated: `name size(bytes) date(YYYY-MM-DD HH:MM) type folder` |
| tree.json | `{name, children:[{name, files, children}]}` folder tree for files app (8 top folders) |
| notes.md | Markdown: headings, bold, italic, lists, code block, links |
| notes_list.json | `{selected, notes:[{id,title,file,modified}] x5}` (note 1 = notes.md) |
| metrics.json | `{kpis x4, line_90d x90 {date,value}, bars_12 {label,value}, donut x4 {label,share}, events x8, ranges, default_range}` |
| settings.json | pages + defaults for the 5 settings pages |
| form.json | wizard steps, `valid` and `invalid` test input, validation rules |

Default screenshot state: first item selected everywhere, no hover, 1100x700, DPR 1.
""")
print("ok", os.listdir(OUT))
