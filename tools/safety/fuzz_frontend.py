#!/usr/bin/env python3
import argparse
import concurrent.futures
import os
import random
import subprocess
import sys
import tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
TOKENS = [
    "(", ")", "{", "}", "[", "]", ";", ",", ":", "::", "->", "=>", "..", "&", "*", "as", "fn", "struct",
    "enum", "match", "if", "else", "while", "for", "in", "return", "let", "var", "import", "gc", "defer",
    "#[", "0", "-1", "18446744073709551616", "\"", "'", "/*", "//", "\\", "\0", "<", ">", "!", "?",
]
CRASH_CODE = 128
TIMEOUT_SECONDS = 20
MAX_SOURCE_BYTES = 6000
UNSUPPORTED_C1 = (3, 4, 5, 6)
NO_MAIN = b"has no function 'main'"


def mutate(text, rng):
    lines = text.split("\n")
    kind = rng.randrange(7)
    if kind == 0 and len(text) > 1:
        a = rng.randrange(len(text))
        b = min(len(text), a + rng.randrange(1, 40))
        return text[:a] + text[b:]
    if kind == 1:
        i = rng.randrange(len(lines))
        lines.insert(i, lines[rng.randrange(len(lines))])
        return "\n".join(lines)
    if kind == 2:
        a = rng.randrange(len(text) + 1)
        return text[:a] + " " + rng.choice(TOKENS) + " " + text[a:]
    if kind == 3:
        return text[: rng.randrange(len(text) + 1)]
    if kind == 4 and len(lines) > 1:
        i, j = rng.randrange(len(lines)), rng.randrange(len(lines))
        lines[i], lines[j] = lines[j], lines[i]
        return "\n".join(lines)
    if kind == 5:
        i = rng.randrange(len(lines))
        lines[i] = lines[i].replace(rng.choice(["(", ")", "{", "}", ":", ","]), "", 1)
        return "\n".join(lines)
    a = rng.randrange(len(text) + 1)
    return text[:a] + chr(rng.randrange(1, 128)) + text[a:]


def run(cmd, env):
    try:
        p = subprocess.run(cmd, capture_output=True, timeout=TIMEOUT_SECONDS, env=env)
    except subprocess.TimeoutExpired:
        return 124, b""
    rc = p.returncode
    if rc < 0:
        rc = CRASH_CODE - rc
    return rc, p.stderr


def crashed(rc, err):
    return rc >= CRASH_CODE or rc == 124 or b"panicked at" in err or b"RUST_BACKTRACE" in err


def check(task):
    idx, text, firnc0, firnc1, env = task
    with tempfile.TemporaryDirectory() as d:
        src = os.path.join(d, "m.fi")
        with open(src, "w", encoding="latin-1") as f:
            f.write(text)
        rc0, err0 = run([firnc0, "--emit=asm", "-o", os.path.join(d, "out.s"), src], env)
        rc1, err1 = run([firnc1, src], env) if firnc1 else (0, b"")
    verdict = "ok"
    if crashed(rc0, err0):
        verdict = "crash0"
    elif firnc1 and crashed(rc1, err1):
        verdict = "crash1"
    elif firnc1 and rc1 not in UNSUPPORTED_C1 and (rc0 == 0) != (rc1 == 0) and NO_MAIN not in err0:
        verdict = "disagree"
    return idx, verdict, rc0, rc1, text


def corpus(seed_count, rng):
    names = sorted(
        n for n in os.listdir(os.path.join(ROOT, "tests"))
        if n.endswith(".fi") and os.path.getsize(os.path.join(ROOT, "tests", n)) <= MAX_SOURCE_BYTES
    )
    rng.shuffle(names)
    out = []
    for n in names[:seed_count]:
        with open(os.path.join(ROOT, "tests", n), encoding="latin-1") as f:
            out.append((n, f.read()))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=2000)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--jobs", type=int, default=3)
    ap.add_argument("--firnc0", default=os.environ.get("FIRNC", os.path.join(ROOT, "compiler/target/release/firnc")))
    ap.add_argument("--firnc1", default=os.environ.get("FIRNC1", os.path.join(ROOT, ".firnc1")))
    ap.add_argument("--keep", default="")
    args = ap.parse_args()
    rng = random.Random(args.seed)
    env = dict(os.environ, FIRNLIB=os.path.join(ROOT, "lib"))
    seeds = corpus(max(50, args.runs // 8), rng)
    firnc1 = args.firnc1 if os.path.exists(args.firnc1) else ""
    tasks = []
    for i in range(args.runs):
        name, text = seeds[i % len(seeds)]
        tasks.append((i, mutate(text, rng), args.firnc0, firnc1, env))
    counts = {"ok": 0, "crash0": 0, "crash1": 0, "disagree": 0}
    shown = 0
    with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
        for idx, verdict, rc0, rc1, text in pool.map(check, tasks):
            counts[verdict] += 1
            if verdict != "ok" and shown < 10:
                shown += 1
                print(f"{verdict} run={idx} rc0={rc0} rc1={rc1}")
                if args.keep:
                    os.makedirs(args.keep, exist_ok=True)
                    with open(os.path.join(args.keep, f"{verdict}_{idx}.fi"), "w", encoding="latin-1") as f:
                        f.write(text)
    print(f"fuzz frontend: runs={args.runs} seed={args.seed} firnc1={'yes' if firnc1 else 'no'} "
          f"ok={counts['ok']} crash0={counts['crash0']} crash1={counts['crash1']} disagree={counts['disagree']}")
    return 1 if counts["crash0"] or counts["crash1"] else 0


if __name__ == "__main__":
    sys.exit(main())
