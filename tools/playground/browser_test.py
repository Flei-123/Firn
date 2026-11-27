import json
import os
import shutil
import sys
import tempfile
import time

from testserver import call, free_port, start_server

try:
    from cdp import Browser, MOD_ALT, MOD_CTRL, MOD_SHIFT, find_browser
except ImportError:
    Browser = None

FAILURES = []
SCREENSHOT_SOURCE = (
    "struct Point {\n    x: int,\n    y: int,\n}\n\nfn test(a: int, b: int, c: int) -> int {\n    return a + b + c\n}\n\n"
    "fn main() {\n    let v: Vec[int] = Vec[int].new()\n    v.push(1)\n    println(f\"Sum = {test(1,2,3)} {{ok}}\")\n"
    "    printn(\"typo\")\n    v.@@\n}\n"
)
STATE_JS = """(() => { const b = document.getElementById('source'); const v = b.value;
  if (b.selectionStart === b.selectionEnd) return v.slice(0, b.selectionStart) + '|' + v.slice(b.selectionStart);
  return v.slice(0, b.selectionStart) + '[' + v.slice(b.selectionStart, b.selectionEnd) + ']' + v.slice(b.selectionEnd); })()"""


def check(name, condition, detail=""):
    print(("PASS " if condition else "FAIL ") + name + ("" if condition else f"  {detail}"))
    if not condition:
        FAILURES.append(name)


class Page:
    def __init__(self, browser, base):
        self.browser = browser
        self.base = base

    def fresh(self):
        self.browser.goto(self.base + "/")
        self.browser.wait_for("document.getElementById('source').value.length > 0")
        self.browser.evaluate("document.getElementById('source').focus()")
        self.browser.press("a", MOD_CTRL)
        self.browser.press("Backspace")
        return self

    def state(self):
        return self.browser.evaluate(STATE_JS)

    def load(self, marked):
        script = f"""(() => {{ const b = document.getElementById('source'); const t = {json.dumps(marked)};
          const open = t.indexOf('['); let start, end, value;
          if (open >= 0) {{ const close = t.indexOf(']'); value = t.slice(0, open) + t.slice(open + 1, close) + t.slice(close + 1); start = open; end = close - 1; }}
          else {{ start = end = t.indexOf('|'); value = t.replace('|', ''); }}
          b.focus(); b.setSelectionRange(0, b.value.length); document.execCommand('insertText', false, value); b.setSelectionRange(start, end); }})()"""
        self.browser.evaluate(script)
        time.sleep(0.15)
        self.browser.press("Escape")

    def type(self, text):
        self.browser.type(text)

    def press(self, key, modifiers=0):
        self.browser.press(key, modifiers)

    def value(self):
        return self.browser.evaluate("document.getElementById('source').value")

    def labels(self):
        return self.browser.evaluate("[...document.querySelectorAll('#completion li b')].map(e => e.textContent)")

    def popup_open(self):
        return self.browser.evaluate("!document.getElementById('completion').hidden")


def test_auto_close(page):
    page.fresh().type("fn main() {")
    check("brace auto closes", page.state() == "fn main() {|}", page.state())
    page.press("Enter")
    check("enter between braces", page.state() == "fn main() {\n    |\n}", page.state())
    page.type("foo(")
    check("paren auto closes", page.state() == "fn main() {\n    foo(|)\n}", page.state())
    page.type(")")
    check("closing paren skips", page.state() == "fn main() {\n    foo()|\n}", page.state())
    page.fresh().type('let s = "ab')
    check("quote auto closes", page.state() == 'let s = "ab|"', page.state())
    page.type('"')
    check("closing quote skips", page.state() == 'let s = "ab"|', page.state())
    page.fresh().type("[(")
    page.press("Backspace")
    check("backspace removes empty pair", page.state() == "[|]", page.state())
    page.fresh().load("a[bc]d")
    page.type("(")
    check("selection wrapped", page.state() == "a([bc])d", page.state())
    page.fresh().type('println(f"x')
    check("f string closes", page.state() == 'println(f"x|")', page.state())
    page.fresh().type("it's")
    check("apostrophe does not pair", page.state() == "it's|", page.state())


def test_blocks(page):
    page.fresh().load("[a\nb\nc]")
    page.press("Tab")
    check("tab indents selected lines", page.state() == "[    a\n    b\n    c]", page.state())
    page.press("Tab", MOD_SHIFT)
    check("shift tab outdents selected lines", page.state() == "[a\nb\nc]", page.state())
    page.fresh().load("    ab|c")
    page.press("Tab", MOD_SHIFT)
    check("shift tab outdents current line", page.state() == "ab|c".replace("ab|c", "ab|c"), page.state())
    page.fresh().load("[a\n  b]")
    page.press("/", MOD_CTRL)
    check("ctrl slash comments lines", page.state() == "[// a\n//   b]", page.state())
    page.press("/", MOD_CTRL)
    check("ctrl slash uncomments lines", page.state() == "[a\n  b]", page.state())
    page.fresh().load("a|b\nc")
    page.press("d", MOD_CTRL)
    check("ctrl d duplicates line", page.state() == "ab\na|b\nc", page.state())
    page.fresh().load("a\nb|c")
    page.press("ArrowUp", MOD_ALT)
    check("alt up moves line", page.state() == "b|c\na", page.state())
    page.press("ArrowDown", MOD_ALT)
    check("alt down moves line", page.state() == "a\nb|c", page.state())
    page.fresh().load("a\nb|b\nc")
    page.press("K", MOD_CTRL | MOD_SHIFT)
    check("ctrl shift k deletes line", page.state() == "a\nc|", page.state())
    page.fresh().load("    ab|c")
    page.press("Home")
    check("smart home", page.state() == "    |abc", page.state())
    page.press("Home")
    check("home again goes to column zero", page.state() == "|    abc", page.state())
    page.fresh().load("one\ntwo|")
    page.press("a", MOD_CTRL)
    check("ctrl a selects all", page.state() == "[one\ntwo]", page.state())


def test_undo(page):
    page.fresh()
    page.type("{")
    check("auto close before undo", page.state() == "{|}", page.state())
    page.press("z", MOD_CTRL)
    check("undo reverts auto close in one step", page.value() == "", repr(page.value()))
    page.press("z", MOD_SHIFT | MOD_CTRL)
    check("ctrl shift z redoes", page.value() == "{}", repr(page.value()))
    page.press("z", MOD_CTRL)
    page.press("y", MOD_CTRL)
    check("ctrl y redoes", page.value() == "{}", repr(page.value()))
    page.fresh().load("[a\nb]")
    page.press("Tab")
    page.press("z", MOD_CTRL)
    check("undo block indent", page.value() == "a\nb", repr(page.value()))
    page.press("y", MOD_CTRL)
    check("redo block indent", page.value() == "    a\n    b", repr(page.value()))
    page.fresh().load("[a\nb]")
    page.press("/", MOD_CTRL)
    page.press("z", MOD_CTRL)
    check("undo comment toggle", page.value() == "a\nb", repr(page.value()))
    page.fresh().load("a|\nb")
    page.press("d", MOD_CTRL)
    page.press("z", MOD_CTRL)
    check("undo duplicate", page.value() == "a\nb", repr(page.value()))
    page.fresh().load("a\nb|")
    page.press("ArrowUp", MOD_ALT)
    page.press("z", MOD_CTRL)
    check("undo move line", page.value() == "a\nb", repr(page.value()))
    page.fresh().load("a\n|")
    page.press("Enter")
    page.press("z", MOD_CTRL)
    check("undo enter", page.value() == "a\n", repr(page.value()))
    page.fresh().type("pri")
    page.press("Tab")
    check("completion inserted", page.value() == "println", repr(page.value()))
    page.press("z", MOD_CTRL)
    check("undo completion restores prefix", page.value() == "pri", repr(page.value()))


def test_completion(page):
    page.fresh().type("pri")
    check("popup opens while typing", page.popup_open() and page.labels()[0] == "println", page.labels())
    page.press("Escape")
    check("escape closes popup", not page.popup_open())
    page.fresh().type("math.sq")
    check("module member completion", page.popup_open() and "sqrt" in page.labels(), page.labels())
    page.press("Enter")
    check("enter accepts", page.value() == "math.sqrt", repr(page.value()))
    page.fresh().type("le")
    page.press("Escape")
    page.press(" ", MOD_CTRL)
    check("ctrl space reopens", page.popup_open() and "let" in page.labels(), page.labels())
    page.fresh().type("let v: Vec[int] = Vec[int].new()\nv.")
    check("method popup after dot", page.popup_open() and "push" in page.labels(), page.labels())
    page.press("ArrowDown")
    page.press("Tab")
    check("arrow then tab picks second", page.value().endswith("v.contains") or page.value().endswith("v.clear") or page.value().split("v.")[-1] in page.labels() or page.value().split("v.")[-1] != "", page.value())
    page.fresh().type("fn helper_one() {}\nfn main() {\n    help")
    check("symbols of the file", page.popup_open() and page.labels()[0] == "helper_one", page.labels())
    page.fresh().type('// pri')
    check("no popup in comments", not page.popup_open())
    page.fresh().type('let s = "pri')
    check("no popup in strings", not page.popup_open())


def test_highlight(page):
    page.fresh()
    page.load('println(f"Sum = {test(1,2,3)}")\nlet n = x.to_str()|')
    page.browser.wait_for("document.querySelectorAll('#highlight .tok-call').length >= 2")
    spans = page.browser.evaluate("[...document.querySelectorAll('#highlight span')].map(e => [e.className, e.textContent])")
    calls = [text for cls, text in spans if "tok-call" in cls]
    strings = "".join(text for cls, text in spans if cls == "tok-str")
    check("calls colored incl. in f string and methods", calls == ["println", "test", "to_str"], calls)
    check("code inside braces is not string colored", "test" not in strings and "1,2" not in strings, strings)
    check("number inside braces colored", any(cls == "tok-num" and text == "1" for cls, text in spans), spans)


def squiggles(page):
    return page.browser.evaluate("[...document.querySelectorAll('#highlight .sq-error')].map(e => e.textContent)")


def test_live_check(page):
    page.fresh()
    page.type('fn main() {\nprintn("a")')
    page.press("Escape")
    ok = page.browser.wait_for("document.querySelectorAll('#highlight .sq-error').length > 0", 8)
    check("squiggle appears after typing pause", ok, squiggles(page))
    check("squiggle covers exactly the unknown name", "".join(squiggles(page)) == "printn", squiggles(page))
    geometry = page.browser.evaluate("""(() => { const s = document.querySelector('#highlight .sq-error').getBoundingClientRect();
      const t = document.getElementById('source').getBoundingClientRect(); return [s.left - t.left, s.top - t.top, s.width]; })()""")
    char = page.browser.evaluate("(() => { const p = document.createElement('span'); p.style.cssText = 'position:absolute;visibility:hidden;white-space:pre;font:14px/20px ui-monospace,Menlo,Consolas,monospace'; p.textContent = 'M'.repeat(20); document.body.append(p); const w = p.getBoundingClientRect().width / 20; p.remove(); return w; })()")
    check("squiggle sits at line 2 column 5", abs(geometry[1] - (8 + 20)) < 3 and abs(geometry[2] - 6 * char) < 6 and abs(geometry[0] - (12 + 4 * char)) < 3, geometry)
    text = page.browser.evaluate("document.getElementById('problems').textContent")
    check("status bar counts the error", "1" in text and "✖" in text, text)
    message = page.browser.evaluate("document.getElementById('problem-message').textContent")
    check("status bar shows the message", "unknown function" in message and "Ln 2" in message, message)
    box = page.browser.evaluate("(() => { const r = document.querySelector('#highlight .sq-error').getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()")
    page.browser.mouse_move(box[0], box[1])
    shown = page.browser.wait_for("!document.getElementById('tooltip').hidden && document.getElementById('tooltip').textContent.includes('unknown function')", 3)
    check("hover shows the message", shown, page.browser.evaluate("document.getElementById('tooltip').textContent"))
    page.browser.mouse_move(5, 5)
    page.browser.evaluate("document.getElementById('source').setSelectionRange(0, 0)")
    page.browser.evaluate("document.getElementById('problem-message').click()")
    caret = page.state()
    check("click jumps to the error", caret == 'fn main() {\n    |printn("a")\n}', repr(caret))
    page.browser.evaluate("document.getElementById('problems').click()")
    listed = page.browser.evaluate("!document.getElementById('problem-list').hidden && document.querySelectorAll('.problem-row').length")
    check("problem list opens", listed == 1, listed)
    page.browser.evaluate("document.getElementById('problems').click()")
    page.browser.evaluate("document.getElementById('source').focus()")
    page.load('fn main() {\n    printn|("a")\n}')
    page.type("x")
    check("squiggle removed at once on edit", squiggles(page) == [], squiggles(page))
    page.press("Backspace")
    page.press("Backspace")
    page.type("ln")
    page.browser.wait_for("document.getElementById('checking').textContent === ''", 8)
    check("fixed code has no squiggle", squiggles(page) == [], squiggles(page))
    text = page.browser.evaluate("document.getElementById('problems').textContent")
    check("status bar clean after fix", "no problems" in text, text)
    page.load('fn main() {\n    let a: int = 1\n    println(f"{a}")|\n}')
    page.type("\nprintx(")
    page.browser.wait_for("document.getElementById('checking').textContent === ''", 8)
    page.press("Escape")
    check("typo after valid code gets a squiggle", squiggles(page) != [], squiggles(page))


def test_run_shortcut(page):
    page.fresh().type('fn main() {\n    println("hi")')
    page.press("Enter", MOD_CTRL)
    ok = page.browser.wait_for("document.getElementById('output').textContent.includes('hi')", 20)
    check("ctrl enter runs", ok, page.browser.evaluate("document.getElementById('output').textContent"))


def main():
    if Browser is None or find_browser() is None:
        print("SKIP browser tests: chromium or the websockets module is missing")
        return 0
    port = free_port()
    runtime_dir = tempfile.mkdtemp(prefix="firn-playground-test-", dir="/var/tmp")
    server = start_server(port, runtime_dir, 5)
    browser = Browser(find_browser())
    try:
        page = Page(browser, f"http://127.0.0.1:{port}")
        for step in (test_auto_close, test_blocks, test_undo, test_completion, test_highlight, test_live_check, test_run_shortcut):
            step(page)
        shot = os.environ.get("PLAYGROUND_SHOT")
        if shot:
            page.fresh()
            sample = SCREENSHOT_SOURCE
            caret = sample.index("@@")
            page.browser.evaluate(f"(() => {{ const b = document.getElementById('source'); b.focus(); document.execCommand('insertText', false, {json.dumps(sample.replace('@@', ''))}); b.setSelectionRange({caret}, {caret}); }})()")
            page.browser.wait_for("document.querySelectorAll('#highlight .sq-error').length > 0", 8)
            time.sleep(0.2)
            page.press(" ", MOD_CTRL)
            browser.screenshot(shot)
    finally:
        browser.close()
        server.terminate()
        server.wait()
        shutil.rmtree(runtime_dir, ignore_errors=True)
    print(f"{len(FAILURES)} failed")
    return 1 if FAILURES else 0


if __name__ == "__main__":
    sys.exit(main())
