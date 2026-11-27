import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.realpath(__file__))
RUN_TIMEOUT_S = 3
FAILURES = []


def free_port():
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def call(port, path, payload=None, raw=None):
    data = raw if raw is not None else (json.dumps(payload).encode() if payload is not None else None)
    try:
        with urllib.request.urlopen(urllib.request.Request(f"http://127.0.0.1:{port}{path}", data=data), timeout=60) as reply:
            return reply.status, reply.read()
    except urllib.error.HTTPError as failure:
        return failure.code, failure.read()


def run(port, source):
    status, body = call(port, "/api/run", {"source": source})
    return status, json.loads(body)


def check(name, condition, detail=""):
    print(("PASS " if condition else "FAIL ") + name + ("" if condition else f"  {detail}"))
    if not condition:
        FAILURES.append(name)


def start_server(port, runtime_dir):
    env = dict(os.environ, PLAYGROUND_PORT=str(port), PLAYGROUND_RUN_TIMEOUT=str(RUN_TIMEOUT_S), PLAYGROUND_RUNTIME=runtime_dir)
    process = subprocess.Popen([sys.executable, os.path.join(HERE, "server.py")], env=env, cwd=HERE, stdout=subprocess.PIPE)
    process.stdout.readline()
    return process


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


def test_static(port):
    status, body = call(port, "/")
    check("index served", status == 200 and b"Firn Playground" in body)
    check("script served", call(port, "/app.js")[0] == 200)
    check("traversal blocked", call(port, "/../server.py")[0] == 404 and call(port, "/..%2fserver.py")[0] == 404)
    check("unknown path 404", call(port, "/nope")[0] == 404)


def main():
    port = free_port()
    runtime_dir = tempfile.mkdtemp(prefix="firn-playground-test-", dir="/var/tmp")
    server = start_server(port, runtime_dir)
    try:
        for step in (test_basics, test_examples, test_static, test_isolation, test_limits):
            step(port)
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
