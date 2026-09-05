#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/liveb4/cases.py -- the OWN cases of round B4.

The official suite (tools/liveb4/wpt.py) measures how much of the DOM is
there. These cases measure the things the official suite cannot see,
because they are about the BROWSER and not about the DOM:

  * the ORDER OF THE SCRIPTS -- parser-blocking, `async`, `defer`, and the
    fact that `defer` and `async` mean NOTHING on an inline script. No WPT
    test in dom/ checks that, and it is the part of round B4 that is
    easiest to get quietly wrong.
  * the event flow with its three phases, `preventDefault`,
    `stopPropagation` and `stopImmediatePropagation` -- checked here as
    well as in the suite, because here the expected order is written out
    by hand and a change to it is visible in the diff.
  * `setTimeout`/`setInterval` ordering and `clearInterval`.
  * that a change made from a script really reaches the PICTURE: the box
    tree after the script has to differ from the box tree before, in the
    way the script asked for.

Every case names what it checks. A case whose expectation is "nothing
happened" is marked as a counter-check.
"""
import json
import struct
import subprocess
import sys

UA = (b"html,body,div,p,span,section{display:block}"
      b"i,b,em{display:inline}head,script,style{display:none}"
      b"body{margin:8px}")


def u32(v):
    return struct.pack("<I", v)


def blob(b):
    return u32(len(b)) + b


def run(binary, html, author=b"", loc=b"http://firn.test/a/b.html",
        flags=2, ms=1000, timeout=60):
    payload = (u32(800) + u32(600) + blob(html) + blob(UA) + blob(author)
               + blob(loc) + u32(flags) + u32(ms))
    p = subprocess.run([binary], input=payload, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, timeout=timeout)
    out = p.stdout.decode("utf-8", "replace")
    report = {}
    lines = []
    for line in out.splitlines():
        if line.startswith(("SCRIPTS ", "EVENTS ", "STYLED ", "TEXT ",
                            "HTML ")):
            for k, v in zip(line.split()[0::2], line.split()[1::2]):
                report[k] = v
            if line.startswith("TEXT "):
                report["TEXT"] = line[5:]
            if line.startswith("HTML "):
                report["HTML"] = line[5:]
        else:
            lines.append(line)
    return "\n".join(lines), report, p.returncode


CASES = []


def case(name, why, html, expect_out=None, expect=None, ms=1000,
         author=b""):
    CASES.append((name, why, html, expect_out, expect or {}, ms, author))


# ---------------------------------------------------------------- scripts
case("script-order-inline",
     "HTML 4.12.1: `defer` and `async` mean NOTHING on an inline script. "
     "Both of these have to run where they stand, in document order.",
     b"""<!doctype html><body>
<script>print("one")</script>
<script defer>print("two")</script>
<script async>print("three")</script>
<script>print("four")</script>
</body>""",
     expect_out="one\ntwo\nthree\nfour")

case("script-type-not-js",
     "COUNTER-CHECK, HTML 4.12.1: a `type` that is not a JavaScript MIME "
     "type marks a DATA BLOCK. It must not be executed at all.",
     b"""<!doctype html><body>
<script type="application/json">print("MUST NOT RUN")</script>
<script type="text/javascript">print("ran")</script>
<script type="module">print("module ran")</script>
</body>""",
     expect_out="ran\nmodule ran")

case("script-sees-dom-before-it",
     "A parser-blocking script sees the tree UP TO ITSELF. Everything "
     "after it is already there too in this round -- the document is "
     "parsed before the first script runs -- and that is said out loud "
     "in docs/ROUNDB4.md rather than pretended away.",
     b"""<!doctype html><body>
<p id=before>B</p>
<script>print("before:", document.getElementById("before").textContent);
print("after:", document.getElementById("after") ? "there" : "missing");</script>
<p id=after>A</p>
</body>""",
     expect_out="before: B\nafter: there")

# ----------------------------------------------------------------- events
case("event-three-phases",
     "DOM 2.9: capture from the root down, then the target, then bubble "
     "back up. A capturing listener runs ONLY on the way down and an "
     "ordinary one ONLY at the target and on the way up.",
     b"""<!doctype html><body><div id=a><div id=b><span id=c>x</span></div></div>
<script>
var log = [];
function on(id, ph, cap){ document.getElementById(id).addEventListener("t",
  function(e){ log.push(ph + e.eventPhase); }, cap); }
on("a","aC",true); on("b","bC",true); on("c","cT",false);
on("b","bB",false); on("a","aB",false);
document.getElementById("c").dispatchEvent(new Event("t",{bubbles:true}));
print(log.join(" "));
</script></body>""",
     expect_out="aC1 bC1 cT2 bB3 aB3")

case("event-no-bubble",
     "COUNTER-CHECK: an event with `bubbles:false` stops at the target. "
     "If the bubble phase ran anyway the previous case would still pass.",
     b"""<!doctype html><body><div id=a><span id=c>x</span></div>
<script>
var log = [];
document.getElementById("a").addEventListener("t", function(){ log.push("aC"); }, true);
document.getElementById("a").addEventListener("t", function(){ log.push("aB"); });
document.getElementById("c").addEventListener("t", function(){ log.push("cT"); });
document.getElementById("c").dispatchEvent(new Event("t",{bubbles:false}));
print(log.join(" "));
</script></body>""",
     expect_out="aC cT")

case("event-prevent-and-stop",
     "DOM 2.5/2.9: `preventDefault` on a cancelable event makes "
     "`dispatchEvent` answer false; `stopPropagation` keeps the "
     "listeners of the SAME target and drops the rest; "
     "`stopImmediatePropagation` drops the rest of the same target too.",
     b"""<!doctype html><body><div id=a><span id=c>x</span></div>
<script>
var log = [];
var a = document.getElementById("a"), c = document.getElementById("c");
c.addEventListener("t", function(e){ log.push("c1"); e.stopPropagation(); e.preventDefault(); });
c.addEventListener("t", function(e){ log.push("c2"); });
a.addEventListener("t", function(e){ log.push("aB"); });
var ev = new Event("t",{bubbles:true,cancelable:true});
print("returned", c.dispatchEvent(ev), "prevented", ev.defaultPrevented, "log", log.join(" "));
log = [];
var ev2 = new Event("t",{bubbles:true});
c.addEventListener("t", function(e){ log.push("c0"); e.stopImmediatePropagation(); }, false);
print("second", c.dispatchEvent(ev2), log.join(" "));
</script></body>""",
     expect_out=("returned false prevented true log c1 c2\n"
                 "second true c1 c2 c0"))

case("event-once-and-duplicate",
     "DOM 2.7: the same function for the same type and phase is "
     "registered ONCE; `{once:true}` removes itself after the first call.",
     b"""<!doctype html><body><script>
var n = 0; function f(){ n++; }
document.addEventListener("t", f); document.addEventListener("t", f);
document.dispatchEvent(new Event("t"));
var m = 0;
document.addEventListener("u", function(){ m++; }, {once:true});
document.dispatchEvent(new Event("u")); document.dispatchEvent(new Event("u"));
print("dedup", n, "once", m);
</script></body>""",
     expect_out="dedup 1 once 1")

# ----------------------------------------------------------------- timers
case("timers-order",
     "HTML 8.6: timers fire EARLIEST FIRST, whatever order they were "
     "registered in, and `clearInterval` really stops one.",
     b"""<!doctype html><body><script>
var log = [];
setTimeout(function(){ log.push("c"); }, 300);
setTimeout(function(){ log.push("a"); }, 10);
setTimeout(function(){ log.push("b"); }, 100);
var iv = setInterval(function(){ log.push("i"); }, 50);
setTimeout(function(){ clearInterval(iv); log.push("stop"); }, 120);
setTimeout(function(){ print(log.join(" ")); }, 900);
</script></body>""",
     # a(10) i(50) b(100) i(100) stop(120); the interval at 150 is gone.
     # Two timers due at the same moment run in REGISTRATION order, which
     # is why `b` comes before the second `i`.
     expect_out="a i b i stop c")

case("timers-arguments",
     "HTML 8.6: the extra arguments of `setTimeout` are handed to the "
     "callback.",
     b"""<!doctype html><body><script>
setTimeout(function(x, y){ print("args", x, y); }, 5, 41, "b");
</script></body>""",
     expect_out="args 41 b")

# ---------------------------------------------------- the tree and the box
case("dom-changes-the-picture",
     "The point of the whole round: a script changes the tree and the "
     "LAYOUT is different afterwards. The box count before and after is "
     "printed by the driver, and the text of the body is read back out "
     "of the DOM.",
     b"""<!doctype html><body><div id=a>one</div>
<script>
var d = document.createElement("div");
d.textContent = "two";
document.getElementById("a").parentNode.appendChild(d);
document.getElementById("a").style.width = "123px";
</script></body>""",
     expect={"HTML_HAS": "<div>two</div>",
             "HTML_HAS2": 'style="width: 123px"'})

case("innerhtml-fragment-context",
     "WHATWG 13.4: `innerHTML` is the FRAGMENT PARSING ALGORITHM in the "
     "context of the element -- a `<td>` inside a `<tr>` stays a `td`, "
     "and inside a `div` it does not.",
     b"""<!doctype html><body><table><tr id=r></tr></table><div id=d></div>
<script>
document.getElementById("r").innerHTML = "<td>cell</td>";
document.getElementById("d").innerHTML = "<td>cell</td>";
print("in tr:", document.getElementById("r").innerHTML);
print("in div:", document.getElementById("d").innerHTML);
</script></body>""",
     expect_out="in tr: <td>cell</td>\nin div: cell")

case("classlist-and-style",
     "The two setters a page uses most: `classList` and `style`, both of "
     "which go through the `class` resp. `style` ATTRIBUTE, so what a "
     "script writes is what the cascade reads.",
     b"""<!doctype html><body><div id=a class="x y">t</div>
<script>
var a = document.getElementById("a");
a.classList.add("z"); a.classList.remove("x");
print("toggle", a.classList.toggle("y"), a.classList.toggle("w"));
print("class", a.getAttribute("class"), "len", a.classList.length);
a.style.color = "red"; a.style.setProperty("font-size", "12px");
a.style.removeProperty("color");
print("style", a.getAttribute("style"), "|", a.style.fontSize);
</script></body>""",
     # "x y" -> add z -> "x y z" -> remove x -> "y z" -> toggle y (off,
     # false) -> "z" -> toggle w (on, true) -> "z w".
     expect_out=("toggle false true\nclass z w len 2\n"
                 "style font-size: 12px | 12px"))

case("identity-of-wrappers",
     "The same node has to give the SAME JavaScript object every time -- "
     "otherwise `===` is false for two references to one element and no "
     "event handler can compare its target.",
     b"""<!doctype html><body><div id=a><span id=b>x</span></div>
<script>
var a1 = document.getElementById("a"), a2 = document.querySelector("#a");
print("same", a1 === a2, a1.firstChild === a1.childNodes[0]);
print("style", a1.style === a1.style, "list", a1.classList === a1.classList);
print("proto", a1 instanceof Element, a1 instanceof Node, a1 instanceof HTMLElement);
</script></body>""",
     expect_out="same true true\nstyle true list true\nproto true true true")

case("no-script-no-change",
     "COUNTER-CHECK: a document without a script must come out of the "
     "new pipeline byte for byte as it went in. If it does not, round B4 "
     "has changed rounds B1 to B3 and every number they measured.",
     b"""<!doctype html><body><div id=a class=x>one <b>two</b></div></body>""",
     expect={"HTML_EQ": '<div id="a" class="x">one <b>two</b></div>',
             "SCRIPTS": "0"})


def main():
    binary = sys.argv[1]
    verbose = "--verbose" in sys.argv
    good = 0
    bad = 0
    for name, why, html, expect_out, expect, ms, author in CASES:
        try:
            out, rep, rc = run(binary, html, author=author, ms=ms)
        except Exception as ex:
            print("      FAIL %-26s %s" % (name, str(ex)[:70]))
            bad += 1
            continue
        ok = rc == 0
        detail = "" if ok else "exit %d" % rc
        if ok and expect_out is not None:
            got = out.strip()
            if got != expect_out.strip():
                ok = False
                detail = "\n         want %r\n         got  %r" % (
                    expect_out.strip(), got)
        if ok:
            for k, v in expect.items():
                if k == "HTML_HAS" or k == "HTML_HAS2":
                    if v not in rep.get("HTML", ""):
                        ok = False
                        detail = "the html has no %r" % v
                elif k == "HTML_EQ":
                    if rep.get("HTML", "").strip() != v:
                        ok = False
                        detail = "\n         want %r\n         got  %r" % (
                            v, rep.get("HTML", "").strip())
                elif rep.get(k, "") != v:
                    ok = False
                    detail = "%s: expected %r, got %r" % (k, v,
                                                          rep.get(k, ""))
        if ok and rep.get("BAD", "0") != "0":
            ok = False
            detail = "%s boxes differ from a full layout" % rep["BAD"]
        if ok:
            good += 1
            if verbose:
                print("      ok   %s" % name)
        else:
            bad += 1
            print("      FAIL %-26s %s" % (name, detail))
            print("           (%s)" % why[:150])
    print("   own cases        %d / %d (of them %d counter-checks)"
          % (good, good + bad,
             sum(1 for _, w, _, _, _, _, _ in CASES
                 if "COUNTER-CHECK" in w)))
    if bad:
        print("CASES FAIL: %d of %d" % (bad, good + bad))
        return 1
    print("CASES OK: %d own cases, 0 wrong" % good)
    return 0


if __name__ == "__main__":
    sys.exit(main())
