import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

from testserver import call, free_port, start_server

HERE = os.path.dirname(os.path.realpath(__file__))
RUN_TIMEOUT_S = 3
FAILURES = []


def run(port, source):
    status, body = call(port, "/api/run", {"source": source})
    return status, json.loads(body)


def check(name, condition, detail=""):
    print(("PASS " if condition else "FAIL ") + name + ("" if condition else f"  {detail}"))
    if not condition:
        FAILURES.append(name)


def test_basics(port):
    status, result = run(port, 'fn main() {\n    println("Hello, Firn!")\n}\n')
    check("hello runs", status == 200 and result["ok"] and result["stdout"] == "Hello, Firn!\n" and result["exit_code"] == 0, result)
    status, result = run(port, 'fn main() {\n    let x: int = "a"\n}\n')
    located = [item for item in result["diagnostics"] if item["line"] == 2]
    check("compile error with line", result["stage"] == "compile" and not result["ok"] and located, result)
    status, result = run(port, 'fn main() {\n    var d: int = 0\n    println(f"{10 / d}")\n}\n')
    check("panic reported", not result["ok"] and result["exit_code"] != 0 and "division by zero" in result["stderr"], result)


def test_limits(port):
    started = time.monotonic()
    status, result = run(port, 'fn main() {\n    println("start")\n    while true {\n    }\n}\n')
    elapsed = time.monotonic() - started
    check("endless loop is stopped", result["timed_out"] and not result["ok"] and elapsed < RUN_TIMEOUT_S + 8, (elapsed, result))
    check("output before the stop is kept", result["stdout"] == "start\n", result)
    status, result = run(port, 'fn main() {\n    while true {\n        println("spam spam spam spam spam spam")\n    }\n}\n')
    check("output flood is cut", result["truncated"] and len(result["stdout"]) <= 64 * 1024, len(result["stdout"]))
    status, result = run(port, "   ")
    check("empty source rejected", result["stage"] == "input" and not result["ok"], result)
    status, result = run(port, "x" * (70 * 1024))
    check("huge source rejected", result["stage"] == "input", result)
    status, _ = call(port, "/api/run", raw=b"not json")
    check("bad json is 400", status == 400, status)


def test_isolation(port):
    source = (
        'fn main() {\n'
        '    let passwd = fs.read_text("/etc/passwd") catch "blocked"\n'
        '    let secret = fs.read_text("/root/jarvis/.env") catch "blocked"\n'
        '    let wrote = fs.write_text("/usr/evil", "x") catch 99\n'
        '    println(f"{passwd.length()} {secret} {wrote}")\n'
        '}\n'
    )
    status, result = run(port, source)
    check("no files outside the sandbox", result["ok"] and result["stdout"].split()[1:] == ["blocked", "99"], result)
    status, result = run(port, 'fn main() {\n    let r = fs.read_text("/work/../etc/shadow") catch "blocked"\n    println(r)\n}\n')
    check("no path escape", result["stdout"].strip() == "blocked", result)


def test_examples(port):
    status, body = call(port, "/api/examples")
    examples = json.loads(body)
    check("six examples", [item["id"] for item in examples] == ["hello", "vec", "strings", "fstring", "tuple", "timer"], examples)
    for item in examples:
        status, result = run(port, item["source"])
        check(f"example {item['id']} runs", result["ok"] and result["stdout"].strip() != "", result)


def check_source(port, source):
    status, body = call(port, "/api/check", {"source": source})
    return status, json.loads(body)


def test_check_endpoint(port):
    status, result = check_source(port, 'fn main() {\n    println("ok")\n}\n')
    check("check accepts valid source", status == 200 and result["ok"] and result["diagnostics"] == [], result)
    status, result = check_source(port, 'fn main() {\n    let x: int = "a"\n    printn(x)\n}\n')
    found = {(item["line"], item["col"], item["length"]) for item in result["diagnostics"]}
    check("check reports position and length", not result["ok"] and found == {(2, 18, 3), (3, 5, 6)}, result)
    check("check keeps the hint", any("parse it with std.num" in item["hint"] for item in result["diagnostics"]), result)
    status, result = check_source(port, 'fn main() {\n    let a: int = 1\n    println(f"{a +}")\n}\n')
    check("check reports parse error", [(item["line"], item["col"]) for item in result["diagnostics"]] == [(3, 19)], result)
    status, result = check_source(port, "   ")
    check("check rejects empty source", not result["ok"] and result["message"], result)
    status, result = run(port, 'fn main() {\n    let x: int = "a"\n}\n')
    check("run diagnostics carry the length", result["diagnostics"][0]["length"] == 3, result)
    leftovers = [name for name in os.listdir("/tmp") if name.startswith("firn-playground-")]
    check("check leaves no sandbox directory", not leftovers, leftovers)
    status, _ = call(port, "/api/check", raw=b"not json")
    check("check bad json is 400", status == 400, status)


def test_static(port):
    status, body = call(port, "/")
    check("index served", status == 200 and b"Firn Playground" in body)
    check("script served", call(port, "/app.js")[0] == 200)
    for name in ("tokenizer.js", "editing.js", "completion.js", "problems.js", "statusbar.js", "tooltip.js", "metrics.js", "textedit.js", "popup.js", "livecheck.js", "highlight.js"):
        check(f"{name} served", call(port, "/" + name)[0] == 200)
    status, body = call(port, "/completions.json")
    check("completion data served", status == 200 and "modules" in json.loads(body), status)
    check("traversal blocked", call(port, "/../server.py")[0] == 404 and call(port, "/..%2fserver.py")[0] == 404)
    check("unknown path 404", call(port, "/nope")[0] == 404)


def test_staged_symlinks(port, runtime_dir):
    for name in ("rt.fi", "vec.fi", "map.fi"):
        path = os.path.join(runtime_dir, "lib", "std", name)
        check(f"staged std/{name} resolves", os.path.islink(path) and os.path.isfile(os.path.realpath(path)), path)
    source = (
        "import std.vec\n"
        "fn sum(xs: ..int) -> int {\n    var t: int = 0\n    for x in xs { t += x }\n    return t\n}\n"
        "fn main() {\n    var v: Vec[int] = Vec[int].new()\n    v.push(4)\n    v.push(5)\n"
        '    println(f"{sum(1, 2, 3)} {v.len()}")\n}\n'
    )
    status, result = run(port, source)
    check("import std.vec and variadic sum run", result["ok"] and result["stdout"].strip() == "6 2", result)


def test_generated_data():
    result = subprocess.run([sys.executable, "-I", os.path.join(HERE, "gen_completions.py"), "--check"])
    check("completions.json is up to date", result.returncode == 0)


def main():
    test_generated_data()
    port = free_port()
    runtime_dir = tempfile.mkdtemp(prefix="firn-playground-test-", dir="/var/tmp")
    server = start_server(port, runtime_dir, RUN_TIMEOUT_S)
    try:
        for step in (test_basics, test_check_endpoint, test_examples, test_static, test_isolation, test_limits):
            step(port)
        test_staged_symlinks(port, runtime_dir)
    finally:
        server.terminate()
        server.wait()
        shutil.rmtree(runtime_dir, ignore_errors=True)
    leftovers = [name for name in os.listdir("/tmp") if name.startswith("firn-playground-")]
    check("no sandbox leftovers in /tmp", not leftovers, leftovers)
    print(f"{len(FAILURES)} failed")
    return 1 if FAILURES else 0


if __name__ == "__main__":
    sys.exit(main())
