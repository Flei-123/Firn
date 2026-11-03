import json
import sys

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
    rules = order(rules)
    json.dump(rules, open(out_json, "w"), indent=1)
    open(rules_rs, "w").write(emit_rules(rules))
    open(tests_rs, "w").write(emit_tests(rules))
    print("rules", len(rules))


if __name__ == "__main__":
    main()
