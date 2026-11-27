export const KEYWORDS = new Set([
  "fn", "let", "var", "if", "else", "while", "return", "struct", "const", "static", "profile", "as", "mut",
  "syscall", "extern", "break", "defer", "errdefer", "comptime", "continue", "for", "in", "import", "export",
  "enum", "match", "error", "try", "catch", "interface", "impl", "where", "type", "pub", "unsafe", "gc",
]);
export const LITERALS = new Set(["true", "false"]);
export const TYPES = new Set([
  "int", "uint", "long", "ulong", "byte", "double", "float", "string", "char", "bool", "str", "void", "octet",
  "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "usize", "isize", "f32", "f64",
  "Vec", "Map", "Set", "Self",
]);
export const CONTROL = new Set([
  "var", "if", "else", "while", "for", "in", "match", "return", "break", "continue", "defer", "errdefer", "try", "catch", "import",
]);
const DECLARATIONS = new Set(["struct", "enum", "error", "interface", "type", "impl"]);

const TOKEN = new RegExp([
  "(?<line>//[^\\n]*)",
  "(?<open>/\\*)",
  "(?<str>\")",
  "(?<chr>'(?:[^'\\\\\\n]|\\\\.)*'?)",
  "(?<attr>#!?\\[[^\\]\\n]*\\]?)",
  "(?<num>0[xX][0-9A-Fa-f_]+(?:[iu](?:8|16|32|64)|usize|isize)?|0[bB][01_]+(?:[iu](?:8|16|32|64)|usize|isize)?|0[oO][0-7_]+|[0-9][0-9_]*(?:\\.[0-9][0-9_]*)?(?:[eE][+-]?[0-9]+)?(?:[iu](?:8|16|32|64)|usize|isize|f32|f64)?)",
  "(?<word>[A-Za-z_][A-Za-z0-9_]*)",
  "(?<op>->|=>|\\.\\.=?|[+\\-*/%=<>!&|^~?]+)",
].join("|"), "y");

const WORD_CHAR = /[A-Za-z0-9_]/;
const ESCAPE_KIND = "esc";

function push(tokens, kind, text) {
  if (!text) return;
  const last = tokens[tokens.length - 1];
  if (last && last.kind === kind && (kind === "" || kind === "str")) last.text += text;
  else tokens.push({ kind, text });
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

function matchingBracket(line, from) {
  let depth = 0;
  for (let index = from; index < line.length; index += 1) {
    if (line[index] === "[") depth += 1;
    else if (line[index] === "]") {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  return -1;
}

function isCallAhead(line, from) {
  let index = from;
  while (line[index] === " " || line[index] === "\t") index += 1;
  if (line[index] === "(") return true;
  if (line[index] === "!" && line[index + 1] === "(") return true;
  if (line[index] !== "[") return false;
  const close = matchingBracket(line, index);
  return close >= 0 && line[close + 1] === "(";
}

function classifyWord(word, line, end, previous) {
  const member = previous === ".";
  if (!member && CONTROL.has(word)) return "ctl";
  if (!member && KEYWORDS.has(word)) return "kw";
  if (!member && LITERALS.has(word)) return "lit";
  if (previous === "fn") return "fn";
  if (DECLARATIONS.has(previous)) return "type";
  if (isCallAhead(line, end)) return "call";
  if (member) return "var";
  if (TYPES.has(word)) return "type";
  if (/^[A-Z][A-Z0-9_]+$/.test(word)) return "const";
  if (/^[A-Z]/.test(word)) return "type";
  return "var";
}

function scanString(line, start, tokens, interpolate, state) {
  push(tokens, "str", line[start]);
  let index = start + 1;
  let text = "";
  const flush = () => { push(tokens, "str", text); text = ""; };
  while (index < line.length) {
    const char = line[index];
    if (char === "\"") {
      text += char;
      flush();
      return index + 1;
    }
    if (char === "\\" && index + 1 < line.length) {
      flush();
      push(tokens, ESCAPE_KIND, line.slice(index, index + 2));
      index += 2;
    } else if (interpolate && (line.startsWith("{{", index) || line.startsWith("}}", index))) {
      flush();
      push(tokens, ESCAPE_KIND, line.slice(index, index + 2));
      index += 2;
    } else if (interpolate && char === "{") {
      flush();
      push(tokens, "interp", "{");
      index = scanCode(line, index + 1, tokens, state, true);
      if (line[index] === "}") {
        push(tokens, "interp", "}");
        index += 1;
      }
    } else {
      text += char;
      index += 1;
    }
  }
  flush();
  return index;
}

function scanFormatSpec(line, from, tokens) {
  let index = from;
  while (index < line.length && line[index] !== "}") index += 1;
  push(tokens, "spec", line.slice(from, index));
  return index;
}

function scanCode(line, from, tokens, state, nested) {
  let index = from;
  let previous = "";
  let braces = 0;
  let groups = 0;
  while (index < line.length) {
    const char = line[index];
    if (nested) {
      if (char === "}" && braces === 0) return index;
      if (char === "{") braces += 1;
      else if (char === "}") braces -= 1;
      else if (char === "(" || char === "[") groups += 1;
      else if (char === ")" || char === "]") groups -= 1;
      else if (char === ":" && braces === 0 && groups === 0) {
        push(tokens, "", ":");
        index = scanFormatSpec(line, index + 1, tokens);
        continue;
      }
    }
    if (char === "f" && line[index + 1] === "\"" && (index === 0 || !WORD_CHAR.test(line[index - 1]))) {
      push(tokens, "str", "f");
      index = scanString(line, index + 1, tokens, true, state);
      previous = "\"";
      continue;
    }
    TOKEN.lastIndex = index;
    const match = TOKEN.exec(line);
    if (!match) {
      push(tokens, "", char);
      if (char !== " " && char !== "\t") previous = char;
      index += 1;
      continue;
    }
    const text = match[0];
    const kind = Object.keys(match.groups).find((name) => match.groups[name] !== undefined);
    if (kind === "str") {
      index = scanString(line, index, tokens, false, state);
      previous = "\"";
      continue;
    }
    if (kind === "open" && !nested) {
      const comment = skipBlockComment(line, index + 2, 1);
      push(tokens, "comment", line.slice(index, comment.end));
      state.depth = comment.depth;
      index = comment.end;
      continue;
    }
    if (kind === "line" && !nested) {
      push(tokens, "comment", text);
      index += text.length;
      continue;
    }
    if (kind === "word") push(tokens, classifyWord(text, line, index + text.length, previous), text);
    else if (kind === "chr") push(tokens, "str", text);
    else if (kind === "open" || kind === "line") push(tokens, "op", text[0]);
    else push(tokens, kind, kind === "open" || kind === "line" ? text[0] : text);
    previous = text;
    index += kind === "open" || kind === "line" ? 1 : text.length;
  }
  return index;
}

export function tokenizeLine(line, state) {
  const tokens = [];
  let index = 0;
  if (state.depth > 0) {
    const skipped = skipBlockComment(line, 0, state.depth);
    push(tokens, "comment", line.slice(0, skipped.end));
    state.depth = skipped.depth;
    index = skipped.end;
  }
  const end = scanCode(line, index, tokens, state, false);
  if (end < line.length) push(tokens, "", line.slice(end));
  return tokens;
}

export function tokenizeSource(source) {
  const state = { depth: 0 };
  return source.split("\n").map((line) => tokenizeLine(line, state));
}

export function kindBefore(source, offset) {
  const lines = tokenizeSource(source);
  const lineIndex = source.slice(0, offset).split("\n").length - 1;
  const column = offset - (source.lastIndexOf("\n", offset - 1) + 1);
  let position = 0;
  for (const token of lines[lineIndex]) {
    position += token.text.length;
    if (position >= column) return column === 0 ? "" : token.kind;
  }
  return "";
}
