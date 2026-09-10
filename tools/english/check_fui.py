#!/usr/bin/env python3
"""tools/english/check_fui.py -- THE THIRD NET, FOR fUi.

The two existing nets (check_comments.py, check_comments2.py) only look
at lines that START with `//`. That let a whole class through: the
TRAILING comment after code --

    caret: usize,   // die Schreibmarke, Oktettversatz

Thirty-two of those survived the first pass because of exactly that.
This net looks at everything after `//` anywhere on a line, and it also
checks IDENTIFIERS, which no comment net ever sees.

Return code 1 while anything German is left.

    python3 tools/english/check_fui.py            what is left
    python3 tools/english/check_fui.py --quiet    only the count
"""
import os, re, sys, glob

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)

# German function/content words. Deliberately WITHOUT words that are
# also English (die, was, in, an, am, es, war, hat, name, wert, also,
# grab, lies) -- the first version of the comment net flagged correct
# English prose because of those.
WORDS = """der das den dem des und oder aber nicht kein keine keinen keiner
nur noch schon auch sonst damit dass weil wenn dann als wie sind waren wird
werden wurde wurden haben hatte hatten kann koennen konnte muss muessen
musste soll sollen darf duerfen fuer von vom mit ohne bei beim nach vor
ueber unter zwischen durch gegen seit aus zum zur sie wir sich jede jeder
jedes alle alles beide dieser diese dieses dabei dafuer daraus davon dazu
deshalb darum trotzdem immer nie oft selten weniger sehr ganz genau erst
zuerst danach spaeter zuvor wieder anders richtig falsch wichtig moeglich
noetig etwa etwas nichts jetzt heute zeile zeilen datei dateien werte namen
malen malt zeichnet zeigt steht liegt gehoert gehoerend wieviele wieviel
belegt taste tasten zeiger auswahl stufen aenderung zeichen leinwand
schrift schriftsatz puffer oktett platz schreibmarke markierung schwebender
gezogen anfangsgriff endgriff waagrechter senkrechter versatz bildpunkt
bildpunkten fenster leere leiste tafel grund karte menue naeher auge weiss
darauf knopf knoepfe akzent anfasst mindestens hoch listenzeile themas
flaeche breite hoehe farbe rand kasten kaesten summe luecke luecken letzte
vorgabe""".split()
RE_WORD = re.compile(r'\b(' + '|'.join(WORDS) + r')\b', re.I)

# German identifiers that must not appear as code any more.
IDENTS = re.compile(r'\b(summe|luecken?|letzte[nrs]?|rand_[lorug]|sammelt|'
                    r'ab_lesen|ab_schreiben|vorgabe|knopf\w*|vorlage\w*|'
                    r'malen|belegt|schwer_[xy]|zaehl\w*|sag|falsch|'
                    r'draussen|kaesten|mitte|kopf|mach|weiss)\b')

# `...` is code or a quoted identifier, not prose.
CODESPAN = re.compile(r'`[^`]*`')
# A real path on disk that happens to be German must stay.
KEEP = ('knopf-vorlage', 'KNOPF-VORLAGE', 'lib/fenster', 'fenster.fi',
        'rgb_nach_bgrx', 'x11.fi', 'win32.fi')


def comment_of(line):
    """Everything after the first // that is not inside a string."""
    inside = False
    i = 0
    while i < len(line) - 1:
        c = line[i]
        if c == '"' and (i == 0 or line[i - 1] != '\\'):
            inside = not inside
        elif c == '/' and line[i + 1] == '/' and not inside:
            return line[i + 2:]
        i += 1
    return ''


def code_of(line):
    """The line without its comment and without string literals."""
    inside = False
    out = []
    i = 0
    while i < len(line):
        c = line[i]
        if c == '"' and (i == 0 or line[i - 1] != '\\'):
            inside = not inside
            continue_ = True
        if i < len(line) - 1 and c == '/' and line[i + 1] == '/' and not inside:
            break
        if not inside and c != '"':
            out.append(c)
        i += 1
    return ''.join(out)


def main():
    quiet = '--quiet' in sys.argv
    files = sorted(glob.glob('lib/fui/*.fi') + glob.glob('tools/fui/*.fi')
                   + glob.glob('tools/fui/*.sh'))
    hits = 0
    for path in files:
        for n, line in enumerate(open(path, encoding='utf-8'), 1):
            line = line.rstrip('\n')
            if any(k in line for k in KEEP):
                continue
            com = CODESPAN.sub(' ', comment_of(line))
            m = RE_WORD.search(com)
            if m:
                hits += 1
                if not quiet:
                    print('%s:%d: [comment: %s] %s' % (path, n, m.group(1), line.strip()[:96]))
            mi = IDENTS.search(code_of(line))
            if mi:
                hits += 1
                if not quiet:
                    print('%s:%d: [name: %s] %s' % (path, n, mi.group(1), line.strip()[:96]))
    print('fUi: %d German leftovers' % hits)
    return 1 if hits else 0


sys.exit(main())
