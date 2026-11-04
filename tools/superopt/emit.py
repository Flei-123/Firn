import json
import sys

from x86 import GPR, SUFFIX, VARTOK, normalize

CONCRETE = [0, 1, 2, 6, 7, 8, 9, 10, 11, 12, 13, 14]
WIDTH_INDEX = {"": 0, "d": 1, "w": 2, "b": 3}

HEADER = ""


def rust_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def rust_list(xs):
    return "&[" + ", ".join(rust_str(x) for x in xs) + "]"


def order(rules):
    seen = set()
    out = []
    for r in sorted(rules, key=lambda r: (-r["gain"], bin(r["dead"]).count("1") + int(r["flags_dead"]), -r["count"])):
        k = ("\n".join(r["pattern"]), r["dead"], r["flags_dead"])
        if k in seen:
            continue
        seen.add(k)
        out.append(r)
    return out


def render(texts):
    return ["    " + VARTOK.sub(lambda m: GPR[CONCRETE[int(m.group(1))]][WIDTH_INDEX[m.group(2)]], t) for t in texts]


def covered_by(r, s):
    if s is r or len(s["pattern"]) >= len(r["pattern"]) or s["dead"] or s["flags_dead"]:
        return False
    if len(s["pattern"]) - len(s["replacement"]) < r["gain"]:
        return False
    lines = render(r["pattern"])
    n = len(s["pattern"])
    for a in range(len(lines) - n + 1):
        norm = normalize(lines[a:a + n])
        if norm and norm[0] == s["pattern"]:
            return True
    return False


SLOW = ("imul",)


def slower(r):
    def count(texts):
        return sum(1 for t in texts if t.split()[0] in SLOW)
    return count(r["replacement"]) > count(r["pattern"])


def drop_decomposable(rules):
    return [r for r in rules if not any(covered_by(r, s) for s in rules)]


def emit_rules(rules):
    lines = ["pub(crate) struct Rule {",
             "    pub(crate) pattern: &'static [&'static str],",
             "    pub(crate) replacement: &'static [&'static str],",
             "    pub(crate) dead: u32,",
             "    pub(crate) flags_dead: bool,",
             "    pub(crate) hits: u32,",
             "}", "", "pub(crate) const RULES: &[Rule] = &["]
    for r in rules:
        lines.append("    Rule { pattern: %s, replacement: %s, dead: %d, flags_dead: %s, hits: %d }," % (
            rust_list(r["pattern"]), rust_list(r["replacement"]), r["dead"],
            "true" if r["flags_dead"] else "false", r["count"]))
    lines.append("];")
    return "\n".join(lines) + "\n"


def emit_tests(rules):
    lines = []
    for i in range(len(rules)):
        lines += ["#[test]", "fn rule_%03d() {" % i, "    super::tests::check_rule(%d);" % i, "}", ""]
    return "\n".join(lines)


def main():
    src, rules_rs, tests_rs, out_json = sys.argv[1:5]
    rules = []
    for p in src.split(","):
        rules += json.load(open(p))
    rules = drop_decomposable([r for r in order(rules) if not slower(r)])
    json.dump(rules, open(out_json, "w"), indent=1)
    open(rules_rs, "w").write(emit_rules(rules))
    open(tests_rs, "w").write(emit_tests(rules))
    print("rules", len(rules))


if __name__ == "__main__":
    main()
