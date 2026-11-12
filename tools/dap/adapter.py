import os
import threading

from gdb_session import GdbError, GdbSession

THREAD_ID = 1
FRAME_STRIDE = 1000
STOP_REASONS = {
    "breakpoint-hit": "breakpoint",
    "end-stepping-range": "step",
    "function-finished": "step",
    "signal-received": "exception",
}
EXIT_REASONS = ("exited", "exited-normally", "exited-signalled")


def quote(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def parse_exit_code(text):
    if len(text) > 1 and text.startswith("0"):
        return int(text, 8)
    return int(text)


class Adapter:
    def __init__(self, channel, gdb_path="gdb"):
        self.channel = channel
        self.gdb_path = gdb_path
        self.gdb = None
        self.source_breakpoints = {}
        self.function_breakpoints = []
        self.stop_on_entry = False
        self.running = False
        self.done = False
        self.handlers = {
            "initialize": self.initialize,
            "launch": self.launch,
            "setBreakpoints": self.set_breakpoints,
            "setFunctionBreakpoints": self.set_function_breakpoints,
            "setExceptionBreakpoints": lambda args: {},
            "configurationDone": self.configuration_done,
            "threads": self.threads,
            "stackTrace": self.stack_trace,
            "scopes": self.scopes,
            "variables": self.variables,
            "continue": self.resume("-exec-continue", {"allThreadsContinued": True}),
            "next": self.resume("-exec-next"),
            "stepIn": self.resume("-exec-step"),
            "stepOut": self.resume("-exec-finish"),
            "pause": self.pause,
            "evaluate": self.evaluate,
            "disconnect": self.disconnect,
            "terminate": self.disconnect,
        }

    def serve(self):
        while not self.done:
            request = self.channel.read_message()
            if request is None:
                break
            self.dispatch(request)
        if self.gdb:
            self.gdb.close()

    def dispatch(self, request):
        handler = self.handlers.get(request["command"])
        if handler is None:
            self.channel.respond(request, error="unsupported request: " + request["command"])
            return
        try:
            body = handler(request.get("arguments") or {})
        except (GdbError, KeyError, ValueError, OSError) as error:
            self.channel.respond(request, error=str(error))
            return
        self.channel.respond(request, body or {})
        if request["command"] == "initialize":
            self.channel.event("initialized")

    def initialize(self, args):
        return {
            "supportsConfigurationDoneRequest": True,
            "supportsFunctionBreakpoints": True,
            "supportsEvaluateForHovers": True,
            "supportsTerminateRequest": True,
        }

    def start_output_pump(self):
        master, slave = os.openpty()
        self.gdb.command("-inferior-tty-set " + os.ttyname(slave))
        def pump():
            while True:
                try:
                    data = os.read(master, 4096)
                except OSError:
                    return
                if not data:
                    return
                self.channel.event("output", {"category": "stdout", "output": data.decode("utf-8", "replace")})
        threading.Thread(target=pump, daemon=True).start()

    def launch(self, args):
        program = args["program"]
        if not os.path.isfile(program):
            raise ValueError("program not found: " + program)
        self.gdb = GdbSession(self.gdb_path)
        self.stop_on_entry = bool(args.get("stopOnEntry"))
        if args.get("cwd"):
            self.gdb.command("-environment-cd " + quote(args["cwd"]))
        self.gdb.command("-file-exec-and-symbols " + quote(program))
        if args.get("args"):
            self.gdb.command("-exec-arguments " + " ".join(quote(a) for a in args["args"]))
        self.start_output_pump()
        threading.Thread(target=self.event_loop, daemon=True).start()
        return {}

    def clear(self, ids):
        for number in ids:
            self.gdb.command("-break-delete " + number)

    def insert(self, location):
        data = self.gdb.command("-break-insert -f " + location)
        bkpt = data["bkpt"]
        return bkpt["number"], {"verified": "pending" not in bkpt and bkpt.get("addr") != "<PENDING>", "line": int(bkpt.get("line", 0))}

    def set_breakpoints(self, args):
        path = args["source"]["path"]
        self.clear(self.source_breakpoints.get(path, []))
        ids, results = [], []
        for item in args.get("breakpoints", []):
            number, result = self.insert("--source %s --line %d" % (quote(path), item["line"]))
            ids.append(number)
            results.append(result)
        self.source_breakpoints[path] = ids
        return {"breakpoints": results}

    def set_function_breakpoints(self, args):
        self.clear(self.function_breakpoints)
        self.function_breakpoints, results = [], []
        for item in args.get("breakpoints", []):
            number, result = self.insert("--function " + quote(item["name"]))
            self.function_breakpoints.append(number)
            results.append(result)
        return {"breakpoints": results}

    def configuration_done(self, args):
        self.running = True
        self.gdb.command("-exec-run --start" if self.stop_on_entry else "-exec-run")
        return {}

    def threads(self, args):
        return {"threads": [{"id": THREAD_ID, "name": "main"}]}

    def stack_trace(self, args):
        data = self.gdb.command("-stack-list-frames --thread %d" % THREAD_ID)
        frames = []
        for frame in data.get("stack", []):
            entry = {
                "id": int(frame["level"]) * FRAME_STRIDE,
                "name": frame.get("func", "??"),
                "line": int(frame.get("line", 0)),
                "column": 1,
            }
            if "fullname" in frame:
                entry["source"] = {"name": os.path.basename(frame["fullname"]), "path": frame["fullname"]}
            frames.append(entry)
        return {"stackFrames": frames, "totalFrames": len(frames)}

    def scopes(self, args):
        frame_id = args["frameId"]
        return {"scopes": [{"name": "Locals", "variablesReference": frame_id + 1, "expensive": False}]}

    def variables(self, args):
        level = (args["variablesReference"] - 1) // FRAME_STRIDE
        data = self.gdb.command("-stack-list-variables --thread %d --frame %d --all-values" % (THREAD_ID, level))
        return {
            "variables": [
                {"name": item["name"], "value": item.get("value", ""), "variablesReference": 0}
                for item in data.get("variables", [])
            ]
        }

    def resume(self, command, extra=None):
        def handler(args):
            self.gdb.command(command)
            return dict(extra or {})
        return handler

    def pause(self, args):
        self.gdb.command("-exec-interrupt")
        return {}

    def evaluate(self, args):
        level = (args.get("frameId") or 0) // FRAME_STRIDE
        self.gdb.command("-stack-select-frame %d" % level)
        data = self.gdb.command("-data-evaluate-expression " + quote(args["expression"]))
        return {"result": data.get("value", ""), "variablesReference": 0}

    def disconnect(self, args):
        self.done = True
        return {}

    def event_loop(self):
        while True:
            record = self.gdb.events.get()
            if record["class"] == "gdb-exit":
                return
            if record["class"] == "stopped":
                self.on_stopped(record["data"])

    def on_stopped(self, data):
        reason = data.get("reason", "")
        if reason in EXIT_REASONS:
            code = parse_exit_code(data.get("exit-code", "0"))
            self.channel.event("exited", {"exitCode": code})
            self.channel.event("terminated")
            return
        mapped = STOP_REASONS.get(reason, reason or "pause")
        if self.stop_on_entry:
            self.stop_on_entry = False
            mapped = "entry"
        body = {"reason": mapped, "threadId": THREAD_ID, "allThreadsStopped": True}
        self.channel.event("stopped", body)
