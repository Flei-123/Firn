#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/lsp/client.py — ein echter LSP-Klient fuer die Gegenprobe.

Er spricht mit `firnc --lsp` ueber Standardein- und -ausgabe, genau so wie ein
Editor es taete: `Content-Length`-Kopf, JSON-Rumpf, Anfragen mit Nummer,
Benachrichtigungen ohne. Er prueft die Antworten gegen Erwartungen und gibt
am Ende PASSED/FAILED je Fall aus.

Aufruf:  client.py <pfad-zu-firnc> <pfad-zur-testdatei>
"""
import json
import os
import subprocess
import sys

PASS = 0
FAIL = 0


def ok(what):
    global PASS
    PASS += 1
    print(f"  ok    {what}")


def bad(what, got=None):
    global FAIL
    FAIL += 1
    print(f"  FAIL  {what}")
    if got is not None:
        print(f"        got: {json.dumps(got)[:400]}")


class Server:
    def __init__(self, exe):
        self.p = subprocess.Popen(
            [exe, "--lsp"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        self.id = 0

    def send(self, msg):
        body = json.dumps(msg).encode()
        self.p.stdin.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
        self.p.stdin.flush()

    def request(self, method, params):
        self.id += 1
        self.send({"jsonrpc": "2.0", "id": self.id, "method": method,
                   "params": params})
        while True:
            m = self.read()
            if m.get("id") == self.id:
                return m

    def notify(self, method, params):
        self.send({"jsonrpc": "2.0", "method": method, "params": params})

    def read(self):
        length = 0
        while True:
            line = self.p.stdout.readline()
            if not line:
                raise SystemExit("the server closed the connection")
            if line in (b"\r\n", b"\n"):
                break
            if line.lower().startswith(b"content-length:"):
                length = int(line.split(b":")[1].strip())
        return json.loads(self.p.stdout.read(length))

    def wait_for(self, method):
        for _ in range(20):
            m = self.read()
            if m.get("method") == method:
                return m
        raise SystemExit(f"no {method} arrived")

    def close(self):
        self.request("shutdown", {})
        self.notify("exit", {})
        self.p.wait(timeout=10)


def find(text, needle):
    for i, ln in enumerate(text.split("\n")):
        c = ln.find(needle)
        if c >= 0:
            return i, c
    raise SystemExit(f"'{needle}' not found in the test file")


def main():
    exe, path = sys.argv[1], sys.argv[2]
    uri = "file://" + os.path.abspath(path)
    src = open(path, encoding="utf-8").read()
    s = Server(exe)

    # --- 1. initialize -----------------------------------------------------
    r = s.request("initialize", {"processId": os.getpid(), "rootUri": None,
                                 "capabilities": {}})
    caps = r.get("result", {}).get("capabilities", {})
    for want in ("definitionProvider", "renameProvider", "hoverProvider",
                 "completionProvider", "documentFormattingProvider"):
        if caps.get(want):
            ok(f"initialize announces {want}")
        else:
            bad(f"initialize announces {want}", caps)
    s.notify("initialized", {})

    # --- 2. didOpen -> diagnostics ----------------------------------------
    s.notify("textDocument/didOpen", {"textDocument": {
        "uri": uri, "languageId": "firn", "version": 1, "text": src}})
    d = s.wait_for("textDocument/publishDiagnostics")
    diags = d["params"]["diagnostics"]
    if diags == []:
        ok("the sound file has no diagnostics")
    else:
        bad("the sound file has no diagnostics", diags)

    # --- 3. definition ----------------------------------------------------
    l, c = find(src, "total(2)")
    r = s.request("textDocument/definition", {
        "textDocument": {"uri": uri}, "position": {"line": l, "character": c}})
    res = r.get("result")
    if res and res.get("range", {}).get("start", {}).get("line") == \
            find(src, "fn total(")[0]:
        ok("definition of the function `total`")
    else:
        bad("definition of the function `total`", res)

    # a LOCAL variable: `count` inside `total`
    l, c = find(src, "return count")
    r = s.request("textDocument/definition", {
        "textDocument": {"uri": uri},
        "position": {"line": l, "character": c + len("return ")}})
    res = r.get("result")
    if res and res.get("range", {}).get("start", {}).get("line") == \
            find(src, "var count")[0]:
        ok("definition of the local variable `count`")
    else:
        bad("definition of the local variable `count`", res)

    # --- 4. hover ---------------------------------------------------------
    l, c = find(src, "total(2)")
    r = s.request("textDocument/hover", {
        "textDocument": {"uri": uri}, "position": {"line": l, "character": c}})
    val = (r.get("result") or {}).get("contents", {}).get("value", "")
    if "function" in val and "total" in val:
        ok("hover names the function")
    else:
        bad("hover names the function", r.get("result"))

    # --- 5. completion ----------------------------------------------------
    l, c = find(src, "return count")
    r = s.request("textDocument/completion", {
        "textDocument": {"uri": uri}, "position": {"line": l, "character": c}})
    labels = [i["label"] for i in (r.get("result") or {}).get("items", [])]
    for want in ("total", "count", "Point", "LIMIT", "while"):
        if want in labels:
            ok(f"completion offers `{want}`")
        else:
            bad(f"completion offers `{want}`", labels[:30])
    if "other_only" not in labels:
        ok("completion does not offer a foreign local")
    else:
        bad("completion does not offer a foreign local", labels[:30])

    # --- 6. rename --------------------------------------------------------
    l, c = find(src, "var count")
    r = s.request("textDocument/rename", {
        "textDocument": {"uri": uri},
        "position": {"line": l, "character": c + len("var ")},
        "newName": "tally"})
    edits = (r.get("result") or {}).get("changes", {}).get(uri, [])
    # `count` stands four times inside `total`: declaration, twice in
    # `count = count + k`, and in `return count`.
    if len(edits) == 4:
        ok("rename changes exactly the four places of the local")
    else:
        bad("rename changes exactly the four places of the local", edits)
    field_line = find(src, "count: i32")[0]
    if all(e["range"]["start"]["line"] != field_line for e in edits):
        ok("rename leaves the struct field alone")
    else:
        bad("rename leaves the struct field alone", edits)

    l, c = find(src, "fn total(")
    r = s.request("textDocument/rename", {
        "textDocument": {"uri": uri},
        "position": {"line": l, "character": c + len("fn ")},
        "newName": "amount"})
    edits = (r.get("result") or {}).get("changes", {}).get(uri, [])
    if len(edits) == 2:
        ok("rename of the function hits declaration and call")
    else:
        bad("rename of the function hits declaration and call", edits)

    # --- 7. a broken file gives diagnostics WITH a suggestion -------------
    broken = src.replace("return count", "return cout")
    s.notify("textDocument/didChange", {
        "textDocument": {"uri": uri, "version": 2},
        "contentChanges": [{"text": broken}]})
    d = s.wait_for("textDocument/publishDiagnostics")
    diags = d["params"]["diagnostics"]
    if len(diags) >= 1 and "unknown name 'cout'" in diags[0]["message"]:
        ok("the broken file reports the unknown name")
    else:
        bad("the broken file reports the unknown name", diags)
    if diags and "help: did you mean 'count'?" in diags[0]["message"]:
        ok("the diagnostic carries the suggestion")
    else:
        bad("the diagnostic carries the suggestion", diags)
    if diags and diags[0]["range"]["start"]["line"] == find(broken, "return cout")[0]:
        ok("the diagnostic sits on the right line")
    else:
        bad("the diagnostic sits on the right line", diags)

    s.notify("textDocument/didChange", {
        "textDocument": {"uri": uri, "version": 3},
        "contentChanges": [{"text": src}]})
    d = s.wait_for("textDocument/publishDiagnostics")
    if d["params"]["diagnostics"] == []:
        ok("after the correction the diagnostics are gone")
    else:
        bad("after the correction the diagnostics are gone",
            d["params"]["diagnostics"])

    # --- 8. counter-checks -------------------------------------------------
    r = s.request("textDocument/definition", {
        "textDocument": {"uri": uri}, "position": {"line": 0, "character": 0}})
    if r.get("result") in (None, [], {}):
        ok("no definition in the void")
    else:
        bad("no definition in the void", r.get("result"))
    r = s.request("textDocument/thisDoesNotExist", {})
    if "id" in r:
        ok("an unknown request is answered")
    else:
        bad("an unknown request is answered", r)

    # --- 9. formatting ----------------------------------------------------
    ugly = "fn main( ) -> i32 {\n  return    1\n}\n"
    s.notify("textDocument/didChange", {
        "textDocument": {"uri": uri, "version": 4},
        "contentChanges": [{"text": ugly}]})
    s.wait_for("textDocument/publishDiagnostics")
    r = s.request("textDocument/formatting", {
        "textDocument": {"uri": uri},
        "options": {"tabSize": 4, "insertSpaces": True}})
    edits = r.get("result") or []
    if edits and edits[0]["newText"] == "fn main() -> i32 {\n    return 1\n}\n":
        ok("formatting delivers the shape of firnfmt")
    else:
        bad("formatting delivers the shape of firnfmt", edits)

    s.close()
    print(f"\nLSP: {PASS} passed, {FAIL} failed")
    return 1 if FAIL else 0


if __name__ == "__main__":
    sys.exit(main())
