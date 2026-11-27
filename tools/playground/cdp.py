import json
import os
import shutil
import subprocess
import tempfile
import time
import urllib.request

from websockets.sync.client import connect

MOD_ALT, MOD_CTRL, MOD_SHIFT = 1, 2, 8
KEYS = {
    "Enter": ("Enter", 13, "\r"), "Tab": ("Tab", 9, ""), "Backspace": ("Backspace", 8, ""),
    "Escape": ("Escape", 27, ""), "ArrowUp": ("ArrowUp", 38, ""), "ArrowDown": ("ArrowDown", 40, ""),
    "Home": ("Home", 36, ""), "ArrowLeft": ("ArrowLeft", 37, ""), "ArrowRight": ("ArrowRight", 39, ""),
}
COMMANDS = {("z", MOD_CTRL): ["undo"], ("y", MOD_CTRL): ["redo"], ("z", MOD_CTRL | MOD_SHIFT): ["redo"], ("a", MOD_CTRL): ["selectAll"]}


def find_browser():
    for name in ("chromium", "chromium-browser", "google-chrome", "chrome"):
        path = shutil.which(name)
        if path:
            return path
    return None


class Browser:
    def __init__(self, binary, width=1400, height=900, extra_args=()):
        self.profile = tempfile.mkdtemp(prefix="firn-playground-browser-", dir="/var/tmp")
        port = self.free_port()
        self.process = subprocess.Popen(
            [binary, "--headless=new", "--no-sandbox", "--disable-gpu", f"--window-size={width},{height}",
             f"--remote-debugging-port={port}", f"--user-data-dir={self.profile}", *extra_args, "about:blank"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        self.next_id = 0
        self.socket = connect(self.page_socket_url(port), max_size=None)
        self.send("Page.enable")
        self.send("Runtime.enable")

    @staticmethod
    def free_port():
        import socket
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            return probe.getsockname()[1]

    def page_socket_url(self, port):
        for _ in range(100):
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{port}/json", timeout=2) as reply:
                    pages = [item for item in json.load(reply) if item.get("type") == "page"]
                if pages:
                    return pages[0]["webSocketDebuggerUrl"]
            except OSError:
                pass
            time.sleep(0.1)
        raise RuntimeError("browser did not start")

    def send(self, method, **params):
        self.next_id += 1
        self.socket.send(json.dumps({"id": self.next_id, "method": method, "params": params}))
        while True:
            message = json.loads(self.socket.recv(timeout=60))
            if message.get("id") == self.next_id:
                if "error" in message:
                    raise RuntimeError(message["error"])
                return message.get("result", {})

    def evaluate(self, expression):
        result = self.send("Runtime.evaluate", expression=expression, returnByValue=True, awaitPromise=True)
        if "exceptionDetails" in result:
            raise RuntimeError(result["exceptionDetails"]["exception"].get("description", "script error"))
        return result["result"].get("value")

    def goto(self, url):
        self.send("Page.navigate", url=url)
        self.wait_for("document.readyState === 'complete'")

    def wait_for(self, expression, timeout=10.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.evaluate(expression):
                return True
            time.sleep(0.05)
        return False

    def press(self, key, modifiers=0):
        if key in KEYS:
            name, code, text = KEYS[key]
            event = {"key": name, "code": name, "windowsVirtualKeyCode": code, "modifiers": modifiers}
            if text and not modifiers:
                event["text"] = text
        elif len(key) == 1 and not modifiers & (MOD_CTRL | MOD_ALT):
            event = {"key": key, "text": key, "unmodifiedText": key, "modifiers": modifiers}
        else:
            event = {"key": key, "code": "Key" + key.upper() if key.isalpha() else "", "windowsVirtualKeyCode": ord(key.upper()), "modifiers": modifiers}
        commands = COMMANDS.get((key, modifiers))
        if commands:
            event["commands"] = commands
        self.send("Input.dispatchKeyEvent", type="keyDown", **event)
        self.send("Input.dispatchKeyEvent", type="keyUp", **{name: value for name, value in event.items() if name not in ("text", "unmodifiedText", "commands")})

    def type(self, text):
        for character in text:
            self.press("Enter" if character == "\n" else character)

    def mouse_move(self, x, y):
        self.send("Input.dispatchMouseEvent", type="mouseMoved", x=x, y=y)

    def screenshot(self, path):
        import base64
        data = self.send("Page.captureScreenshot", format="png")["data"]
        with open(path, "wb") as handle:
            handle.write(base64.b64decode(data))

    def close(self):
        try:
            self.socket.close()
        finally:
            self.process.terminate()
            self.process.wait()
            shutil.rmtree(self.profile, ignore_errors=True)
