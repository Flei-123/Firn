import { check, finish } from "./harness.mjs";
import { loadStatic } from "./load.mjs";

const { editFor, applyToText } = await loadStatic("editing.js");

function show(state) {
  const { value, start, end } = state;
  if (start === end) return value.slice(0, start) + "|" + value.slice(start);
  return value.slice(0, start) + "[" + value.slice(start, end) + "]" + value.slice(end);
}

function load(text) {
  const open = text.indexOf("[");
  if (open >= 0) {
    const close = text.indexOf("]");
    return { value: text.slice(0, open) + text.slice(open + 1, close) + text.slice(close + 1), start: open, end: close - 1 };
  }
  const caret = text.indexOf("|");
  return { value: text.replace("|", ""), start: caret, end: caret };
}

function press(text, key, mods = {}) {
  const state = load(text);
  const edit = editFor(state, { key, ctrl: false, alt: false, shift: false, ...mods });
  if (edit === null) {
    if (key.length !== 1) return "native";
    const typed = { value: state.value.slice(0, state.start) + key + state.value.slice(state.end), start: state.start + 1, end: state.start + 1 };
    return show(typed);
  }
  return show(applyToText(state, edit));
}

check("brace pairs", press("a |", "{"), "a {|}");
check("paren pairs", press("f|", "("), "f(|)");
check("bracket pairs", press("|", "["), "[|]");
check("quote pairs", press("let s = |", "\""), "let s = \"|\"");
check("no pair before word", press("|abc", "("), "(|abc");
check("quote inside word not doubled", press("it|", "\""), "it\"|");
check("quote inside string not doubled", press("\"ab|\"", "\""), "\"ab\"|");
check("f quote pairs", press("println(f|)", "\""), "println(f\"|\")");
check("quote inside open string not doubled", press("\"a| b", "\""), "\"a\"| b");
check("skip closing paren", press("f(x|)", ")"), "f(x)|");
check("skip closing brace", press("{|}", "}"), "{}|");
check("skip closing quote", press("\"a|\"", "\""), "\"a\"|");
check("closer without partner inserted", press("a|", ")"), "a)|");
check("wrap selection in parens", press("x = [a + b]", "("), "x = ([a + b])");
check("wrap selection in quotes", press("[hi]", "\""), "\"[hi]\"");
check("backspace deletes pair", press("f(|)", "Backspace"), "f|");
check("backspace deletes quote pair", press("\"|\"", "Backspace"), "|");
check("backspace plain is native", press("ab|", "Backspace"), "native");
check("backspace non pair is native", press("(a|)", "Backspace"), "native");
check("enter keeps indent", press("    let x = 1|", "Enter"), "    let x = 1\n    |");
check("enter after brace indents", press("fn main() {|", "Enter"), "fn main() {\n    |");
check("enter between braces", press("fn main() {|}", "Enter"), "fn main() {\n    |\n}");
check("enter between braces nested indent", press("    if x {|}", "Enter"), "    if x {\n        |\n    }");
check("tab inserts indent", press("a|", "Tab"), "a    |");
check("tab indents selected lines", press("[a\nb\nc]", "Tab"), "[    a\n    b\n    c]");
check("tab skips empty lines", press("[a\n\nc]", "Tab"), "[    a\n\n    c]");
check("tab on partial selection lines", press("a[a\nb]b\nc", "Tab"), "    a[a\n    b]b\nc");
check("shift tab outdents selection", press("[    a\n        b\nc]", "Tab", { shift: true }), "[a\n    b\nc]");
check("shift tab outdents current line", press("    a|b", "Tab", { shift: true }), "a|b");
check("shift tab with short indent", press("  a|", "Tab", { shift: true }), "a|");
check("shift tab nothing to remove", press("a|", "Tab", { shift: true }), "a|");
check("selection ending at line start excludes that line", press("[a\nb\n]c", "Tab"), "[    a\n    b\n]c");
check("comment single line", press("  let x|", "/", { ctrl: true }), "  // let x|");
check("uncomment single line", press("  // let x|", "/", { ctrl: true }), "  let x|");
check("comment multiple lines", press("[a\n  b]", "/", { ctrl: true }), "[// a\n//   b]");
check("uncomment multiple lines", press("[// a\n//   b]", "/", { ctrl: true }), "[a\n  b]");
check("comment skips blank lines", press("[a\n\nb]", "/", { ctrl: true }), "[// a\n\n// b]");
check("mixed lines get commented", press("[// a\nb]", "/", { ctrl: true }), "[// // a\n// b]");
check("duplicate line", press("a|b\nc", "d", { ctrl: true }), "ab\na|b\nc");
check("duplicate selected lines", press("[a\nb]\nc", "d", { ctrl: true }), "a\nb\n[a\nb]\nc");
check("move line up", press("a\nb|c", "ArrowUp", { alt: true }), "b|c\na");
check("move line down", press("a|b\nc", "ArrowDown", { alt: true }), "c\na|b");
check("move first line up is noop", press("a|\nb", "ArrowUp", { alt: true }), "native");
check("move last line down is noop", press("a\nb|", "ArrowDown", { alt: true }), "native");
check("move selected lines down", press("[a\nb]\nc\nd", "ArrowDown", { alt: true }), "c\n[a\nb]\nd");
check("delete line middle", press("a\nb|b\nc", "k", { ctrl: true, shift: true }), "a\nc|");
check("delete last line", press("a\nb|", "k", { ctrl: true, shift: true }), "a|");
check("delete only line", press("ab|", "k", { ctrl: true, shift: true }), "|");
check("delete selected lines", press("a\n[b\nc]\nd", "K", { ctrl: true, shift: true }), "a\n|d");
check("home goes to indent first", press("    ab|c", "Home"), "    |abc");
check("home toggles to column zero", press("    |abc", "Home"), "|    abc");
check("home from column zero goes to indent", press("|    abc", "Home"), "    |abc");
check("shift home extends", press("    ab|c", "Home", { shift: true }), "    [ab]c");
check("ctrl enter is not handled", press("a|", "Enter", { ctrl: true }), "native");
check("plain letter is native insert", press("a|", "x"), "ax|");
finish();
