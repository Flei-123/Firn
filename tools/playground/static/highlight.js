const KEYWORDS = new Set([
  "fn", "let", "var", "if", "else", "while", "return", "struct", "const", "static", "profile", "as", "mut",
  "syscall", "extern", "break", "defer", "errdefer", "comptime", "continue", "for", "in", "import", "export",
  "enum", "match", "error", "try", "catch", "interface", "impl", "where", "type", "pub",
]);
const LITERALS = new Set(["true", "false"]);
const TYPES = new Set([
  "int", "uint", "long", "ulong", "byte", "double", "float", "string", "char", "bool", "str", "void", "octet",
  "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "usize", "isize", "f32", "f64",
  "Vec", "Map", "Set",
]);

const TOKEN = new RegExp([
  "(?<line>//[^\\n]*)",
  "(?<open>/\\*)",
  "(?<fstr>f\"(?:[^\"\\\\\\n]|\\\\.)*\"?)",
  "(?<str>\"(?:[^\"\\\\\\n]|\\\\.)*\"?)",
  "(?<chr>'(?:[^'\\\\\\n]|\\\\.)*'?)",
  "(?<num>0[xX][0-9A-Fa-f_]+|0[bB][01_]+|[0-9][0-9_]*(?:\\.[0-9][0-9_]*)?(?:[eE][+-]?[0-9]+)?(?:[iu](?:8|16|32|64)|usize|isize|f32|f64|f)?)",
  "(?<word>[A-Za-z_][A-Za-z0-9_]*)",
].join("|"), "y");

function escapeHtml(text) {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function span(kind, text) {
  return `<span class="tok-${kind}">${escapeHtml(text)}</span>`;
}

function classifyWord(word, rest, previous) {
  if (KEYWORDS.has(word)) return "kw";
  if (LITERALS.has(word)) return "lit";
  if (TYPES.has(word)) return "type";
  if (previous === "fn") return "fn";
  if (/^\s*\(/.test(rest)) return "call";
  if (/^[A-Z]/.test(word)) return "type";
  return null;
}

function skipBlockComment(line, from, depth) {
  let index = from;
  while (index < line.length && depth > 0) {
    if (line.startsWith("/*", index)) { depth += 1; index += 2; }
    else if (line.startsWith("*/", index)) { depth -= 1; index += 2; }
    else index += 1;
  }
  return { end: index, depth };
}

export function highlightLine(line, state) {
  let html = "";
  let index = 0;
  let previous = "";
  if (state.depth > 0) {
    const skipped = skipBlockComment(line, 0, state.depth);
    html += span("comment", line.slice(0, skipped.end));
    state.depth = skipped.depth;
    index = skipped.end;
  }
  while (index < line.length) {
    TOKEN.lastIndex = index;
    const match = TOKEN.exec(line);
    if (!match) {
      html += escapeHtml(line[index]);
      index += 1;
      continue;
    }
    const text = match[0];
    const kind = Object.keys(match.groups).find((name) => match.groups[name] !== undefined);
    if (kind === "open") {
      const comment = skipBlockComment(line, index + 2, 1);
      html += span("comment", line.slice(index, comment.end));
      state.depth = comment.depth;
      index = comment.end;
      continue;
    }
    if (kind === "word") {
      const cls = classifyWord(text, line.slice(index + text.length), previous);
      html += cls ? span(cls, text) : escapeHtml(text);
      previous = text;
    } else {
      html += span(kind === "line" ? "comment" : kind === "fstr" || kind === "str" || kind === "chr" ? "str" : "num", text);
      previous = "";
    }
    index += text.length;
  }
  return html;
}

export function highlightSource(source, errorLines) {
  const state = { depth: 0 };
  return source.split("\n").map((line, number) => {
    const cls = errorLines.has(number + 1) ? "line err" : "line";
    return `<div class="${cls}">${highlightLine(line, state) || " "}</div>`;
  }).join("");
}
