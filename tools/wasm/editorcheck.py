#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import base64
import io
import json
import os
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request

import numpy as np
import websocket
from PIL import Image

SITE = os.path.abspath(sys.argv[1])
APP = sys.argv[2] if len(sys.argv) > 2 else "editor"
WIDTH = 1100
HEIGHT = 700
STATUS_TOP = 672
STATUS_LEFT_WIDTH = 900
STATUS_BOTTOM = 696
EDGE = 3
TEXT_TOP = 160
TEXT_BOTTOM = 660
BUTTON_Y = 54
BUTTON_NEW = (36, BUTTON_Y)
BUTTON_OPEN = (110, BUTTON_Y)
BUTTON_SAVE = (184, BUTTON_Y)
TEXT_CLICK = (550, 300)
OPENED_NAME = "opened.md"
OPENED_BODY = "# Title\nline two\n"
failures = []


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


class Browser:
    def __init__(self, download_dir):
        self.port = free_port()
        self.proc = subprocess.Popen(
            ["chromium", "--headless=new", "--no-sandbox", "--disable-gpu",
             "--hide-scrollbars", "--force-color-profile=srgb",
             "--remote-debugging-port=%d" % self.port,
             "--window-size=%d,%d" % (WIDTH, HEIGHT), "about:blank"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.ws = self.connect()
        self.next_id = 0
        self.events = []
        self.call("Page.enable")
        self.call("Runtime.enable")
        self.call("DOM.enable")
        self.call("Browser.setDownloadBehavior", behavior="allow", downloadPath=download_dir)

    def connect(self):
        for _ in range(100):
            try:
                with urllib.request.urlopen("http://127.0.0.1:%d/json" % self.port) as r:
                    pages = [t for t in json.load(r) if t["type"] == "page"]
                    if pages:
                        return websocket.create_connection(
                            pages[0]["webSocketDebuggerUrl"], timeout=30, suppress_origin=True)
            except OSError:
                pass
            time.sleep(0.1)
        raise SystemExit("editorcheck: chromium did not come up")

    def call(self, method, **params):
        self.next_id += 1
        me = self.next_id
        self.ws.send(json.dumps({"id": me, "method": method, "params": params}))
        while True:
            m = json.loads(self.ws.recv())
            if "method" in m:
                self.events.append(m)
            if m.get("id") == me:
                if "error" in m:
                    raise SystemExit("editorcheck: %s: %s" % (method, m["error"]))
                return m.get("result", {})

    def js(self, expr):
        r = self.call("Runtime.evaluate", expression=expr, returnByValue=True)
        return r.get("result", {}).get("value")

    def shot(self):
        r = self.call("Page.captureScreenshot", format="png",
                      clip={"x": 0, "y": 0, "width": WIDTH, "height": HEIGHT, "scale": 1})
        img = Image.open(io.BytesIO(base64.b64decode(r["data"]))).convert("RGB")
        return np.asarray(img).astype(np.int32)

    def mouse(self, kind, x, y, **kw):
        self.call("Input.dispatchMouseEvent", type=kind, x=x, y=y, **kw)

    def click(self, point):
        x, y = point
        self.mouse("mouseMoved", x, y)
        self.mouse("mousePressed", x, y, button="left", buttons=1, clickCount=1)
        self.mouse("mouseReleased", x, y, button="left", buttons=0, clickCount=1)
        time.sleep(0.4)

    def key(self, name, code, vk=0, text=None):
        p = {"key": name, "code": code}
        if vk:
            p["windowsVirtualKeyCode"] = vk
        if text is not None:
            p["text"] = text
        self.call("Input.dispatchKeyEvent", type="keyDown", **p)
        self.call("Input.dispatchKeyEvent", type="keyUp", key=name, code=code)

    def type_text(self, text):
        for ch in text:
            self.key(ch, "Key" + ch.upper(), 0, ch)
        time.sleep(0.2)

    def wait_until(self, cond, seconds=10.0):
        end = time.time() + seconds
        while time.time() < end:
            if cond():
                return True
            time.sleep(0.1)
        return False

    def close(self):
        try:
            self.ws.close()
        finally:
            self.proc.terminate()
            self.proc.wait()


def verdict(ok, text):
    print("   %-72s %s" % (text, "OK" if ok else "FAILED"))
    if not ok:
        failures.append(text)


def region_diff(a, b, top, bottom, left=0, right=WIDTH):
    return int((a[top:bottom, left:right] != b[top:bottom, left:right]).any(axis=2).sum())


def ink_rows(img, top, bottom):
    area = img[top:bottom, EDGE:WIDTH - EDGE]
    background = area[0, -5]
    return int((area != background).any(axis=2).any(axis=1).sum())


def choose_file(browser, path):
    for event in browser.events:
        if event["method"] == "Page.fileChooserOpened":
            browser.events.remove(event)
            browser.call("DOM.setFileInputFiles", files=[path], backendNodeId=event["params"]["backendNodeId"])
            return True
    browser.call("Runtime.evaluate", expression="0")
    return False


def run(browser, port, work):
    browser.call("Emulation.setDeviceMetricsOverride", width=WIDTH, height=HEIGHT,
                 deviceScaleFactor=1, mobile=False)
    browser.call("Page.navigate", url="http://127.0.0.1:%d/%s.html?w=%d&h=%d" % (port, APP, WIDTH, HEIGHT))
    verdict(browser.wait_until(lambda: (browser.js("window.firnFrames || 0") or 0) >= 1), "the editor starts and paints")
    time.sleep(0.5)
    start = browser.shot()
    verdict(ink_rows(start, TEXT_TOP, TEXT_BOTTOM) > 100, "start: the sample document is shown")

    browser.click(TEXT_CLICK)
    browser.type_text("x")
    edited = browser.shot()
    verdict(region_diff(start, edited, STATUS_TOP, STATUS_BOTTOM, EDGE, STATUS_LEFT_WIDTH) > 0, "typing: the status bar shows the modified flag")
    verdict(region_diff(start, edited, TEXT_TOP, TEXT_BOTTOM) > 0, "typing: the text changed")

    browser.click(BUTTON_SAVE)
    verdict(browser.wait_until(lambda: (browser.js("(window.firnDownloads || []).length") or 0) >= 1), "Save: the browser got a download")
    time.sleep(0.6)
    saved = browser.shot()
    if os.environ.get("EDITORCHECK_DEBUG"):
        Image.fromarray(saved.astype(np.uint8)).save(os.path.join(os.environ["EDITORCHECK_DEBUG"], "saved.png"))
        Image.fromarray(start.astype(np.uint8)).save(os.path.join(os.environ["EDITORCHECK_DEBUG"], "start.png"))
        Image.fromarray(edited.astype(np.uint8)).save(os.path.join(os.environ["EDITORCHECK_DEBUG"], "edited.png"))
    info = browser.js("JSON.stringify((window.firnDownloads || [])[0] || {})") or "{}"
    download = json.loads(info)
    verdict(download.get("name") == "editor_sample.fi", "Save: the download carries the document name (%s)" % download.get("name"))
    verdict(region_diff(start, saved, STATUS_TOP, STATUS_BOTTOM, EDGE, STATUS_LEFT_WIDTH) == 0, "Save: the modified flag is gone again")

    opened = os.path.join(work, OPENED_NAME)
    with open(opened, "w") as f:
        f.write(OPENED_BODY)
    browser.call("Page.setInterceptFileChooserDialog", enabled=True)
    browser.click(BUTTON_OPEN)
    verdict(browser.wait_until(lambda: choose_file(browser, opened)), "Open: the browser file picker appeared")
    time.sleep(1.0)
    shown = browser.shot()
    verdict(ink_rows(shown, TEXT_TOP, TEXT_BOTTOM) < 60, "Open: the short file replaced the sample document")
    verdict(region_diff(saved, shown, STATUS_TOP, STATUS_BOTTOM, EDGE, STATUS_LEFT_WIDTH) > 0, "Open: the status bar shows the new file")

    browser.click(TEXT_CLICK)
    browser.type_text("z")
    browser.click(BUTTON_NEW)
    asked = browser.shot()
    verdict(region_diff(shown, asked, 200, 500) > 0, "New: a modified document asks before it is dropped")


def main():
    port = free_port()
    server = subprocess.Popen([sys.executable, "-m", "http.server", str(port), "--bind", "127.0.0.1",
                               "--directory", SITE], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(0.5)
    with tempfile.TemporaryDirectory() as work:
        browser = Browser(work)
        try:
            run(browser, port, work)
        finally:
            browser.close()
            server.terminate()
    if failures:
        print("EDITORCHECK FAILED (%d)" % len(failures))
        sys.exit(1)
    print("EDITORCHECK PASSED")


main()
