import { check, finish } from "./harness.mjs";
import { loadStatic } from "./load.mjs";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const { symbolsOf, contextAt, suggest } = await loadStatic("completion.js");
const here = dirname(fileURLToPath(import.meta.url));
const data = JSON.parse(readFileSync(join(here, "..", "static", "completions.json"), "utf8"));

function labels(source, explicit = false) {
  const caret = source.indexOf("|");
  const text = source.replace("|", "");
  return suggest(contextAt(text, caret), data, symbolsOf(text), explicit).map((item) => item.label);
}

check("keyword prefix", labels("le|").slice(0, 1), ["let"]);
check("prelude function", labels("printl|").slice(0, 1), ["println"]);
check("module names", labels("mat|").includes("math"), true);
check("module members after dot", labels("math.s|").includes("sqrt"), true);
check("module members need dot", labels("math.|").length > 10, true);
check("fs module", labels("fs.read_|").includes("read_text"), true);
check("vec methods by declared type", labels("let v: Vec[int] = Vec[int].new()\nv.p|").slice(0, 2), ["pop", "push"]);
check("vec methods by constructor", labels("var v = Vec[int].new()\nv.|").includes("push"), true);
check("str methods by literal", labels("\"abc\".tr|").includes("trim"), true);
check("str methods by variable", labels("let s: str = \"x\"\ns.spl|").includes("split"), true);
check("number methods", labels("let n: i32 = 4\nn.to_|").includes("to_str"), true);
check("unknown receiver offers all methods", labels("x.pu|").includes("push"), true);
check("file symbols", labels("fn compute(a: int) -> int { return a }\nfn main() {\n    comp|").slice(0, 1), ["compute"]);
check("parameters visible", labels("fn f(width: int) {\n    wid|").includes("width"), true);
check("variables visible", labels("fn main() {\n    let counter = 1\n    cou|").slice(0, 1), ["counter"]);
check("struct fields after dot", labels("struct P {\n    x: int,\n    y: int,\n}\nfn main() {\n    let p = P { x: 1, y: 2 }\n    p.|").slice(0, 2), ["x", "y"]);
check("impl methods after dot", labels("struct P {\n    x: int,\n}\nimpl P {\n    fn area(*self) -> int { return 1 }\n}\nfn main() {\n    let p = P { x: 1 }\n    p.ar|").slice(0, 1), ["area"]);
check("empty prefix needs explicit", labels("|"), []);
check("empty prefix explicit lists all", labels("|", true).length > 10, true);
check("range dots are not member access", contextAt("0..", 3).member, false);
check("no match is empty", labels("zzzqq|"), []);
check("data has std modules", ["math", "time", "fs", "chars", "io"].every((name) => name in data.modules), true);
check("data keywords match lsp", data.keywords.includes("fn") && data.keywords.includes("catch"), true);
finish();
