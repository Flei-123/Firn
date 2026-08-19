#!/usr/bin/env python3
"""Einbinder fuer die Firn-Bibliothek des Moduls `str`.

Stufe 0 hat (noch) kein Modulsystem: `firnc` uebersetzt genau EINE Datei.
Damit `lib/str/*.fi` und `lib/num/*.fi` trotzdem nur einmal existieren und
nicht in jede Testdatei kopiert werden muessen, loest dieses Werkzeug Zeilen
der Form

    //#include lib/str/str16.fi

rekursiv auf (jede Datei hoechstens einmal) und schreibt das Ergebnis als
eigenstaendiges .fi-Programm. Die erzeugten Dateien in tests/ sind Teil des
Baums, damit `test.sh` sie ohne Zusatzwerkzeug uebersetzen kann.

Aufruf:  tools/strlib/expand.py <quelle.fi> <ziel.fi>
         tools/strlib/expand.py --all      (alle Testquellen neu erzeugen)
         tools/strlib/expand.py --check    (erzeugte Dateien sind aktuell?)
"""

import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# source (in tools/strlib/src) -> generated file (in tests/ resp. tools/)
TARGETS = [
    ("tools/strlib/src/300_str16_surrogate.fi", "tests/300_str16_surrogate.fi"),
    ("tools/strlib/src/301_bytes_utf8.fi", "tests/301_bytes_utf8.fi"),
    ("tools/strlib/src/302_atom_intern.fi", "tests/302_atom_intern.fi"),
    ("tools/strlib/src/303_wtf8_roundtrip.fi", "tests/303_wtf8_roundtrip.fi"),
    ("tools/strlib/src/304_strtod_hardcases.fi", "tests/304_strtod_hardcases.fi"),
    ("tools/strlib/src/305_dtoa_hardcases.fi", "tests/305_dtoa_hardcases.fi"),
    ("tools/strlib/src/306_dtoa_roundtrip_small.fi", "tests/306_dtoa_roundtrip_small.fi"),
    ("tools/strlib/src/307_bignum.fi", "tests/307_bignum.fi"),
    ("tools/strlib/src/308_str16_api.fi", "tests/308_str16_api.fi"),
    ("tools/strlib/src/dtoa_stream.fi", "tools/dtoa_vectors/dtoa_stream.fi"),
    # std facade (round 39): lib/str and lib/num are include
    # libraries; the facade is put together textually as ONE module.
    ("tools/strlib/src/std_str.fi", "lib/std/str.fi"),
    ("tools/strlib/src/std_num.fi", "lib/std/num.fi"),
    ("tools/strlib/src/neg/str_bytes_is_no_text.fi", "tests/neg/str_bytes_is_no_text.fi"),
    ("tools/strlib/src/neg/str16_is_no_bytes.fi", "tests/neg/str16_is_no_bytes.fi"),
]


def expand(path, seen, out, stack):
    real = os.path.normpath(os.path.join(ROOT, path))
    if real in seen:
        return
    seen.add(real)
    if not os.path.exists(real):
        raise SystemExit("expand.py: '%s' gibt es nicht (eingebunden aus %s)"
                         % (path, stack[-1] if stack else "<oben>"))
    with open(real, encoding="utf-8") as f:
        lines = f.read().split("\n")
    stack.append(path)
    for line in lines:
        s = line.strip()
        if s.startswith("//#include"):
            inc = s[len("//#include"):].strip()
            expand(inc, seen, out, stack)
        elif s.startswith("//#str "):
            # //#str name text   ->   fn text_name(b: *mut Bytes) { ... }
            rest = s[len("//#str "):]
            name, _, text = rest.partition(" ")
            out.extend(gen_text_fn(name, text))
        else:
            out.append(line)
    stack.pop()


def gen_text_fn(name, text):
    """Erzeugt eine Firn-Funktion, die `text` als ASCII in ein Bytes legt.

    Stufe 0 kennt noch keine Zeichenkettenliterale (die Anbindung von
    compiler/src/strings.rs an den Lexer gehoert dem Modul kern); bis dahin
    baut dieses Werkzeug die Oktettfolge auf.
    """
    lines = ["// Text: %s" % text, "fn text_%s(b: *mut Bytes) {" % name,
             "    bytes_clear(b)"]
    for ch in text.encode("utf-8"):
        lines.append("    bytes_push(b, %d as u8)" % ch)
    lines.append("}")
    return lines


def fix_error_line(lines):
    """`// expect_error: ?:SPALTE text` bekommt die Zeilennummer der mit
    `// FEHLERZEILE` markierten Zeile — die Bibliothek waechst, die Erwartung
    bleibt richtig."""
    mark = None
    for i, l in enumerate(lines):
        if l.rstrip().endswith("// FEHLERZEILE"):
            mark = i + 1
            break
    if mark is None:
        return lines
    for i, l in enumerate(lines):
        if l.startswith("// expect_error: ?:"):
            lines[i] = l.replace("// expect_error: ?:", "// expect_error: %d:" % mark, 1)
            break
    return lines


def build(src, dst):
    out = []
    expand(src, set(), out, [])
    text = "\n".join(out)
    # merge double empty lines so that the generated file stays readable
    while "\n\n\n" in text:
        text = text.replace("\n\n\n", "\n\n")
    if not text.endswith("\n"):
        text += "\n"
    header = "// ERZEUGT von tools/strlib/expand.py aus %s — nicht von Hand aendern.\n" % src
    # The expectation line (// expect_*) has to stay line 1 (test.sh reads it).
    lines = text.split("\n")
    if lines and lines[0].startswith("// expect"):
        lines = [lines[0], header.rstrip("\n")] + lines[1:]
    else:
        lines = [header.rstrip("\n")] + lines
    # Only now are the line numbers finally settled.
    lines = fix_error_line(lines)
    return "\n".join(lines)


def main(argv):
    if len(argv) == 2 and argv[1] in ("--all", "--check"):
        bad = 0
        for src, dst in TARGETS:
            text = build(src, dst)
            full = os.path.join(ROOT, dst)
            old = None
            if os.path.exists(full):
                with open(full, encoding="utf-8") as f:
                    old = f.read()
            if argv[1] == "--check":
                if old != text:
                    print("veraltet: %s" % dst)
                    bad += 1
            else:
                if old != text:
                    os.makedirs(os.path.dirname(full), exist_ok=True)
                    with open(full, "w", encoding="utf-8") as f:
                        f.write(text)
                    print("erzeugt:  %s" % dst)
        if argv[1] == "--check":
            print("expand.py: %d veraltete Dateien" % bad)
            return 1 if bad else 0
        return 0
    if len(argv) != 3:
        print(__doc__)
        return 2
    text = build(argv[1], argv[2])
    with open(argv[2], "w", encoding="utf-8") as f:
        f.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
