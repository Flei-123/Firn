#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""
mp3_tabellen.py -- erzeugt lib/ton/mp3_tab.fi aus minimp3.h.

WARUM EIN GENERATOR. Die Tabellen des MP3-Dekoders sind ueber 2500 Zahlen,
darunter der gepackte Huffman-Baum (`tabs`, ~2100 int16). Wer die abtippt,
tippt sie falsch. Der Generator liest die ECHTEN Werte aus der Vorlage
(minimp3, CC0, https://github.com/lieff/minimp3) und schreibt sie als
Firn-Quelltext. Firn kennt keine globalen Variablen (SPEC 14.1), also
werden die Tabellen in einen Satz gepackt, den der Aufrufer einmal baut
und danach als Zeiger weiterreicht.

Aufruf:  python3 tools/mp3_tabellen.py /tmp/minimp3.h > lib/ton/mp3_tab.fi
"""
import re
import sys


def hole(text, name, zeile=0):
    """Den Klammerinhalt der Deklaration `name` als flache Zahlenliste.

    `zeile` > 0: die Tabelle ist zweidimensional und JEDE innere Zeile wird
    auf diese Breite mit Nullen aufgefuellt -- genau das, was C mit einer
    zu kurz geschriebenen Initialisierung tut (g_scf_mixed hat Zeilen mit
    37 statt 40 Werten). Ohne das Auffuellen verrutscht die ganze Tabelle.
    """
    i = text.index(name)
    a = text.index('{', i)
    tiefe = 0
    for j in range(a, len(text)):
        if text[j] == '{':
            tiefe += 1
        elif text[j] == '}':
            tiefe -= 1
            if tiefe == 0:
                roh = text[a:j + 1]
                break
    roh = re.sub(r'/\*.*?\*/', '', roh, flags=re.S)
    if zeile > 0:
        aus = []
        for teil in re.findall(r'\{([^{}]*)\}', roh[1:-1]):
            werte = [w.strip() for w in teil.split(',') if w.strip()]
            if len(werte) > zeile:
                sys.exit('Zeile in %s hat %d > %d Werte' % (name, len(werte), zeile))
            aus.extend(werte + ['0'] * (zeile - len(werte)))
        return aus
    roh = roh.replace('{', ' ').replace('}', ' ')
    werte = [w.strip() for w in roh.split(',')]
    return [w for w in werte if w]


def zahl(w):
    w = w.rstrip('fF') if re.match(r'^-?[0-9.]+[fF]$', w) else w
    return w


def ganz(werte):
    return [str(int(float(zahl(w)))) for w in werte]


def komma(werte):
    """Fliesskomma im Firn-Format: `1.5f` (f32-Literal)."""
    aus = []
    for w in werte:
        s = zahl(w)
        if '.' not in s and 'e' not in s and 'E' not in s:
            s = s + '.0'
        aus.append(s + 'f')
    return aus


def block(namen, werte, breite=12):
    zeilen = []
    for i in range(0, len(werte), breite):
        zeilen.append('        ' + ', '.join(werte[i:i + breite]) + ',')
    return '    ' + namen + ': [\n' + '\n'.join(zeilen) + '\n    ],'


def main():
    quelle = open(sys.argv[1], encoding='utf-8', errors='replace').read()

    # ganze Zahlen
    scf_long = ganz(hole(quelle, 'g_scf_long[8][23]', 23))
    scf_short = ganz(hole(quelle, 'g_scf_short[8][40]', 40))
    scf_mixed = ganz(hole(quelle, 'g_scf_mixed[8][40]', 40))
    partitions = ganz(hole(quelle, 'g_scf_partitions[3][28]', 28))
    scfc = ganz(hole(quelle, 'g_scfc_decode[16]'))
    gmod = ganz(hole(quelle, 'g_mod[6*4]'))
    preamp = ganz(hole(quelle, 'g_preamp[10]'))
    tabs = ganz(hole(quelle, 'int16_t tabs[]'))
    tab32 = ganz(hole(quelle, 'tab32[]'))
    tab33 = ganz(hole(quelle, 'tab33[]'))
    tabindex = ganz(hole(quelle, 'tabindex[2*16]'))
    linbits = ganz(hole(quelle, 'g_linbits[]'))
    halfrate = ganz(hole(quelle, 'halfrate[2][3][15]', 15))

    # Fliesskomma
    pow43 = komma(hole(quelle, 'g_pow43[129 + 16]'))
    pan = komma(hole(quelle, 'g_pan[7*2]'))
    aa = komma(hole(quelle, 'g_aa[2][8]', 8))
    twid9 = komma(hole(quelle, 'g_twid9[18]'))
    twid3 = komma(hole(quelle, 'g_twid3[6]'))
    fenster = komma(hole(quelle, 'g_mdct_window[2][18]', 18))
    sec = komma(hole(quelle, 'g_sec[24]'))
    win = komma(hole(quelle, 'g_win[]'))

    masse = {
        'scf_long': (len(scf_long), 8 * 23), 'scf_short': (len(scf_short), 8 * 40),
        'scf_mixed': (len(scf_mixed), 8 * 40), 'partitions': (len(partitions), 3 * 28),
        'scfc': (len(scfc), 16), 'gmod': (len(gmod), 24), 'preamp': (len(preamp), 10),
        'tab32': (len(tab32), 28), 'tab33': (len(tab33), 16),
        'tabindex': (len(tabindex), 32), 'linbits': (len(linbits), 32),
        'halfrate': (len(halfrate), 90), 'pow43': (len(pow43), 145),
        'pan': (len(pan), 14), 'aa': (len(aa), 16), 'twid9': (len(twid9), 18),
        'twid3': (len(twid3), 6), 'fenster': (len(fenster), 36), 'sec': (len(sec), 24),
        'win': (len(win), 240),
    }
    for k, (ist, soll) in masse.items():
        if ist != soll:
            sys.exit('Tabelle %s hat %d statt %d Werte' % (k, ist, soll))

    kopf = '''// SPDX-License-Identifier: MPL-2.0
// lib/ton/mp3_tab.fi -- DIE TABELLEN DES MP3-DEKODERS. ERZEUGT, NICHT GETIPPT.
//
// Diese Datei wird von `tools/mp3_tabellen.py` aus der Vorlage minimp3
// (CC0-1.0, https://github.com/lieff/minimp3) erzeugt. Von Hand aendern
// heisst: die naechste Erzeugung wirft die Aenderung weg.
//
// Firn hat keine globalen Variablen (SPEC 14.1 Punkt 5). Alle Tabellen
// liegen deshalb in EINEM Satz `Tabs`, den der Aufrufer einmal mit
// `tabs_new()` baut und danach als `*mut Tabs` weiterreicht. Der Satz ist
// rund %d KiB gross -- er gehoert auf den Haufen oder in einen langlebigen
// Rahmen, nicht in eine Schleife.
//
// ANZAHL DER WERTE: Huffman %d, Fenster %d, pow(4/3) %d.

export { Tabs, tabs_new }

struct Tabs {
''' % ((len(tabs) * 2 + len(win) * 4) // 1024, len(tabs), len(win), len(pow43))

    felder = [
        ('scf_long', 'u8', 8 * 23), ('scf_short', 'u8', 8 * 40),
        ('scf_mixed', 'u8', 8 * 40), ('partitions', 'u8', 3 * 28),
        ('scfc_decode', 'u8', 16), ('modulo', 'u8', 24), ('preamp', 'u8', 10),
        ('tabs', 'i16', len(tabs)), ('tab32', 'u8', 28), ('tab33', 'u8', 16),
        ('tabindex', 'i16', 32), ('linbits', 'u8', 32), ('halfrate', 'u8', 90),
        ('pow43', 'f32', 145), ('pan', 'f32', 14), ('aa', 'f32', 16),
        ('twid9', 'f32', 18), ('twid3', 'f32', 6), ('fenster', 'f32', 36),
        ('sec', 'f32', 24), ('win', 'f32', 240),
    ]
    aus = [kopf]
    for name, typ, n in felder:
        aus.append('    %s: [%s; %d],' % (name, typ, n))
    aus.append('}\n')
    aus.append('fn tabs_new() -> Tabs {')
    aus.append('    return Tabs {')
    daten = [
        ('scf_long', scf_long), ('scf_short', scf_short), ('scf_mixed', scf_mixed),
        ('partitions', partitions), ('scfc_decode', scfc), ('modulo', gmod),
        ('preamp', preamp), ('tabs', tabs), ('tab32', tab32), ('tab33', tab33),
        ('tabindex', tabindex), ('linbits', linbits), ('halfrate', halfrate),
        ('pow43', pow43), ('pan', pan), ('aa', aa), ('twid9', twid9),
        ('twid3', twid3), ('fenster', fenster), ('sec', sec), ('win', win),
    ]
    for name, werte in daten:
        aus.append(block(name, werte))
    aus.append('    }')
    aus.append('}')
    print('\n'.join(aus))


main()
