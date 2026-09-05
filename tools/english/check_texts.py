#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/english/check_texts.py — GEGENPROBE fuer die AUSGABETEXTE.

Sucht in allen ZEICHENKETTENLITERALEN der beiden Uebersetzer (compiler/src/*.rs,
lib/firnc1/*.fi, bin/*.fi) nach deutschen Woertern. Ein Treffer heisst: dieser
Ausgabetext ist noch deutsch.

WOHER DIE WORTLISTE KOMMT (mechanisch, nicht von Hand gepflegt): Vor der
Umstellung (Basis-Commit) war JEDER Text deutsch. Also gilt

    Deutsch = {Woerter in den Literalen des Basis-Commits}
              - {Woerter der englischen Spalte von messages.tsv}
              - {englische Wortliste ENGLISCH unten}

Damit faellt jedes Wort auf, das aus der alten deutschen Welt uebrig geblieben
ist — auch solche, die in morphemes.tsv nie standen (`quelltext`, `hinweis`, …).
Kommentare zaehlen NICHT, die sind Etappe B. Ausnahmen: text_exceptions.txt.
"""
import os, re, subprocess, sys, glob

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)
BASIS = os.environ.get('ENGLISCH_BASIS', '370e6b9')

# Woerter, die in beiden Sprachen vorkommen oder ohnehin englisch sind.
ENGLISCH = set("""a all also an and any arc arg args as asm at attr base bin bit
block bool break by byte bytes call can cap case cast char class code col const
context continue copy core count data debug def defer dev dyn else emit end
entry enum error exe exit expr extern false field file fir first flag float fn
for found from gc get global has have head heap here hot i if impl import in
index inline insts int interface is it item key kind label lang layout len let
level lib line link list load local lock loop low main map match max may mem
memory meta method mid min mod mode module move mut name needs new no node not
note null number object of off offset ok on one only op opt or order os out
over pack page pair parent pass path payload point pointer pop print profile
program ptr public push raw read ref release ret return root row run safe scope
select self set shift show side size slot slow so source span src stack stage
start state static std step store str string struct sub sum switch sync table
tag target test text than that the then this thread three time to top total
true try two type types u unit units unknown up use used user value var vec
version via void was way weak while width with word write yes zero
rax rbx rcx rdx rsi rdi rsp rbp rip eax ebx ecx edx esi edi esp ebp ax bx cx dx
al bl cl dl spl bpl sil dil qword dword byteptr ptr mov movzx movsx movsb stosb
rep lea push pop jmp jne jle jge ja jae jb jbe jc jz jnz js jns jo jno jp jl jg
call ret leave hlt iretq syscall xor and shl
shr sar imul idiv cqo cdq cmp test sete setne cmovne cmove xadd cmpxchg lock
section text rodata data bss globl intel syntax noprefix progbits gnu stack
note align quad long short zero_ fs gs cfi endbr nop neg not sub add div mul
rem calli regs copymem stdin stdout stderr proc usr bin exe elf abi
err val arr idx blk slit alit awdh zuw param fseg fmt ptrmut init alloc collect
live objects pause barriers tokens firn""".split())

def lange_morpheme():
    """Deutsche Morpheme ab 5 Zeichen aus morphemes.tsv — fuer einwortige Literale."""
    p = 'tools/english/morphemes.tsv'
    w = set()
    if os.path.exists(p):
        for z in open(p, encoding='utf-8'):
            d = z.split('\t')[0].strip().lower()
            if len(d) >= 5 and d.isalpha():
                w.add(d)
    return w


LANGE_MORPHEME = lange_morpheme()

LIT = re.compile(r'"((?:[^"\\\n]|\\.)*)"', re.S)
WORT = re.compile(r'[A-Za-z]{3,}')
# ROUND 88: `lib/` joined the list. Until then only the two COMPILERS were
# checked -- and that is exactly why `firn-gc: gc_init() wurde nicht
# aufgerufen` (lib/gc/gc.fi) survived every check since round 55: it is a
# RUN TIME text, it lives in the library, and nobody looked there. Whoever
# writes a message in `lib/**.fi` is now caught by the same net.
DATEIEN = ('compiler/src', 'lib/firnc1', 'bin', 'lib')


def literale(text):
    text = re.sub(r'(?m)^\s*//.*$', '', text)
    for m in LIT.finditer(text):
        yield m.group(1)


def basis_woerter():
    aus = subprocess.run(['git', 'ls-tree', '-r', '--name-only', BASIS],
                         capture_output=True, text=True).stdout.split('\n')
    w = set()
    for f in aus:
        # Die WORTLISTE kommt weiter nur aus den beiden Uebersetzern: sie
        # bestimmt, was ueberhaupt als deutsch gilt. Wuerde lib/ mitzaehlen,
        # kaemen aus HTML-Testdaten Woerter wie `frame` oder `style` hinein
        # und jeder englische Satz mit `frame` waere ein Fehlalarm.
        if not (f.startswith('compiler/src/') or f.startswith('lib/firnc1/')
                or f.startswith('bin/')):
            continue
        if not (f.endswith('.rs') or f.endswith('.fi')):
            continue
        r = subprocess.run(['git', 'show', BASIS + ':' + f],
                           capture_output=True, text=True)
        if r.returncode:
            continue
        for lit in literale(r.stdout):
            w |= {x.lower() for x in WORT.findall(lit)}
    return w


def englische_spalte():
    w = set()
    p = 'tools/english/messages.tsv'
    if os.path.exists(p):
        for z in open(p, encoding='utf-8'):
            if '\t' in z:
                w |= {x.lower() for x in WORT.findall(z.split('\t')[1])}
    return w


def ausnahmen():
    p = 'tools/english/text_exceptions.txt'
    if not os.path.exists(p):
        return set()
    w = set()
    for z in open(p, encoding='utf-8'):
        if z.strip() and not z.startswith('#'):
            w |= {x.lower() for x in z.split()}
    return w


def quelldateien():
    """Alle Quellen der beiden Uebersetzer UND der Bibliothek, rekursiv."""
    aus = []
    for wurzel in DATEIEN:
        for ordner, _, namen in os.walk(wurzel):
            for n in sorted(namen):
                aus.append(os.path.join(ordner, n))
    return sorted(set(aus))


def main():
    de = basis_woerter() - englische_spalte() - ENGLISCH - ausnahmen()
    treffer = []
    for f in quelldateien():
            if os.path.islink(f) or f.endswith('gctext.fi'):
                continue
            if not (f.endswith('.rs') or f.endswith('.fi')):
                continue
            s = open(f, encoding='utf-8').read()
            zeilen = s.split('\n')
            for lit in literale(s):
                # Nur PROSA pruefen: mindestens zwei durch Leerzeichen
                # getrennte Woerter. Einzelne Bezeichner, Register- und
                # Tokenlisten sind Sache von check.py (Bezeichner).
                # PROSA (mindestens zwei Woerter) wird immer geprueft.
                # Einwortige Literale (`"bitgleich"`) nur dann, wenn im Wort
                # ein deutsches Morphem ab 5 Zeichen steckt — sonst waeren
                # HTML-Namen und Testdaten (`img`, `abc`) lauter Fehlalarme.
                if len(re.findall(r'(?:^|[ ,;:.])[A-Za-z]{3,}', lit)) < 2:
                    w = lit.strip().lower()
                    if not re.fullmatch(r'[a-z]{3,}', w):
                        continue
                    if not any(m in w for m in LANGE_MORPHEME):
                        continue
                schlecht = sorted({x.lower() for x in WORT.findall(lit)} & de)
                if not schlecht:
                    continue
                nr = next((i + 1 for i, z in enumerate(zeilen)
                           if lit.split('\n')[0][:40] in z), 0)
                treffer.append((f, nr, ','.join(schlecht), lit[:70]))
    for t in treffer:
        print('GERMAN  %s:%d  [%s]  %s' % t)
    print('German text sites:', len(treffer))
    return 1 if treffer else 0


sys.exit(main())
