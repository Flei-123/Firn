#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/fui/pywords_table.py -- GENERATES lib/fui/pywords.fi FROM tools/fui/pywords.txt.
#
# WHY A SCRIPT: the word list is data, and data that nobody can check is an
# opinion. Every entry "word pinyin" is checked against the syllable table
# (lib/fui/pinyin.fi): the key must split into exactly one syllable per
# character, and each character must be listed under its syllable. An entry
# that fails is REJECTED and reported (exit code 1), so the generated file
# never holds a word the single-character input could not produce itself.
#
# THE APOSTROPHE: if the intended split differs from what the input's greedy
# longest-syllable split would do ("xi'an" would become "xian"), the key is
# stored WITH the apostrophe, and only an apostrophe typed there matches it.
#
# Call:  python3 tools/fui/pywords_table.py > lib/fui/pywords.fi
import re, sys, os

HERE = os.path.dirname(os.path.abspath(__file__))
LIB = os.path.join(HERE, "..", "..", "lib", "fui")

syl = {}
for line in open(os.path.join(LIB, "pinyin.fi"), encoding="utf-8"):
    m = re.match(r'static PY_[A-Z]: \[u8; \d+\] = "(.*)"$', line.rstrip("\n"))
    if m:
        for ent in m.group(1).split():
            k, v = ent.split(":")
            syl[k] = v

def greedy(key):
    out, i = [], 0
    while i < len(key):
        for k in range(min(6, len(key) - i), 0, -1):
            if key[i:i + k] in syl:
                out.append(key[i:i + k])
                i += k
                break
        else:
            return None
    return out

def splits(key, word):
    if not word:
        return [[]] if not key else []
    res = []
    for k in range(1, min(6, len(key)) + 1):
        s = key[:k]
        if s in syl and word[0] in syl[s]:
            for rest in splits(key[k:], word[1:]):
                res.append([s] + rest)
    return res

blocks = {}
bad = []
total = 0
seen = set()
for line in open(os.path.join(HERE, "pywords.txt"), encoding="utf-8"):
    line = line.strip()
    if not line or line.startswith("#"):
        continue
    word, key = line.split()
    if (word, key) in seen:
        bad.append((word, key, "duplicate"))
        continue
    seen.add((word, key))
    if len(word) < 2 or any(len(c.encode()) != 3 for c in word):
        bad.append((word, key, "not 2+ three-byte characters"))
        continue
    sp = splits(key, word)
    if not sp:
        bad.append((word, key, "no split fits the syllable table"))
        continue
    g = greedy(key)
    chosen = g if g in sp else sp[0]
    stored = key if g in sp else "'".join(chosen)
    blocks.setdefault(stored[0], []).append("%s:%s" % (stored, word))
    total += 1

if bad:
    for w, k, why in bad:
        sys.stderr.write("REJECTED %s %s: %s\n" % (w, k, why))

o = []
o.append("// SPDX-License-Identifier: MPL-2.0")
o.append("// lib/fui/pywords.fi -- THE WORD LIST OF THE BUILT-IN PINYIN INPUT.")
o.append("//")
o.append("// THIS FILE IS GENERATED. It arises with tools/fui/pywords_table.py from")
o.append("// tools/fui/pywords.txt; whoever wants other words changes that list and not")
o.append("// this file. Every entry was checked against lib/fui/pinyin.fi: one syllable")
o.append("// per character, each character listed under its syllable.")
o.append("//")
o.append("// WHAT IT CONTAINS: %d words of two or more characters, each as \"key:word\"," % total)
o.append("// the key being the pinyin without tones (v for u-umlaut, as in pinyin.fi).")
o.append("// An apostrophe stands in the key exactly where the greedy syllable split of")
o.append("// the input would be wrong without it (xi'an, not xian). One line per initial")
o.append("// letter, most frequent first; the same key may appear several times.")
o.append("//")
o.append("// WHY `static`: as in pinyin.fi, once in .rodata, nothing allocated on lookup.")
o.append("")
o.append("export {")
o.append("    pw_block, PW_WORDS,")
o.append("}")
o.append("")
o.append("// The number that tools/fui/ime_main.fi holds against the list.")
o.append("const PW_WORDS: usize = %d" % total)
o.append("")
for c in sorted(blocks):
    txt = " ".join(blocks[c]) + " "
    o.append('static PW_%s: [u8; %d] = "%s"' % (c.upper(), len(txt.encode()), txt))
o.append("")
o.append("// THE BLOCK OF AN INITIAL LETTER. Returned is the pointer to the line and in")
o.append("// `*n` its length; 0 means: no word begins with this letter.")
o.append("fn pw_block(b: u8, n: *mut usize) -> u64 {")
for c in sorted(blocks):
    ln = len((" ".join(blocks[c]) + " ").encode())
    o.append("    if b == %d as u8 {" % ord(c))
    o.append("        *n = %d" % ln)
    o.append("        return (&PW_%s[0]) as u64" % c.upper())
    o.append("    }")
o.append("    *n = 0")
o.append("    return 0")
o.append("}")
sys.stdout.write("\n".join(o) + "\n")
sys.exit(1 if bad else 0)
