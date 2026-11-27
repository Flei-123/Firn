import { check, finish } from "./harness.mjs";
import { loadStatic } from "./load.mjs";

const { tokenizeLine, tokenizeSource } = await loadStatic("tokenizer.js");

function marked(line) {
  return tokenizeLine(line, { depth: 0 }).filter((token) => token.kind !== "").map((token) => `${token.kind}:${token.text}`);
}

function all(line) {
  return tokenizeLine(line, { depth: 0 }).map((token) => `${token.kind || "-"}:${token.text}`);
}

check("fstring call inside braces", all('f"Sum = {test(1,2,3)}"'), [
  'str:f"Sum = ', "interp:{", "call:test", "-:(", "num:1", "-:,", "num:2", "-:,", "num:3", "-:)", "interp:}", 'str:"',
]);
check("fstring escaped braces stay string", all('f"{{x}} {y}"'), ['str:f"', "esc:{{", "str:x", "esc:}}", "str: ", "interp:{", "var:y", "interp:}", 'str:"']);
check("fstring operators and numbers", marked('f"{a + 1}"'), ['str:f"', "interp:{", "var:a", "op:+", "num:1", "interp:}", 'str:"']);
check("fstring nested string", marked('f"[{pick("a", 0)}] x"'), [
  'str:f"[', "interp:{", "call:pick", 'str:"a"', "num:0", "interp:}", 'str:] x"',
]);
check("fstring nested fstring", marked('f"{g(f"{h(1)}")}"').filter((item) => item.startsWith("call:")), ["call:g", "call:h"]);
check("fstring format spec", all('f"{n:>8}"'), ['str:f"', "interp:{", "var:n", "-::", "spec:>8", "interp:}", 'str:"']);
check("fstring struct literal braces have no spec", marked('f"{P { x: 1 }}"').some((item) => item.startsWith("spec")), false);
check("fstring plain after code", all('f"{a}b"').slice(-3), ["var:a", "interp:}", 'str:b"']);
check("unterminated fstring", all('f"{a'), ['str:f"', "interp:{", "var:a"]);
check("plain string with braces and escape", all('"a {b} \\n"'), ['str:"a {b} ', "esc:\\n", 'str:"']);
check("definition and types", marked("fn test(a: int) -> int {"), ["kw:fn", "fn:test", "var:a", "type:int", "op:->", "type:int"]);
check("method calls", marked("x.to_str().len()"), ["var:x", "call:to_str", "call:len"]);
check("push call", marked("v.push(1)"), ["var:v", "call:push", "num:1"]);
check("fields and variables are var", marked("p.x + p.y"), ["var:p", "var:x", "op:+", "var:p", "var:y"]);
check("generic call", marked("vec_new[T]()"), ["call:vec_new", "type:T"]);
check("types and static call", marked("let v: Vec[int] = Vec[int].new()").filter((item) => /^(type|call)/.test(item)), ["type:Vec", "type:int", "type:Vec", "type:int", "call:new"]);
check("variadic type", marked("fn sum(xs: ..int)"), ["kw:fn", "fn:sum", "var:xs", "op:..", "type:int"]);
check("cast", marked("x as u8"), ["var:x", "kw:as", "type:u8"]);
check("tuple type", marked('let t: (int, str) = (1, "a")').filter((item) => item.startsWith("type")), ["type:int", "type:str"]);
check("attribute", marked("#[no_inline]"), ["attr:#[no_inline]"]);
check("hex and suffix numbers", marked("0xFFu8 0b101 1_000i64 3.5e2 7usize"), ["num:0xFFu8", "num:0b101", "num:1_000i64", "num:3.5e2", "num:7usize"]);
check("range keeps ints", marked("0..10"), ["num:0", "op:..", "num:10"]);
check("line comment", marked('x // note f"{a}"'), ["var:x", 'comment:// note f"{a}"']);
check("comment marker inside string", marked('"//x"'), ['str:"//x"']);
check("block comment spans lines", tokenizeSource("a /* x\ny */ b").map((line) => line.map((token) => token.kind || "-")), [["var", "-", "comment"], ["comment", "-", "var"]]);
check("nested block comment", tokenizeSource("/* a /* b */ c */ d")[0].map((token) => token.kind || "-"), ["comment", "-", "var"]);
check("keyword after dot is member", marked("x.type"), ["var:x", "var:type"]);
check("constants and types", marked("PI + Point"), ["const:PI", "op:+", "type:Point"]);
check("char literal", marked("'a' + '\\n'"), ["str:'a'", "op:+", "str:'\\n'"]);
check("println is call", marked("println(1)"), ["call:println", "num:1"]);
check("user type name before paren is call", marked("Point(1)"), ["call:Point", "num:1"]);
check("bool literals", marked("true && false"), ["lit:true", "op:&&", "lit:false"]);
check("control keywords", marked("if x { return } for i in v { break }").filter((item) => item.startsWith("ctl")), ["ctl:if", "ctl:return", "ctl:for", "ctl:in", "ctl:break"]);
check("var is control colored", marked("var n = 1"), ["ctl:var", "var:n", "op:=", "num:1"]);
check("declaration keywords", marked("let a = 1").slice(0, 1), ["kw:let"]);
check("struct name is type", marked("struct Game {"), ["kw:struct", "type:Game"]);
finish();
