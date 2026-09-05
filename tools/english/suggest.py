#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
# tools/english/suggest.py — baut aus der Morphemtabelle und den
# Handentscheidungen die VOLLSTAENDIGE Bezeichner-Abbildung `names.tsv`.
#
#   morphemes.tsv  deutsches Wortteil  -> englisches Wortteil
#   manual.tsv      GANZER Bezeichner   -> englischer Bezeichner (schlaegt alles)
#   names.tsv     Ergebnis: alt -> neu, wird von rename.py angewandt
#
# Geprueft wird dabei:
#   * kein Zielname ist ein Schluesselwort von Rust oder Firn
#   * die Abbildung ist INJEKTIV (zwei deutsche Namen duerfen nicht auf
#     denselben englischen Namen fallen — sonst verdeckt einer den anderen)
#   * kein Zielname kollidiert in DERSELBEN Datei mit einem Bezeichner,
#     der unveraendert bleibt
# Alles davon wird als Fehler gemeldet; `names.tsv` entsteht trotzdem, damit
# man die Meldungen abarbeiten kann.
import re
import os
import sys
import collections

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)
sys.path.insert(0, 'tools/english')
import source as Q

RUST_KW = set("""as break const continue crate dyn else enum extern false fn for if impl in let
loop match mod move mut pub ref return self Self static struct super trait true type unsafe use
where while async await box become do final macro override priv typeof unsized virtual yield try
abstract union""".split())
FIRN_KW = set("""fn let var if else while return struct const profile as mut true false syscall
extern break defer errdefer comptime continue for in import export enum match error try catch""".split())
RESERVIERT = RUST_KW | FIRN_KW

ROOTS = ['compiler/src', 'lib', 'bin', 'tools', 'demos', 'examples', 'tests', 'bench', 'testdata']
TEIL = re.compile(r'[a-z0-9]+|[A-Z]+(?![a-z])|[A-Z][a-z0-9]*')


def dateien():
    aus = []
    for r in ROOTS:
        for d, _, fs in os.walk(r):
            if 'target' in d or '__pycache__' in d or '.test-work' in d:
                continue
            for f in fs:
                q = os.path.join(d, f)
                if f.endswith(('.rs', '.fi')) and not os.path.islink(q):
                    aus.append(q)
    return sorted(aus)


def tabelle(pfad):
    t = {}
    if not os.path.exists(pfad):
        return t
    for z in open(pfad, encoding='utf-8'):
        if not z.strip() or z.startswith('#'):
            continue
        d, e = z.rstrip('\n').split('\t')
        t[d] = e
    return t


def teile(name):
    """Bezeichner in Morpheme zerlegen: _-Grenzen und CamelCase."""
    return [TEIL.findall(g) or [g] for g in name.split('_')]


def uebersetze(name, morph):
    """Morphemweise Uebersetzung. Gibt (neuer_name, getroffen, offene_teile)."""
    neu_gruppen = []
    getroffen = False
    offen = []
    for g in teile(name):
        neu = []
        for t in g:
            low = t.lower()
            if low in morph:
                e = morph[low]
                # Ein englisches Morphem darf aus mehreren Woertern bestehen
                # (`sprungtabelle` -> `jump_table`). In snake_case bleibt der
                # Unterstrich, in CamelCase wird daraus JumpTable.
                if t.isupper() and len(t) > 1:
                    e = e.replace('_', '').upper()
                elif t[:1].isupper():
                    e = ''.join(w[:1].upper() + w[1:] for w in e.split('_'))
                neu.append(e)
                getroffen = True
            else:
                neu.append(t)
                if len(low) >= 3 and re.fullmatch(r'[a-z]+', low):
                    offen.append(low)
        neu_gruppen.append(''.join(neu) if len(g) > 1 else neu[0])
    return '_'.join(neu_gruppen), getroffen, offen


DEF = re.compile(r'\\b(?:fn|struct|enum|const|static|type|union|trait)\\s+(?P<n>[A-Za-z_][A-Za-z0-9_]*)')
FN = re.compile(r'^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?(?:const[ \t]+)?(?:unsafe[ \t]+)?'
                r'(?:extern[ \t]+"[^"]*"[ \t]+)?(?:fn|struct|enum|impl|trait|union)[ \t]', re.M)


def rumpfe(src, rust):
    """Grobe Zerlegung in Rumpfe (fn/struct/enum/impl) (alles vor dem ersten `fn` ist der
    Datei-Kopf und zaehlt als eigener Rumpf)."""
    pos = [m.start() for m in FN.finditer(src)]
    if not pos:
        return [src]
    aus = [src[:pos[0]]]
    for i, a in enumerate(pos):
        e = pos[i + 1] if i + 1 < len(pos) else len(src)
        aus.append(src[a:e])
    return aus


def main():
    morph = tabelle('tools/english/morphemes.tsv')
    hand = tabelle('tools/english/manual.tsv')
    geprueft = set(tabelle('tools/english/checked.tsv').items())
    handd = {}
    if os.path.exists('tools/english/manual_file.tsv'):
        for z in open('tools/english/manual_file.tsv', encoding='utf-8'):
            if not z.strip() or z.startswith('#'):
                continue
            d, a, n = z.rstrip('\n').split('\t')
            handd[(d, a)] = n

    fs = dateien()
    proDatei = {}
    proDateiFrei = {}
    haeufig = collections.Counter()
    for p in fs:
        s = open(p, encoding='utf-8', errors='replace').read()
        b = Q.bezeichner(s, p.endswith('.rs'))
        proDatei[p] = b
        proDateiFrei[p] = Q.freie_bezeichner(s, p.endswith('.rs'))
        for k, v in b.items():
            haeufig[k] += v

    abb = {}
    offen = collections.Counter()
    for name in haeufig:
        if name in hand:
            if hand[name] != name:
                abb[name] = hand[name]
            continue
        neu, hit, off = uebersetze(name, morph)
        if hit and neu != name:
            abb[name] = neu
            for o in off:
                offen[o] += 1

    fehler = []
    # 1. Schluesselwoerter
    for a, n in sorted(abb.items()):
        if n in RESERVIERT:
            fehler.append(f"SCHLUESSELWORT  {a} -> {n}")
    # 2. Zwei deutsche Namen auf denselben englischen — gefaehrlich ist das
    #    nur, wenn beide in DERSELBEN Datei vorkommen (dann verdeckt einer
    #    den anderen). Sonst ist es blosse Wiederverwendung eines Wortes.
    rueck = collections.defaultdict(list)
    for a, n in abb.items():
        rueck[n].append(a)
    mehrdeutig = {n: q for n, q in rueck.items() if len(q) > 1}
    # 3. Kollision mit einem Namen, der im SELBEN RUMPF unveraendert bleibt.
    #    Nur dann kann die Umbenennung wirklich etwas verdecken; zwei gleiche
    #    Namen in verschiedenen Funktionen einer Datei sind harmlos.
    koll = collections.defaultdict(set)
    mehr = collections.defaultdict(set)
    defdop = collections.defaultdict(set)
    for p in proDatei:
        src = open(p, encoding='utf-8', errors='replace').read()
        rust = p.endswith('.rs')
        lokal = {a: n for (d, a), n in handd.items() if d == p}
        for rumpf in rumpfe(src, rust):
            frei = Q.freie_bezeichner(rumpf, rust)
            bleibt = {k for k in frei if k not in abb}
            for k in frei:
                if k in lokal:
                    continue
                n = abb.get(k)
                if n is None:
                    continue
                if n in bleibt and (k, n) not in geprueft:
                    koll[(k, n)].add(p)
                if n in mehrdeutig:
                    zus = sorted(q for q in mehrdeutig[n] if q in frei and q not in lokal
                                 and (q, n) not in geprueft)
                    if len(zus) > 1:
                        mehr[(n, tuple(zus))].add(p)
        # doppelte DEFINITIONEN nach der Umbenennung
        defs = collections.defaultdict(list)
        for m in DEF.finditer(''.join(t for a, t in Q.regionen(src, rust) if a == 'code')):
            d = m.group('n')
            defs[lokal.get(d, abb.get(d, d))].append(d)
        for neu_name, quellen in defs.items():
            if len(set(quellen)) > 1:
                defdop[(neu_name, tuple(sorted(set(quellen))))].add(p)
    for (n, zus), ps in sorted(mehr.items()):
        fehler.append(f"MEHRDEUTIG      {n} <- {', '.join(zus)}  ({sorted(ps)[0]})")
    for (n, zus), ps in sorted(defdop.items()):
        fehler.append(f"DOPPELDEF       {n} <- {', '.join(zus)}  ({sorted(ps)[0]})")
    for (k, n), ps in sorted(koll.items()):
        fehler.append(f"KOLLISION       {k} -> {n}  (schon in {', '.join(sorted(ps)[:3])})")
    # 4. Ein Zielname darf nicht selbst wieder Quellname sein (sonst haengt
    #    das Ergebnis von der Reihenfolge ab).
    for a, n in sorted(abb.items()):
        if n in abb:
            fehler.append(f"KETTE           {a} -> {n} -> {abb[n]}")

    with open('tools/english/names.tsv', 'w', encoding='utf-8') as f:
        for k in sorted(abb):
            f.write(f"{k}\t{abb[k]}\n")
    with open('tools/english/names_file.tsv', 'w', encoding='utf-8') as f:
        for (d, a) in sorted(handd):
            f.write(f"{d}\t{a}\t{handd[(d, a)]}\n")
    with open('tools/english/open.txt', 'w', encoding='utf-8') as f:
        for w, c in offen.most_common():
            f.write(f"{c}\t{w}\n")

    print(f"dateien: {len(fs)}  bezeichner: {len(haeufig)}  umzubenennen: {len(abb)}")
    print(f"unbekannte wortteile in umbenannten namen: {len(offen)}")
    if fehler:
        print(f"--- {len(fehler)} PROBLEME ---")
        for z in fehler:
            print(z)
    return 1 if fehler else 0


if __name__ == '__main__':
    sys.exit(main())
