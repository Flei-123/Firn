#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dns/idn_check.py <idn_main> -- lib/net/idn.fi against Python.

  1. the RFC 3492 sample strings (A-S), via Python's own `punycode` codec
  2. random labels (Latin-1/Ext-A, Greek, Cyrillic, CJK, kana, Hangul, emoji)
     against codecs 'punycode' (exact) and, where nameprep is the identity
     on lower case, against `str.encode("idna")`
  3. upper case in the supported ranges against `encode("idna")`
  4. decoding: random xn-- labels against `bytes.decode("idna")`
  5. refusals: bad UTF-8, empty labels, over-long labels, bad punycode
"""
import random, subprocess, sys, unicodedata

EXE = sys.argv[1]
fails = 0
total = 0

def run(mode, names):
    r = subprocess.run([EXE, mode] + names, capture_output=True, timeout=60)
    assert r.returncode == 0, r.returncode
    return r.stdout.decode("utf-8", "surrogateescape").split("\n")[:-1]

def raw_run(mode, raws):
    r = subprocess.run([EXE.encode(), mode.encode()] + raws, capture_output=True, timeout=60)
    return r.stdout.decode("utf-8", "replace").split("\n")[:-1]

def check(label, got, want):
    global fails, total
    total += 1
    if got != want:
        fails += 1
        if fails <= 15:
            print("  FAIL", label, "got", repr(got), "want", repr(want))

# 1. RFC 3492 section 7.1 samples (the encoded forms, without xn--)
RFC = [
    ("ليهمابتكلموشعربي؟", "egbpdaj6bu4bxfgehfvwxn"),
    ("他们为什么不说中文", "ihqwcrb4cv8a8dqg056pqjye"),
    ("他們爲什麽不說中文", "ihqwctvzc91f659drss3x8bo0yb"),
    ("Pročprostěnemluvíčesky", "Proprostnemluvesky-uyb24dma41a"),
    ("למההםפשוטלאמדבריםעברית", "4dbcagdahymbxekheh6e0a7fei0b"),
    ("यहलोगहिन्दीक्योंनहींबोलसकतेहैं", "i1baa7eci9glrd9b2ae1bj0hfcgg6iyaf8o0a1dig0cd"),
    ("なぜみんな日本語を話してくれないのか", "n8jok5ay5dzabd5bym9f0cm5685rrjetr6pdxa"),
    ("세계의모든사람들이한국어를이해한다면얼마나좋을까", "989aomsvi5e83db1d2a355cv1e0vak1dwrv93d5xbh15a0dt30a5jpsd879ccm6fea98c"),
    ("почемужеонинеговорятпорусски", "b1abfaaepdrnnbgefbadotcwatmq2g4l"),
    ("PorquénopuedensimplementehablarenEspañol", "PorqunopuedensimplementehablarenEspaol-fmd56a"),
    ("TạisaohọkhôngthểchỉnóitiếngViệt", "TisaohkhngthchnitingVit-kjcr8268qyxafd2f1b9g"),
    ("3年B組金八先生", "3B-ww4c5e180e575a65lsy2b"),
    ("安室奈美恵-with-SUPER-MONKEYS", "-with-SUPER-MONKEYS-pc58ag80a8qai00g7n9n"),
    ("Hello-Another-Way-それぞれの場所", "Hello-Another-Way--fc4qua05auwb3674vfr0b"),
    ("ひとつ屋根の下2", "2-u9tlzr9756bt3uc0v"),
    ("MajiでKoiする5秒前", "MajiKoi5-783gue6qz075azm5e"),
    ("パフィーdeルンバ", "de-jg4avhby1noc0d"),
    ("そのスピードで", "d9juau41awczczp"),
]
for uni, enc in RFC:
    # Python's codec is the reference for the mixed-case ones too; our
    # lib lower-cases ASCII, so compare lower-cased input to the codec.
    lo = uni.lower() if all(ord(c) < 128 or c.lower() == c for c in uni) else uni
    want = "xn--" + lo.encode("punycode").decode()
    if all(ord(c) < 128 for c in lo):
        want = lo
    if any(c.isupper() and ord(c) < 128 for c in uni) and uni != lo:
        pass
    got = run("a", [uni])[0]
    if len(want) > 63:
        check("rfc long " + enc[:12], got, "ERR")  # over the 63 octet label limit
        continue
    check("rfc " + enc[:12], got, want)
    back = run("u", [got])[0]
    check("rfc back " + enc[:12], back, lo)

# 2. random labels vs the punycode codec and idna
random.seed(20261013)
RANGES = [(0xE0, 0xFF), (0x100, 0x137), (0x3B1, 0x3C9), (0x430, 0x44F),
          (0x4E00, 0x4F00), (0x3042, 0x3093), (0xAC00, 0xAC80), (0x1F600, 0x1F64F),
          (0x61, 0x7A), (0x30, 0x39)]
def rand_label():
    n = random.randint(1, 20)
    s = ""
    for _ in range(n):
        lo, hi = random.choice(RANGES)
        s += chr(random.randint(lo, hi))
    return s

cases = []
for _ in range(1500):
    s = rand_label()
    if s != s.lower() or unicodedata.normalize("NFKC", s) != s:
        continue
    cases.append(s)
for chunk in range(0, len(cases), 100):
    part = cases[chunk:chunk + 100]
    names = [a + "." + b for a, b in zip(part, part[::-1])]
    got = run("a", names)
    for nm, g in zip(names, got):
        try:
            want = nm.encode("idna").decode("ascii")
        except Exception:
            continue  # nameprep prohibits something (e.g. emoji): not promised
        check("idna " + nm, g, want)
        check("round " + nm, run("u", [g])[0], want.encode("ascii").decode("idna"))
    # exact punycode codec for every label (emoji included)
    for lab in part:
        if "\u03c2" in lab:
            continue  # final sigma is folded by the library (as nameprep does)
        g = run("a", [lab])[0]
        if all(ord(c) < 128 for c in lab):
            want = lab
        else:
            want = "xn--" + lab.encode("punycode").decode()
        check("puny " + lab, g, want)

# 3. upper case inside the supported ranges
upp = []
for _ in range(400):
    s = ""
    for _ in range(random.randint(1, 8)):
        lo, hi = random.choice([(0xC0, 0xDE), (0x100, 0x17E), (0x391, 0x3A9), (0x410, 0x42F), (0x400, 0x40F), (0x41, 0x5A)])
        c = chr(random.randint(lo, hi))
        if c in "\u00d7\u0130\u0131\u0132\u0133\u013f\u0140\u0138\u0149\u03a2" or not c.isalpha():
            continue
        s += c
    if s:
        upp.append(s)
got = run("a", upp)
for s, g in zip(upp, got):
    try:
        want = s.encode("idna").decode("ascii")
    except Exception:
        continue
    if all(ord(c) < 128 for c in s):
        want = want.lower()  # Python leaves pure ASCII labels alone, we lower-case them
    check("upper " + s, g, want)

# 4. decoding of hand-made xn-- labels vs Python
for s in cases[:300]:
    if "\u03c2" in s or all(ord(c) < 128 for c in s):
        continue
    enc = s.encode("idna").decode("ascii") if False else "xn--" + s.encode("punycode").decode()
    try:
        want = enc.encode("ascii").decode("idna")
    except Exception:
        continue
    check("dec " + enc, run("u", [enc])[0], want)

# 5. refusals
bad = [b"\xff\xfe.example", b"a..b", b".a", b"\xc0\x80.x", b"\xed\xa0\x80.x",
       b"a" * 64, (b"\xc3\xa9" * 80) + b".x", b"\xf5\x80\x80\x80"]
for b in bad:
    out = raw_run("a", [b])
    check("refuse " + repr(b[:12]), out[0] if out else None, "ERR")
for b in ["xn--", "xn--!!", "xn--99999999999999999", "xn--ab-ü"]:
    out = raw_run("u", [b.encode()])
    if b == "xn--":
        check("lone prefix", out[0], "xn--")  # a bare prefix is a plain label
    else:
        check("bad puny " + b, out[0], "ERR")
# trailing dot and case
check("root dot", run("a", ["Bücher.Example."])[0], "xn--bcher-kva.example.")
check("ascii lower", run("a", ["WWW.Example.COM"])[0], "www.example.com")

print("IDN %d checks, %d fail" % (total, fails))
sys.exit(1 if fails else 0)
