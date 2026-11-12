import os
import signal
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

from mi_parser import parse_record
from protocol import Channel

failures = []


def check(name, condition, detail=""):
    print(("ok   " if condition else "FAIL ") + name + ("" if condition else "  " + str(detail)))
    if not condition:
        failures.append(name)


class Client:
    def __init__(self, server_path):
        self.process = subprocess.Popen(
            [sys.executable, "-I", server_path],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
        )
        self.channel = Channel(self.process.stdout, self.process.stdin)
        self.seq = 0
        self.events = []

    def send(self, command, arguments=None):
        self.seq += 1
        request = {"seq": self.seq, "type": "request", "command": command}
        if arguments is not None:
            request["arguments"] = arguments
        self.channel.send_raw(request)
        while True:
            message = self.channel.read_message()
            if message is None:
                return {"success": False, "message": "eof"}
            if message["type"] == "event":
                self.events.append(message)
            elif message.get("request_seq") == self.seq:
                return message

    def wait_event(self, name):
        for index, event in enumerate(self.events):
            if event["event"] == name:
                return self.events.pop(index)
        while True:
            message = self.channel.read_message()
            if message is None:
                return None
            if message["type"] != "event":
                continue
            if message["event"] == name:
                return message
            self.events.append(message)


def send_raw(self, message):
    import json
    body = json.dumps(message).encode("utf-8")
    self.writer.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
    self.writer.flush()


Channel.send_raw = send_raw


def test_mi_parser():
    record = parse_record('12^done,stack=[frame={level="0",func="f",line="3"},frame={level="1",func="main"}]')
    check("mi result record", record["class"] == "done" and len(record["data"]["stack"]) == 2)
    check("mi nested value", record["data"]["stack"][1]["func"] == "main")
    stream = parse_record('~"a \\"b\\"\\n"')
    check("mi console stream unescape", stream["text"] == 'a "b"\n')
    stopped = parse_record('*stopped,reason="exited",exit-code="067"')
    check("mi async record", stopped["data"]["exit-code"] == "067")
    check("mi prompt ignored", parse_record("(gdb) ") is None)


def test_session(firnc, source, workdir):
    program = os.path.join(workdir, "debuggee")
    build = subprocess.run([firnc, "--no-opt", "-o", program, source], capture_output=True, text=True)
    check("debuggee builds", build.returncode == 0, build.stderr[-300:])
    client = Client(os.path.join(HERE, "dap_server.py"))
    init = client.send("initialize", {"adapterID": "firn"})
    check("initialize capabilities", init["success"] and init["body"]["supportsConfigurationDoneRequest"])
    check("initialized event", client.wait_event("initialized") is not None)
    check("launch", client.send("launch", {"program": program})["success"])
    bp = client.send("setBreakpoints", {"source": {"path": os.path.abspath(source)}, "breakpoints": [{"line": 3}]})
    check("breakpoint verified", bp["success"] and bp["body"]["breakpoints"][0]["verified"], bp)
    check("configurationDone", client.send("configurationDone")["success"])
    stopped = client.wait_event("stopped")
    check("stopped at breakpoint", stopped and stopped["body"]["reason"] == "breakpoint", stopped)
    stack = client.send("stackTrace", {"threadId": 1})["body"]["stackFrames"]
    check("stack names", [f["name"] for f in stack][:2] == ["sum_to", "main"], stack)
    check("stack line", stack[0]["line"] == 3, stack[0])
    scopes = client.send("scopes", {"frameId": stack[0]["id"]})["body"]["scopes"]
    variables = client.send("variables", {"variablesReference": scopes[0]["variablesReference"]})["body"]["variables"]
    values = {v["name"]: v["value"] for v in variables}
    check("variable n", values.get("n") == "10", values)
    check("evaluate expression", client.send("evaluate", {"expression": "n + 1", "frameId": stack[0]["id"]})["body"]["result"] == "11")
    check("evaluate error reported", not client.send("evaluate", {"expression": "nope_x"})["success"])
    check("next", client.send("next", {"threadId": 1})["success"])
    step = client.wait_event("stopped")
    check("stopped after next", step and step["body"]["reason"] == "step", step)
    check("function breakpoint", client.send("setFunctionBreakpoints", {"breakpoints": [{"name": "main"}]})["success"])
    check("unsupported request fails", not client.send("noSuchRequest")["success"])
    check("continue", client.send("continue", {"threadId": 1})["success"])
    exited = client.wait_event("exited")
    check("exit code", exited and exited["body"]["exitCode"] == 55, exited)
    check("terminated event", client.wait_event("terminated") is not None)
    client.send("disconnect")
    client.process.wait(timeout=10)
    check("server exits", client.process.returncode == 0, client.process.returncode)


def main():
    signal.alarm(90)
    firnc, source, workdir = sys.argv[1:4]
    test_mi_parser()
    test_session(firnc, source, workdir)
    print("%d failure(s)" % len(failures))
    sys.exit(1 if failures else 0)


main()
