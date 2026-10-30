import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

from tpl_ref import icon, box, text, page
from tpl_ref_store import nav

PLACES = [("home", "Home"), ("document", "Documents"), ("image", "Pictures"), ("music", "Music")]
DIRS = [("Documents", "2026-09-14"), ("Music", "2026-10-21"), ("Pictures", "2026-08-30")]
FILES = [("backup.zip", "folder_zip", 8388608, "2026-03-12"), ("budget.xlsx", "document", 184320, "2026-09-14"),
         ("clip.mp4", "video", 31457280, "2026-05-09"), ("invoice.pdf", "document_pdf", 96000, "2026-04-22"),
         ("logo.png", "image", 640000, "2026-07-04"), ("main.fi", "code", 4300, "2026-10-28"),
         ("notes.txt", "document_text", 2048, "2026-10-02"), ("plan.docx", "document", 52000, "2026-02-01"),
         ("readme.md", "document_text", 1200, "2026-10-29"), ("song.mp3", "music", 5242880, "2026-06-18"),
         ("summary.pdf", "document_pdf", 1572864, "2026-08-30"), ("todo.md", "document_text", 912, "2026-10-21")]


def size_text(v):
    if v < 1024:
        return "%d B" % v
    if v < 1048576:
        return "%d KB" % (v // 1024)
    return "%d MB" % (v // 1048576)


def toolbar(c, grid):
    o = ""
    for k, name in enumerate(("back", "forward", "up")):
        o += icon(name, 272 + k * 36, 24, 16, c["disabled"])
    o += text(388, 16, 32, 14, 600, c["text"], "Home")
    o += icon("list" if grid else "grid", 1012, 24, 16, c["text"]) + icon("sort", 1048, 24, 16, c["text"])
    return o


def status(c, h):
    return box(240, h - 28, 860, 28, c["surface"]) + box(240, h - 28, 860, 1, c["sep"]) + text(264, h - 28, 28, 12, 400, c["muted"], "15 items")


def files_list(c, w, h):
    o = nav(c, h, 0, False, PLACES, False) + toolbar(c, False)
    o += text(288, 60, 32, 12, 600, c["text"], "Name") + icon("chevron_up", 330, 70, 12, c["text"])
    o += text(800, 60, 32, 12, 400, c["muted"], "Modified") + text(1004, 60, 32, 12, 400, c["muted"], "Size")
    o += box(240, 91, 860, 1, c["sep"])
    y = 92
    for name, when in DIRS:
        o += icon("folder", 280, y + 8, 20, c["accent"]) + text(312, y + 1, 36, 14, 400, c["text"], name) + text(800, y + 1, 36, 14, 400, c["muted"], when)
        y += 36
    for name, ico, size, when in FILES:
        o += icon(ico, 280, y + 8, 20, c["muted"]) + text(312, y + 1, 36, 14, 400, c["text"], name) + text(800, y + 1, 36, 14, 400, c["muted"], when)
        o += text(1000, y + 1, 36, 14, 400, c["muted"], size_text(size), 52, "right")
        y += 36
    o += status(c, h)
    return page(o, w, h, c)


def files_grid(c, w, h):
    o = nav(c, h, 0, False, PLACES, False) + toolbar(c, True)
    cw = (812 - 40) / 6
    items = [(n, "folder", c["accent"]) for n, _ in DIRS] + [(n, i, c["muted"]) for n, i, _, _ in FILES]
    for k, (name, ico, col) in enumerate(items):
        r, q = divmod(k, 6)
        x = 264 + q * (cw + 8)
        y = 60 + r * 112
        o += icon(ico, x + cw / 2 - 20, y + 12, 40, col)
        o += text(x, y + 68, 24, 13, 400, c["text"], name, cw, "center")
    o += status(c, h)
    return page(o, w, h, c)


TARGETS = {"files": files_list, "files_grid": files_grid}
