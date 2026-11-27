from cdp import MOD_CTRL
from session_test import EOF_SOURCE, PROMPT_SOURCE, SPIN_SOURCE

OUTPUT_JS = "document.getElementById('output').textContent"
LABEL_JS = "document.getElementById('run-label').textContent"
FOCUS_JS = "document.activeElement && document.activeElement.id"
EXIT_JS = "(document.querySelector('#output .term-exit') || {}).textContent || ''"


def run_program(page, source):
    page.fresh()
    page.load(source + "|")
    page.press("Enter", MOD_CTRL)


def wait_output(page, needle, timeout=20):
    return page.browser.wait_for(f"{OUTPUT_JS}.includes({needle!r})", timeout)


def wait_exit(page, needle, timeout=15):
    return page.browser.wait_for(f"{EXIT_JS}.includes({needle!r})", timeout)


def test_interactive(page, check):
    run_program(page, PROMPT_SOURCE)
    check("prompt shows before input", wait_output(page, "Name: "), page.browser.evaluate(OUTPUT_JS))
    check("run button turns into stop", "Stop" in page.browser.evaluate(LABEL_JS), page.browser.evaluate(LABEL_JS))
    check("terminal input has focus", page.browser.evaluate(FOCUS_JS) == "term-input", page.browser.evaluate(FOCUS_JS))
    page.type("Ada")
    typed = page.browser.evaluate("[...document.querySelectorAll('#output .term-live .term-in')].map(e => e.textContent).join('')")
    check("typed text is echoed live", typed.strip() == "Ada", repr(typed))
    page.press("Enter")
    check("second prompt after enter", wait_output(page, "Age: "), page.browser.evaluate(OUTPUT_JS))
    page.type("36\n")
    check("program answered", wait_output(page, "Ada is 36"), page.browser.evaluate(OUTPUT_JS))
    check("input is part of the scrollback", "Name: Ada" in page.browser.evaluate(OUTPUT_JS), page.browser.evaluate(OUTPUT_JS))
    check("exit line shown", wait_exit(page, "exited with code 0"), page.browser.evaluate(EXIT_JS))
    check("run button back to run", "Run" in page.browser.evaluate(LABEL_JS), page.browser.evaluate(LABEL_JS))


def test_backspace_and_cursor(page, check):
    run_program(page, PROMPT_SOURCE)
    wait_output(page, "Name: ")
    page.type("Adx")
    page.press("Backspace")
    page.type("a\n")
    check("backspace edits the line", wait_output(page, "Ada") and "Name: Ada" in page.browser.evaluate(OUTPUT_JS), page.browser.evaluate(OUTPUT_JS))
    page.press("d", MOD_CTRL)
    check("eof on a finished run is harmless", True)


def test_eof(page, check):
    run_program(page, EOF_SOURCE)
    wait_output(page, "ready")
    page.type("abc\n")
    page.press("d", MOD_CTRL)
    check("ctrl d ends input", wait_output(page, "got 4") and wait_exit(page, "code 0"), page.browser.evaluate(OUTPUT_JS))


def test_interrupt(page, check):
    run_program(page, SPIN_SOURCE)
    wait_output(page, "spinning")
    page.press("c", MOD_CTRL)
    check("ctrl c stops the program", wait_exit(page, "Stopped"), page.browser.evaluate(OUTPUT_JS))


def test_stop_button(page, check):
    run_program(page, SPIN_SOURCE)
    wait_output(page, "spinning")
    page.browser.evaluate("document.getElementById('run').click()")
    check("stop button stops the program", wait_exit(page, "Stopped"), page.browser.evaluate(OUTPUT_JS))
    check("label is run again", "Run" in page.browser.evaluate(LABEL_JS), page.browser.evaluate(LABEL_JS))


def test_compile_error(page, check):
    run_program(page, 'fn main() {\n    let x: int = "a"\n}\n')
    found = page.browser.wait_for("document.querySelectorAll('#output .diag').length > 0", 20)
    check("compile error still lists clickable diagnostics", found, page.browser.evaluate(OUTPUT_JS))


def test_restart_from_terminal(page, check):
    run_program(page, PROMPT_SOURCE)
    wait_output(page, "Name: ")
    page.type("Bob\n")
    wait_output(page, "Age: ")
    page.press("Enter", MOD_CTRL)
    page.browser.wait_for(f"!{OUTPUT_JS}.includes('Bob')", 10)
    ok = wait_output(page, "Name: ") and "Bob" not in page.browser.evaluate(OUTPUT_JS)
    check("ctrl enter in the terminal restarts the run", ok, page.browser.evaluate(OUTPUT_JS))
    page.press("c", MOD_CTRL)


def run_all(page, check):
    for step in (test_interactive, test_backspace_and_cursor, test_eof, test_interrupt, test_stop_button, test_compile_error, test_restart_from_terminal):
        step(page, check)
