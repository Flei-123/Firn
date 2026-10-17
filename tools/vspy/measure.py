#!/usr/bin/env python3
"""Measure one Python-vs-Firn task pair.

Usage: measure.py <task_dir> [--runs N] [--args-py "..."] [--args-fi "..."]
  <task_dir> holds task.py and task.fi (and optionally run.sh with the shell
  command line for both: it gets $1 = "py" | "fi" and $2 = binary/script).
Prints one TSV line:
  name  py_lines  fi_lines  factor_lines  py_tokens  fi_tokens  factor_tokens
        py_imports  fi_imports  py_run_ms  fi_run_ms  fi_compile_ms
Lines = non-blank, non-comment source lines. Tokens: Python `tokenize` without
NEWLINE/NL/COMMENT/INDENT/DEDENT/ENDMARKER; Firn `firnc --emit=tokens` minus EOF.
Imports: number of `import`/`from` statements.
Run times: median of N runs (default 5), wall clock, Python = `python3 task.py`,
Firn = the built binary (dev-fast, same as firn-run). Compile = one cold build.
"""
import io, os, re, subprocess, sys, tempfile, time, tokenize, statistics

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
FIRNC = os.environ.get("FIRNC", os.path.join(ROOT, "compiler/target/release/firnc"))
if not os.path.exists(FIRNC):
    FIRNC = "/root/firn/compiler/target/release/firnc"
FIRNLIB = os.environ.get("FIRNLIB", os.path.join(ROOT, "lib"))


def count_lines(path, comment):
    n = 0
    in_doc = False
    for ln in open(path, encoding="utf-8"):
        s = ln.strip()
        if not s or s.startswith(comment) or s == "#!/usr/bin/env python3":
            continue
        n += 1
    return n


def py_tokens(path):
    skip = {tokenize.NEWLINE, tokenize.NL, tokenize.COMMENT, tokenize.INDENT,
            tokenize.DEDENT, tokenize.ENDMARKER}
    with open(path, "rb") as f:
        return sum(1 for t in tokenize.tokenize(f.readline) if t.type not in skip and t.type != tokenize.ENCODING)


def fi_tokens(path):
    env = dict(os.environ, FIRNLIB=FIRNLIB)
    out = subprocess.run([FIRNC, "--emit=tokens", path], capture_output=True, text=True, env=env).stdout
    lines = [l for l in out.splitlines() if l.strip() and "Eof" not in l.split()[-1]]
    return len(lines)


def imports(path, kind):
    pat = r"^\s*(import|from)\s" if kind == "py" else r"^\s*(import|use)\s"
    return sum(1 for l in open(path, encoding="utf-8") if re.match(pat, l))


def median_ms(cmd, runs, cwd, env=None):
    ts = []
    for _ in range(runs):
        t = time.perf_counter()
        subprocess.run(cmd, cwd=cwd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                       stdin=subprocess.DEVNULL, env=env)
        ts.append((time.perf_counter() - t) * 1000)
    return statistics.median(ts)


def start_server(d):
    """Start <d>/server.py (the test harness) on a free port; env VSPY_PORT carries it."""
    import socket
    sk = socket.socket()
    sk.bind(("127.0.0.1", 0))
    port = sk.getsockname()[1]
    sk.close()
    env = dict(os.environ, VSPY_PORT=str(port))
    proc = subprocess.Popen(["python3", os.path.join(d, "server.py")], env=env, cwd=d)
    for _ in range(100):
        try:
            socket.create_connection(("127.0.0.1", port), 0.2).close()
            return proc, port
        except OSError:
            time.sleep(0.05)
    proc.kill()
    raise SystemExit("server.py did not come up")


def main():
    d = os.path.abspath(sys.argv[1])
    runs = 5
    if "--runs" in sys.argv:
        runs = int(sys.argv[sys.argv.index("--runs") + 1])
    py, fi = os.path.join(d, "task.py"), os.path.join(d, "task.fi")
    name = os.path.basename(d)
    env = dict(os.environ, FIRNLIB=FIRNLIB)
    tmp = tempfile.mkdtemp(prefix="vspy-")
    exe = os.path.join(tmp, "task")
    t = time.perf_counter()
    flags = open(os.path.join(d, "firnflags")).read().split() if os.path.exists(os.path.join(d, "firnflags")) else []
    r = subprocess.run([FIRNC, "--opt-level=dev-fast", *flags, "-o", exe, fi], capture_output=True, text=True, env=env, cwd=d)
    comp = (time.perf_counter() - t) * 1000
    if r.returncode != 0:
        print(name + "\tCOMPILE-FAIL\t" + r.stderr[:200].replace("\n", " "))
        return 1
    pyl, fil = count_lines(py, "#"), count_lines(fi, "//")
    pyt, fit = py_tokens(py), fi_tokens(fi)
    srv = None
    renv = None
    if os.path.exists(os.path.join(d, "server.py")):
        srv, port = start_server(d)
        renv = dict(os.environ, VSPY_PORT=str(port))
    try:
        if os.path.exists(os.path.join(d, "norun")):  # a window program: no timing
            pyr = fir = -1
        else:
            pyr = median_ms(["python3", py], runs, d, renv)
            fir = median_ms([exe], runs, d, renv)
    finally:
        if srv:
            srv.kill()
    print("\t".join(str(x) for x in [
        name, pyl, fil, round(fil / pyl, 2), pyt, fit, round(fit / pyt, 2),
        imports(py, "py"), imports(fi, "fi"), round(pyr, 1), round(fir, 1), round(comp)]))
    subprocess.run(["rm", "-rf", tmp])
    return 0


sys.exit(main())
