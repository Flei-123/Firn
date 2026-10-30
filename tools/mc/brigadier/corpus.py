#!/usr/bin/env python3
import random
import sys

SOURCES = [0, 1, 2, 3, 4, 5, 6, 7, -1, 100]
EMOJI = "\U0001F600"
MATH = "\U0001D11E"

VALID = [
    "say", "say hi", "say hello world", "say   spaced  out", "say hello ", "say  ",
    "say héllo wörld", "say " + EMOJI, "say " + EMOJI + EMOJI + " x", "say " + MATH + "y", "say é中文",
    "tp 5", "tp 5 6", "tp 5 6 7", "tp -100", "tp 100", "tp -101", "tp 101", "tp 5 6 0", "tp 5 6 10", "tp 5 6 11", "tp 5 6 -1",
    "tp 2147483647", "tp -2147483648", "tp 2147483648", "tp x", "tp bob", "tp bob here", "tp bob here extra",
    "tp 5 -2147483648 3", "tp 5 2147483647 10", "tp 5 99999999999 1",
    "give bob", "give bob stone", "give bob stone 5", "give bob stone 64", "give bob stone 65", "give bob stone 0",
    "give bob \"two words\"", "give bob \"two words\" 3", "give bob 'single quoted' 3", "give bob \"esc \\\" quote\" 3",
    "give bob \"back \\\\ slash\" 3", "give bob \"bad \\n escape\" 3", "give bob \"unterminated", "give bob \"unterminated 3",
    "give bob 'mixed\" 3", "give bob \"\" 3", "give bob '' 3", "give bob a.b-c_d+e 3", "give bob é 3", "give bob \"" + EMOJI + "\" 3",
    "give bob \"abc\"def", "give bob \"abc\" def", "give a", "give al", "give alp", "give x", "give xy", "give X", "give Xz", "give ",
    "give bob a", "give bob al", "give bob alpha", "give bob ALPHA", "give bob b", "give bob B", "give bob alp", "give bob \"al",
    "give bob stone 1", "give bob stone 10", "give bob stone 6", "give bob stone 64 ", "give bob stone ",
    "flag true", "flag false", "flag maybe", "flag TRUE", "flag True", "flag off", "flag tr", "flag fa", "flag t", "flag f", "flag", "flag ",
    "scale 0.5", "scale 100.0", "scale 100", "scale 0.4999", "scale 100.0001", "scale 1e1", "scale 1E1", "scale 1e", "scale 1.", "scale .5",
    "scale -.5", "scale NaN", "scale Infinity", "scale 1_0", "scale +5", "scale 0x10", "scale 5f", "scale 5d", "scale 1.5 0.5", "scale 1.5 -1.5",
    "scale 1.5 1.5", "scale 1.5 1.6", "scale 1.5 -1.6", "scale 3.4028235E38", "scale 1.0E-45", "scale 99.99999999",
    "scale 1.5 1e-320", "scale 1.5 4.9E-324", "scale 1.5 1e0", "scale 1.5 0.1", "scale 1.5 1.4999999999999999", "scale 50 -1.5000000000000002",
    "big 0", "big 9223372036854775807", "big -9223372036854775808", "big 9223372036854775808", "big 1.5", "big abc", "big 12abc", "big -",
    "big 5000000000", "big -5000000000", "big 5000000001",
    "hidden", "hidden sub", "hidden open", "hidden open x",
    "alias foo", "alias foobar", "alias Foo", "alias fo", "alias fo x", "alias f", "alias foo x", "alias FOO", "alias bar", "alias fo ",
    "opt", "opt 1", "opt 1 2", "opt z", "opt z true", "opt z false", "opt z x", "opt x", "opt 1 2 3",
    "wide a b c", "wide a b", "wide a", "wide a b c d", "wide", "wide a  b c",
    "mix 5", "mix abc", "mix lit", "mix 5x", "mix -5", "mix 2147483648",
    "exec", "exec show", "exec boom", "exec as 0 show", "exec as 1 show", "exec as 2 show", "exec as 3 show", "exec as 4 show",
    "exec as 2 as 2 show", "exec as 2 as 3 show", "exec as 3 as 3 as 3 show", "exec at 5 show", "exec at 5 at 6 show", "exec at -5 show",
    "exec at 5 as 2 show", "exec as 2 at 1 show", "exec if 5 show", "exec if x show", "exec run say hi", "exec run tp 5 6 7", "exec run exec show",
    "exec as 2 run say hi", "exec as 2 run tp 5", "exec at 1 run say hello world", "exec bad", "exec bad show", "exec fk show", "exec fk fk show",
    "exec as 2 fk show", "exec at 1 fk show", "exec at 2 fk show", "exec nothing show", "exec as 2 nothing show", "exec plain show", "exec plain plain show",
    "exec forkbad show", "exec many show", "exec many many show", "exec many say hi", "exec many many many show", "exec many run say hi",
    "exec as 2 many show", "exec many as 2 show", "exec as 2 forkbad show", "exec at 3 bad show", "exec bad bad show", "exec boom boom",
    "exec as 2 boom", "exec many boom", "exec fk boom", "exec as 3 fk boom", "exec as 2 at 1 fk say hi", "exec as 2 at 2 plain show",
    "exec as 2 run exec as 2 run say hi", "exec run run", "exec run", "exec as", "exec as x", "exec as 5 show", "exec at", "exec at x", "exec at 5",
    "exec if", "exec if 1", "exec if 2", "exec show extra", "exec show ", "exec  show", "exec as 2  show",
    "say hello", "say hello there", "say hello x y", "boom", "boom x", "src", "src x", "anyroot", "anyroot say hi", "anyroot tp 5", "anyroot anyroot say hi",
    "", " ", "  ", "x", "xyz", "say", "SAY hi", "Say hi", "tp  5", " tp 5", "tp 5 ", "tp\t5", "tp 5\t6",
    EMOJI, MATH, "é", "İ", "i̇", "ẞ", "ß", "SAY É",
    "pick", "pick ", "pick a", "pick A", "pick al", "pick x", "pick x1", "pick X", "pick z", "pick e", "pick i", "pick I", "pick \u0130", "pick i\u0307", "pick \u0391\u03a3", "pick \u03b1", "pick \u0391", "pick \u03a3",
    "pick Alpha", "pick alpha", "pick \u0130s", "pick is", "pick Is", "pick ome", "pick OME", "pick ph", "pick a\u00e9", "pick " + EMOJI,
    "pick \u0391\u03a3", "pick \u0391\u03a3\u0391", "pick \u0391\u03a3.", "pick \u03a3", "pick \u0391\u03a3'\u0391", "pick \u0391\u03a3 ", "pick \u0391.\u03a3", "pick \u0391\u03a3.\u0391", "pick 1\u03a3", "pick \u0391\u03a3\u03a3",
    "pick \U00010400", "pick \U00010428", "pick \U00010400a", "pick \u00b5", "pick \u039c", "pick \u00ff", "pick \u0178", "pick \u0130\u0130", "pick I\u0307", "pick \u0131",
    "num", "num ", "num 1", "num 7", "num 14", "num 2", "num -", "num -5", "num 1 ", "num 10", "num 100", "num 9", "num 0", "num x", "num 5x",
    "give \u0130", "give bob \u0130", "give \u0391\u03a3", "give X", "give x", "give XY", "give xY",
    "hid", "hidden s", "hidden o", "e", "ex", "exe", "exec a", "exec as 2 a", "t", "tp b", "g", "gi", "give b", "give bob s", "give bob stone 6",
    "s", "sc", "bi", "al", "ali", "alias f", "alias fo", "alias foo", "alias fooba", "alias Fo",
]

ALPHABET = list(" \"'\\-.e+0123456789abxzXAtrufalse|") + [EMOJI, "é", "中", MATH]

USAGE_PATHS = [
    "", "say", "say msg", "tp", "tp x", "tp x y", "tp x y z", "tp target", "tp target here", "give", "give player", "give player item", "give player item count",
    "flag", "flag on", "flag off", "scale", "scale f", "scale f d", "big", "big l", "big lb", "hidden", "hidden sub", "hidden open", "alias", "alias foo",
    "alias fo", "alias fo rest", "alias any", "opt", "opt a", "opt a b", "opt z", "opt z c", "wide", "wide a1", "wide a1 a2", "wide a1 a2 a3", "mix",
    "mix n", "mix w", "mix lit", "exec", "exec as", "exec as who", "exec at", "exec at n", "exec if", "exec if c", "exec run", "exec bad", "exec fk",
    "exec nothing", "exec plain", "exec forkbad", "exec show", "exec boom", "exec many", "boom", "src", "anyroot", "nope", "say nope", "tp x nope",
    "exec as who nope", "alias Foo", "alias foobar", " ", "say  ", "say msg ", "exec  as", "pick", "pick w", "num", "num n",
]


def mutate(rng, line):
    src, _, text = line.partition("|")
    chars = list(text)
    ops = rng.randint(1, 3)
    for _ in range(ops):
        kind = rng.randint(0, 5)
        if kind == 0 and chars:
            del chars[rng.randrange(len(chars))]
        elif kind == 1:
            chars.insert(rng.randint(0, len(chars)), rng.choice(ALPHABET))
        elif kind == 2 and chars:
            chars[rng.randrange(len(chars))] = rng.choice(ALPHABET)
        elif kind == 3 and len(chars) > 1:
            i = rng.randrange(len(chars) - 1)
            chars[i], chars[i + 1] = chars[i + 1], chars[i]
        elif kind == 4 and chars:
            i = rng.randrange(len(chars))
            chars.insert(i, chars[i])
        elif kind == 5 and chars:
            del chars[rng.randrange(len(chars)):]
    return src + "|" + "".join(chars)


def main():
    out = sys.argv[1]
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else 1
    rng = random.Random(seed)
    lines = []
    seen = set()

    def add(line):
        if "\n" in line or "\r" in line or line in seen:
            return
        seen.add(line)
        lines.append(line)

    for text in VALID:
        add("0|" + text)
        add("%d|%s" % (rng.choice(SOURCES), text))
    for text in VALID:
        for src in (1, 2, 3, 5, 6):
            if rng.random() < 0.45:
                add("%d|%s" % (src, text))
    base = list(lines)
    guard = 0
    while len(lines) < len(base) + 900 and guard < 20000:
        guard += 1
        add(mutate(rng, rng.choice(base)))
    for src in SOURCES:
        for path in USAGE_PATHS:
            if src in (0, 1, 3, 6) or rng.random() < 0.3:
                add("@usage %d %s" % (src, path) if path else "@usage %d" % src)
    for _ in range(120):
        words = [rng.choice(["exec", "as", "at", "if", "run", "show", "fk", "many", "plain", "nothing", "bad", "1", "2", "3", "say", "hi", "tp", "5"]) for _ in range(rng.randint(2, 7))]
        add("%d|%s" % (rng.choice(SOURCES), " ".join(words)))
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        for line in lines:
            f.write(line + "\n")
    print(len(lines), file=sys.stderr)


main()
