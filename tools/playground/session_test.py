import json
import os
import subprocess
import time

from sseclient import Stream
from testserver import call

PROMPT_SOURCE = (
    'fn main() {\n    print("Name: ")\n    let name = io.read_line()\n    print("Age: ")\n'
    '    let age = io.read_line()\n    println(f"{name} is {age}")\n}\n'
)
EOF_SOURCE = (
    'fn main() {\n    println("ready")\n    var b: rt.Buf = rt.buf_new()\n    io.read_stdin(&b)\n'
    '    println(f"got {b.len}")\n}\n'
)
SPIN_SOURCE = 'fn main() {\n    println("spinning")\n    while true {\n        time.sleep_ms(50)\n    }\n}\n'
TICK_SOURCE = 'fn main() {\n    while true {\n        print(".")\n        time.sleep_ms(200)\n    }\n}\n'
FLOOD_SOURCE = 'fn main() {\n    while true {\n        println("flooding the terminal with text")\n    }\n}\n'
WAIT_SOURCE = 'fn main() {\n    println("waiting")\n    let line = io.read_line()\n    println(line)\n}\n'


def open_session(port, source):
    status, body = call(port, "/api/session", {"source": source})
    return status, json.loads(body)


def start(port, source):
    status, reply = open_session(port, source)
    if status != 200:
        return None, reply
    return reply["id"], Stream(port, reply["id"])


def send(port, session_id, text="", eof=False):
    status, body = call(port, f"/api/session/{session_id}/stdin", {"data": text, "eof": eof})
    return status, json.loads(body)


def stop(port, session_id):
    return call(port, f"/api/session/{session_id}/stop", {})[0]


def leftovers():
    names = [name for name in os.listdir("/tmp") if name.startswith("firn-playground-")]
    running = subprocess.run(["pgrep", "-f", "/work/program"], capture_output=True, text=True).stdout.split()
    return names, running


def test_interactive(port, check):
    session_id, stream = start(port, PROMPT_SOURCE)
    check("session created", session_id is not None, stream)
    check("prompt visible before any input", stream.wait_text("Name: ", 20), stream.names())
    check("prompt has no newline", stream.text() == "Name: ", repr(stream.text()))
    check("stdin accepted", send(port, session_id, "Ada\n")[0] == 200)
    check("second prompt visible", stream.wait_text("Age: "), repr(stream.text()))
    send(port, session_id, "36\n")
    terminal = stream.wait_terminal()
    check("interactive run output", stream.text() == "Name: Age: Ada is 36\n", repr(stream.text()))
    check("interactive run exits 0", terminal and terminal[0] == "exit" and terminal[1]["code"] == 0 and terminal[1]["reason"] == "exit", terminal)
    check("event order", stream.names()[:2] == ["status", "started"], stream.names())
    status, reply = send(port, session_id, "late\n")
    check("stdin after exit is 409", status == 409, reply)
    stream.close()


def test_eof(port, check):
    session_id, stream = start(port, EOF_SOURCE)
    stream.wait_text("ready", 20)
    send(port, session_id, "abc\n")
    send(port, session_id, eof=True)
    terminal = stream.wait_terminal()
    check("eof ends the read", stream.text() == "ready\ngot 4\n" and terminal[1]["code"] == 0, (stream.text(), terminal))
    stream.close()


def test_stop(port, check):
    session_id, stream = start(port, SPIN_SOURCE)
    stream.wait_text("spinning", 20)
    started = time.monotonic()
    check("stop accepted", stop(port, session_id) == 200)
    terminal = stream.wait_terminal(5)
    check("stop ends the program", terminal and terminal[1]["reason"] == "stopped" and time.monotonic() - started < 3, terminal)
    stream.close()


def test_idle(port, check):
    session_id, stream = start(port, WAIT_SOURCE)
    stream.wait_text("waiting", 20)
    started = time.monotonic()
    terminal = stream.wait_terminal(15)
    check("idle timeout", terminal and terminal[1]["reason"] == "idle" and 2 < time.monotonic() - started < 8, terminal)
    stream.close()


def test_total_timeout(port, check):
    session_id, stream = start(port, TICK_SOURCE)
    terminal = stream.wait_terminal(30)
    check("total timeout despite activity", terminal and terminal[1]["reason"] == "timeout", terminal)
    check("output arrived live", len(stream.text()) > 5, stream.text())
    stream.close()


def test_flood(port, check):
    session_id, stream = start(port, FLOOD_SOURCE)
    terminal = stream.wait_terminal(20)
    size = len(stream.text().encode())
    check("flood cut at the output limit", terminal and terminal[1]["reason"] == "output_limit" and size <= 64 * 1024, (terminal, size))
    stream.close()


def test_disconnect(port, check):
    session_id, stream = start(port, WAIT_SOURCE)
    stream.wait_text("waiting", 20)
    stream.close()
    time.sleep(4)
    replay = Stream(port, session_id)
    terminal = replay.wait_terminal(5)
    check("disconnect ends the program", terminal and terminal[1]["reason"] == "disconnected", terminal)
    check("replay delivers earlier output", replay.text() == "waiting\n", replay.text())
    replay.close()
    names, running = leftovers()
    check("disconnect leaves no directory or process", not names and not running, (names, running))


def test_compile_error(port, check):
    session_id, stream = start(port, 'fn main() {\n    let x: int = "a"\n}\n')
    terminal = stream.wait_terminal(30)
    check("compile error arrives as failed", terminal and terminal[0] == "failed" and terminal[1]["diagnostics"][0]["line"] == 2, terminal)
    stream.close()


def test_limits(port, check):
    first = start(port, WAIT_SOURCE)
    second = start(port, WAIT_SOURCE)
    status, reply = open_session(port, WAIT_SOURCE)
    check("third session of one client is refused", status == 503, (status, reply))
    check("stdin limit", send(port, first[0], "x" * 9000)[0] == 413)
    for session_id, stream in (first, second):
        stop(port, session_id)
        stream.wait_terminal(5)
        stream.close()
    check("unknown session is 404", call(port, "/api/session/aaaaaaaaaaaaaaaaaaaaaa/stdin", {"data": "x"})[0] == 404)
    check("bad json is 400", call(port, "/api/session", raw=b"nope")[0] == 400)
    check("empty source is 400", open_session(port, "  ")[0] == 400)
    time.sleep(1)
    names, running = leftovers()
    check("sessions leave no directory or process", not names and not running, (names, running))


def run_all(port, check):
    for step in (test_interactive, test_eof, test_stop, test_idle, test_flood, test_compile_error, test_disconnect, test_limits, test_total_timeout):
        step(port, check)
