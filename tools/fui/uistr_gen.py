#!/usr/bin/env python3
"""One-shot helper that wired the fUi standard texts to lib/fui/uistr.fi.

  python3 tools/fui/uistr_gen.py wire    -- rewrite `fn tr_x() -> str { return "..." }` to call uistr.t
  python3 tools/fui/uistr_gen.py table   -- print the German `de()` body generated from DE below

The German table is the source of truth; `uistr_audit.py` checks that lib/fui/uistr.fi contains it.
"""
import re, sys, os

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
MODS = ["filedlg", "colordlg", "fontdlg", "appctl"]

DE = {
 "home": "Start", "desktop": "Desktop", "documents": "Dokumente", "downloads": "Downloads", "pictures": "Bilder",
 "music": "Musik", "videos": "Videos", "computer": "Computer", "new_folder": "Neuer Ordner", "all_files": "Alle Dateien",
 "col_size": "Größe", "col_modified": "Geändert", "col_name": "Name", "empty_folder": "Dieser Ordner ist leer",
 "msg_folder_gone": "Der Ordner existiert nicht mehr: ", "msg_not_a_folder": "Kein Ordner: ",
 "msg_no_such_folder": "Ordner nicht gefunden: ", "msg_file_not_found": "Datei nicht gefunden: ",
 "msg_replace_warning": "Das Ersetzen einer Datei kann nicht rückgängig gemacht werden",
 "msg_type_a_name": "Namen eingeben oder Datei auswählen",
 "msg_choose_first": "Zuerst eine Datei wählen oder einen Namen eingeben", "msg_broken_link": "Defekter Link: ",
 "msg_type_folder_name": "Namen für den neuen Ordner eingeben", "msg_bad_folder_name": "Das ist kein gültiger Ordnername",
 "msg_folder_created": "Ordner erstellt: ",
 "msg_exists": "Eine Datei oder ein Ordner mit diesem Namen existiert bereits: ",
 "msg_cannot_create_denied": "Zugriff verweigert: Hier kann kein Ordner erstellt werden",
 "msg_cannot_create": "Ordner kann nicht erstellt werden: ", "msg_denied": "Zugriff verweigert: ",
 "msg_no_such_path": "Datei oder Ordner nicht gefunden: ", "msg_no_match": "Kein Ordner beginnt mit: ",
 "msg_folders": "Ordner: ", "back": "Zurück", "forward": "Vorwärts", "up": "Einen Ordner nach oben",
 "refresh": "Aktualisieren", "location": "Ort", "folder": "Ordner", "path": "Pfad",
 "new_folder_label": "Name des neuen Ordners:", "new_folder_name": "Name des neuen Ordners", "create": "Erstellen",
 "cancel": "Abbrechen", "places": "Orte", "place": "Ort", "files": "Dateien", "name_label": "Name:",
 "file_name": "Dateiname", "filter": "Dateityp-Filter", "replace": "Ersetzen", "keep": "Nein, nicht ersetzen",
 "status": "Status", "show_hidden": "Versteckte Dateien anzeigen", "ok": "OK", "items": " Elemente", "item": " Element",
 "selected": " ausgewählt", "find": "  -  Suche: ", "files_chosen": " Dateien ausgewählt",
 "ask_replace": "\" existiert bereits. Ersetzen?",
 # colordlg
 "basic": "Grundfarben", "custom": "Benutzerdefinierte Farben", "add": "Zu benutzerdefinierten Farben hinzufügen",
 "hue": "Farbton", "sat": "Sättigung", "lum": "Helligkeit", "red": "Rot", "green": "Grün", "blue": "Blau", "hex": "Hex",
 "old": "Alt", "new": "Neu", "name_basic": "Grundfarben, ", "name_none": "keine Farbe ausgewählt",
 "name_custom": "Benutzerdefinierte Farben, ", "name_slot": "Platz ", "name_of": " von 16 ausgewählt",
 "name_free": " freie Plätze", "name_hue": "Farbton ", "name_sat": ", Sättigung ", "name_lum": "Helligkeit ",
 "name_old": "Alte Farbe ", "name_new": ", neue Farbe ",
 "name_prev": "Alte und neue Farbe. Auf Alt klicken, um sie wiederherzustellen",
 # fontdlg
 "family": "Schriftart", "style": "Schriftschnitt", "size": "Größe", "sample": "Beispiel", "bold": "Fett",
 "italic": "Kursiv", "underline": "Unterstrichen", "strike": "Durchgestrichen", "filter_hint": "Schriftarten filtern",
 "size_hint": "Schriftgröße", "sample_hint": "Beispieltext", "family_list": "Liste der Schriftarten",
 "style_list": "Liste der Schriftschnitte", "size_list": "Liste der Schriftgrößen", "preview": "Schriftbeispiel",
 "scanning": "Schriftarten werden gesucht", "families": "Schriftfamilien",
 "no_match": "Keine Schriftart passt zum Filter",
 "fallback": "Keine Schriftarten gefunden - Systemschrift wird verwendet",
 "no_preview": "Dieses Schriftformat kann nicht angezeigt werden - Systemschrift wird verwendet",
 "emulated": "In dieser Familie nicht installiert - im Beispiel nachgebildet",
}
# msgdlg (keys written by hand in msgdlg.fi)
DE_EXTRA = {
 "msgdlg.yes": "Ja", "msgdlg.no": "Nein", "msgdlg.cancel": "Abbrechen", "msgdlg.ok": "OK",
 "msgdlg.question": "Frage", "msgdlg.warning": "Warnung", "msgdlg.error": "Fehler", "msgdlg.information": "Information",
}
LIT = r'("(?:[^"\\]|\\.)*")'
PAT = re.compile(r'^fn (tr_\w+)\(\) -> str \{ return ' + LIT + r' \}\s*$')

def esc(s):
    return s.replace("\\", "\\\\").replace('"', '\\"')

def keys():
    out = []
    for m in MODS:
        for line in open(os.path.join(ROOT, "lib/fui", m + ".fi"), encoding="utf-8"):
            r = PAT.match(line)
            if r:
                out.append((m, r.group(1)[3:], r.group(2)))
    return out

def wire():
    for m in MODS:
        p = os.path.join(ROOT, "lib/fui", m + ".fi")
        src = open(p, encoding="utf-8").read().split("\n")
        out = []
        n = 0
        for line in src:
            r = PAT.match(line)
            if r:
                line = 'fn %s() -> str { return uistr.t("%s.%s", %s) }' % (r.group(1), m, r.group(1)[3:], r.group(2))
                n += 1
            out.append(line)
        txt = "\n".join(out)
        if "import fui.uistr" not in txt:
            # after the last top-level import
            idx = [i for i, l in enumerate(out) if l.startswith("import ")]
            out.insert(idx[-1] + 1, "import fui.uistr")
            txt = "\n".join(out)
        open(p, "w", encoding="utf-8").write(txt)
        print(m, n)

def table():
    seen = set()
    for m, name, lit in keys():
        if name not in DE:
            print("MISSING", m, name, file=sys.stderr)
            continue
        k = "%s.%s" % (m, name)
        if k in seen:
            continue
        seen.add(k)
        print('    if same(key, "%s") { return "%s" }' % (k, esc(DE[name])))
    for k, v in DE_EXTRA.items():
        print('    if same(key, "%s") { return "%s" }' % (k, esc(v)))

if __name__ == "__main__":
    {"wire": wire, "table": table}[sys.argv[1]]()
