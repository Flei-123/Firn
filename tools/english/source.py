#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
# tools/english/source.py — winziger Lexer fuer .rs und .fi, der den
# Quelltext in CODE und NICHT-CODE (Kommentar, Zeichenkette, Zeichenliteral)
# zerlegt. Damit trifft eine Umbenennung nur echte Bezeichner und mangelt
# nicht die deutsche Prosa in den Kommentaren (das ist Etappe B).
#
# Firn:  //-Zeile, /* */ (schachtelbar), "..." mit den Praefixen b/u/f.
# Rust:  dazu r"..."/r#"..."#/br#"..."# und 'c' gegen 'lebenszeit.
import re

IDENT = re.compile(r'[A-Za-z_][A-Za-z0-9_]*')
# Rust-Zeichenliteral: '\\n', '\\'', '\\\\', '\\x41', '\\u{1F600}' oder ein einzelnes Zeichen
ZEICHEN = re.compile(r"'(?:\\\\(?:x[0-9a-fA-F]{2}|u\\{[0-9a-fA-F]{1,6}\\}|.)|[^\\\\'])'")


def regionen(s, rust=False):
    """Zerlegt `s` in eine Liste (art, text) mit art in {'code','skip'}.
    ''.join(text) ergibt wieder genau `s`."""
    out = []
    n = len(s)
    i = 0
    anfang = 0

    def code_bis(j):
        if j > anfang:
            out.append(('code', s[anfang:j]))

    while i < n:
        c = s[i]
        # ---------------------------------------------------------- Kommentar
        if c == '/' and i + 1 < n and s[i + 1] == '/':
            code_bis(i)
            j = s.find('\n', i)
            j = n if j < 0 else j
            out.append(('skip', s[i:j]))
            i = anfang = j
            continue
        if c == '/' and i + 1 < n and s[i + 1] == '*':
            code_bis(i)
            tiefe = 1
            j = i + 2
            while j < n and tiefe > 0:
                if s[j] == '/' and j + 1 < n and s[j + 1] == '*':
                    tiefe += 1
                    j += 2
                elif s[j] == '*' and j + 1 < n and s[j + 1] == '/':
                    tiefe -= 1
                    j += 2
                else:
                    j += 1
            out.append(('skip', s[i:j]))
            i = anfang = j
            continue
        # ------------------------------------------------- Rust-Rohliteral
        if rust and c in 'rb':
            m = re.match(r'(?:b?r)(#*)"', s[i:])
            if m and (i == 0 or not (s[i - 1].isalnum() or s[i - 1] == '_')):
                rauten = m.group(1)
                ende = s.find('"' + rauten, i + m.end())
                j = n if ende < 0 else ende + 1 + len(rauten)
                code_bis(i)
                out.append(('skip', s[i:j]))
                i = anfang = j
                continue
        # ------------------------------------------------------ Zeichenkette
        if c == '"':
            code_bis(i)
            j = i + 1
            while j < n:
                if s[j] == '\\':
                    j += 2
                    continue
                if s[j] == '"':
                    j += 1
                    break
                j += 1
            out.append(('skip', s[i:j]))
            i = anfang = j
            continue
        # ------------------------- Rust: Zeichenliteral gegen Lebenszeitname
        if rust and c == "'":
            m = ZEICHEN.match(s, i)
            if not m:
                i += 1          # Lebenszeit ('a, 'static) — bleibt Code
                continue
            j = m.end()
            code_bis(i)
            out.append(('skip', s[i:j]))
            i = anfang = j
            continue
        i += 1
    code_bis(n)
    return out


def ersetze(s, abb, rust=False):
    """Wendet die Abbildung {alt: neu} auf alle BEZEICHNER im Code an.
    Gibt (neuer_text, anzahl_ersetzungen) zurueck."""
    zahl = 0
    teile = []
    for art, text in regionen(s, rust):
        if art == 'skip':
            teile.append(text)
            continue
        def f(m):
            nonlocal zahl
            neu = abb.get(m.group(0))
            if neu is None:
                return m.group(0)
            zahl += 1
            return neu
        teile.append(IDENT.sub(f, text))
    return ''.join(teile), zahl


def bezeichner(s, rust=False):
    """Alle Bezeichner im CODE-Teil (mit Haeufigkeit)."""
    aus = {}
    for art, text in regionen(s, rust):
        if art != 'code':
            continue
        for m in IDENT.finditer(text):
            aus[m.group(0)] = aus.get(m.group(0), 0) + 1
    return aus


def freie_bezeichner(s, rust=False):
    """Nur Bezeichner, die MINDESTENS EINMAL nicht hinter einem '.' stehen —
    also nicht bloss Methoden-/Feldnamen fremder Typen (`x.is_empty()`).
    Fuer die Kollisionspruefung ist nur das interessant."""
    aus = set()
    for art, text in regionen(s, rust):
        if art != 'code':
            continue
        for m in IDENT.finditer(text):
            i = m.start()
            if i > 0 and text[i - 1] == '.':
                continue            # x.feld / modul.fn
            if i > 1 and text[i - 2:i] == '::':
                continue            # Enum::Variante / modul::fn
            aus.add(m.group(0))
    return aus


def selbsttest(s, rust=False):
    """Grobpruefung: in einem CODE-Bereich darf kein Anfuehrungszeichen und
    kein Kommentaranfang mehr stehen. Faellt der Lexer aus dem Tritt, faellt
    es hier auf."""
    schlecht = []
    for art, text in regionen(s, rust):
        if art != 'code':
            continue
        for z in ('"', '//', '/*'):
            if z in text:
                i = text.index(z)
                schlecht.append((z, text[max(0, i - 40):i + 40]))
    return schlecht
