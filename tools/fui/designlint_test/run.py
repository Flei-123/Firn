#!/usr/bin/env python3
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
TOOL = os.path.join(os.path.dirname(HERE), "designlint.py")
sys.path.insert(0, os.path.dirname(HERE))

from designlint_lib import allow, engine


def lint(rel):
    path = os.path.join(HERE, rel)
    with open(path, encoding="utf-8") as fh:
        return engine.lint_source(rel, fh.read())


def triples(found):
    return [[f["line"], f["rule"], f["text"]] for f in found]


def run_cli(args):
    p = subprocess.run([sys.executable, "-I", TOOL] + args, capture_output=True, text=True)
    return p.returncode, p.stdout


def main():
    with open(os.path.join(HERE, "expected.json"), encoding="utf-8") as fh:
        expected = json.load(fh)
    failures = []
    checks = 0

    for rel in sorted(expected):
        checks += 1
        got = triples(lint(rel))
        if got != expected[rel]:
            failures.append("%s: expected %s got %s" % (rel, expected[rel], got))

    for name in sorted(os.listdir(os.path.join(HERE, "good"))):
        checks += 1
        got = lint("good/" + name)
        if got:
            failures.append("good/%s: unexpected %s" % (name, triples(got)))

    checks += 1
    code, _ = run_cli(["--no-allow", os.path.join(HERE, "good")])
    if code != 0:
        failures.append("cli: good dir must exit 0")

    checks += 1
    code, out = run_cli(["--no-allow", os.path.join(HERE, "bad")])
    if code != 1 or "NAV_TEXT_BUTTON" not in out:
        failures.append("cli: bad dir must exit 1")

    checks += 1
    code, out = run_cli(["--no-allow", "--json", os.path.join(HERE, "bad")])
    data = json.loads(out)
    total = sum(len(v) for v in expected.values())
    if code != 1 or len(data["findings"]) != total or sum(data["counts"].values()) != total:
        failures.append("cli: json shape or count mismatch")

    checks += 1
    allow_file = os.path.join(HERE, "allow_fixture.txt")
    code, out = run_cli(["--allow", allow_file, "--json", os.path.join(HERE, "bad", "nav.fi")])
    data = json.loads(out)
    texts = [f["text"] for f in data["findings"]]
    if texts != ["Zurueck", "←", "Up", "back"] or data["allowed"] != 4:
        failures.append("cli: allowlist did not filter exactly the listed findings: %s" % texts)

    checks += 1
    code, out = run_cli(["--no-allow", "--rules", "PAGING", os.path.join(HERE, "bad")])
    if code != 1 or any(line.split(": ")[1:2] not in ([], ["PAGING"]) for line in out.splitlines() if ".fi:" in line):
        failures.append("cli: --rules filter")

    for app, want in (("bad_app", 1), ("good_app", 0), ("core_app", 0), ("set_app", 0), ("stale_set", 1)):
        checks += 1
        code, out = run_cli(["--no-allow", os.path.join(HERE, "icon_install", app)])
        if code != want or (want == 1 and "ICON_NOT_INSTALLED" not in out and "ICON_SET_STALE" not in out):
            failures.append("icon_install/%s: exit %d, output %s" % (app, code, out.strip()))

    for f in failures:
        print("FAIL: " + f)
    print("designlint tests: %d checks, %d failed" % (checks, len(failures)))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
