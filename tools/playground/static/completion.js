const MAX_ITEMS = 24;
const IDENTIFIER_END = /[A-Za-z_][A-Za-z0-9_]*$/;
const TYPE_GROUPS = {
  Vec: "Vec", Map: "Map", Set: "Set", str: "str", string: "str", Span: "str", bool: "bool",
  i8: "int", i16: "int", i32: "int", i64: "int", u8: "int", u16: "int", u32: "int", u64: "int", usize: "int", isize: "int", int: "int", uint: "int", long: "int", ulong: "int", byte: "int",
  f32: "float", f64: "float", float: "float", double: "float",
};
const SOURCE_PATTERNS = {
  fn: /^\s*(?:pub\s+|export\s+)?fn\s+([A-Za-z_]\w*)\s*(?:\[[^\]]*\])?\s*\(([^)]*)\)?\s*(->\s*[^{]+)?/,
  type: /^\s*(?:pub\s+|export\s+)?(struct|enum|error|interface|type)\s+([A-Za-z_]\w*)/,
  impl: /^\s*impl\s+([A-Za-z_]\w*)/,
  binding: /^\s*(let|var|const|static)\s+(?:mut\s+)?([A-Za-z_]\w*)\s*(?::\s*([^=]+?))?\s*(?:=\s*(.*))?$/,
  loop: /^\s*for\s+([A-Za-z_]\w*)\s+in\b/,
  field: /^\s+([A-Za-z_]\w*)\s*:\s*([^,]+?),?\s*$/,
};

function baseType(text) {
  const match = (text || "").trim().replace(/^\*(?:mut\s+)?/, "").match(/^[A-Za-z_]\w*/);
  return match ? match[0] : "";
}

function initializerType(text) {
  const value = (text || "").trim();
  if (value.startsWith("\"") || value.startsWith("f\"")) return "str";
  if (/^\d[\d_]*\.\d/.test(value)) return "f64";
  if (/^\d/.test(value)) return "int";
  if (value === "true" || value === "false") return "bool";
  const constructed = value.match(/^([A-Za-z_]\w*)\s*(?:\[[^\]]*\])?\s*(?:\{|\.new\b)/);
  return constructed ? constructed[1] : "";
}

function addName(symbols, name, kind, detail, type) {
  if (!symbols.names.has(name)) symbols.names.set(name, { label: name, kind, detail });
  if (type) symbols.varTypes.set(name, type);
}

function addMember(symbols, owner, entry) {
  if (!symbols.members.has(owner)) symbols.members.set(owner, new Map());
  symbols.members.get(owner).set(entry.label, entry);
}

function addParameters(symbols, text) {
  for (const part of text.split(",")) {
    const match = part.match(/^\s*(?:\.\.)?([A-Za-z_]\w*)\s*:\s*(.+?)\s*$/);
    if (match) addName(symbols, match[1], "variable", match[2], baseType(match[2].replace(/^\.\./, "")));
  }
}

function braceDelta(line) {
  const cleaned = line.replace(/"(?:[^"\\]|\\.)*"/g, "").replace(/\/\/.*/, "");
  return (cleaned.match(/{/g) || []).length - (cleaned.match(/}/g) || []).length;
}

export function symbolsOf(source) {
  const symbols = { names: new Map(), members: new Map(), varTypes: new Map() };
  let owner = null;
  let ownerKind = "";
  let depth = 0;
  for (const line of source.split("\n")) {
    const impl = line.match(SOURCE_PATTERNS.impl);
    const declared = line.match(SOURCE_PATTERNS.type);
    if (impl && depth === 0) { owner = impl[1]; ownerKind = "impl"; }
    else if (declared && depth === 0) {
      addName(symbols, declared[2], "type", declared[1] + " " + declared[2]);
      owner = declared[2];
      ownerKind = declared[1];
    }
    const fn = line.match(SOURCE_PATTERNS.fn);
    const binding = line.match(SOURCE_PATTERNS.binding);
    const loop = line.match(SOURCE_PATTERNS.loop);
    if (fn) {
      const detail = `fn ${fn[1]}(${fn[2].trim()})${fn[3] ? " " + fn[3].trim() : ""}`;
      if (owner && depth > 0 && ownerKind === "impl") addMember(symbols, owner, { label: fn[1], kind: "method", detail });
      else addName(symbols, fn[1], "function", detail);
      addParameters(symbols, fn[2]);
    } else if (binding && depth > 0) {
      const type = baseType(binding[3]) || initializerType(binding[4]);
      addName(symbols, binding[2], binding[1] === "const" ? "constant" : "variable", binding[3] ? binding[3].trim() : type, type);
    } else if (binding) {
      addName(symbols, binding[2], "constant", binding[3] ? binding[3].trim() : "", baseType(binding[3]) || initializerType(binding[4]));
    } else if (loop) {
      addName(symbols, loop[1], "variable", "", "");
    } else if (owner && ownerKind !== "impl" && depth === 1) {
      const field = line.match(SOURCE_PATTERNS.field);
      if (field && ownerKind === "struct") addMember(symbols, owner, { label: field[1], kind: "field", detail: field[2] });
    }
    depth += braceDelta(line);
    if (depth <= 0) { depth = 0; if (!impl && !declared) owner = null; }
  }
  return symbols;
}

export function contextAt(value, caret) {
  const before = value.slice(0, caret);
  const prefixMatch = before.match(IDENTIFIER_END);
  const prefix = prefixMatch ? prefixMatch[0] : "";
  const head = before.slice(0, before.length - prefix.length);
  const member = head.endsWith(".") && !head.endsWith("..");
  const context = { prefix, member, receiver: "", literal: "" };
  if (!member) return context;
  const receiverText = head.slice(0, -1);
  if (receiverText.endsWith("\"")) context.literal = "str";
  else if (/\d$/.test(receiverText) && !/[A-Za-z_]\w*$/.test(receiverText)) context.literal = /\.\d/.test(receiverText.slice(-6)) ? "float" : "int";
  const name = receiverText.match(IDENTIFIER_END);
  if (name && !receiverText.slice(0, receiverText.length - name[0].length).endsWith(".")) context.receiver = name[0];
  return context;
}

function matchRank(label, prefix) {
  if (prefix === "") return 3;
  if (label.startsWith(prefix)) return 0;
  const lower = label.toLowerCase();
  const wanted = prefix.toLowerCase();
  if (lower.startsWith(wanted)) return 1;
  return lower.includes(wanted) ? 2 : -1;
}

function rankItems(items, prefix) {
  const ranked = [];
  const seen = new Set();
  for (const item of items) {
    const rank = matchRank(item.label, prefix);
    if (rank < 0 || seen.has(item.label)) continue;
    seen.add(item.label);
    ranked.push({ item, rank, order: ranked.length });
  }
  ranked.sort((a, b) => a.rank - b.rank || a.order - b.order);
  return ranked.slice(0, MAX_ITEMS).map((entry) => entry.item);
}

function entryItem(entry, kind) {
  return { label: entry.name, kind: kind || entry.kind, detail: entry.sig };
}

function methodItems(context, data, symbols) {
  const type = symbols.varTypes.get(context.receiver) || "";
  const group = context.literal || TYPE_GROUPS[type] || "";
  const owned = symbols.members.get(type);
  if (owned) return [...owned.values()];
  if (group && data.methods[group]) return data.methods[group].map((entry) => entryItem(entry, "method"));
  const own = [...symbols.members.values()].flatMap((members) => [...members.values()]);
  const merged = Object.entries(data.methods).flatMap(([name, entries]) => entries.map((entry) => ({ ...entryItem(entry, "method"), detail: `${name}: ${entry.sig}` })));
  return [...own, ...merged];
}

function memberItems(context, data, symbols) {
  const local = symbols.names.has(context.receiver) && symbols.names.get(context.receiver).kind !== "type";
  if (context.receiver && !local && data.modules[context.receiver]) {
    return data.modules[context.receiver].map((entry) => entryItem(entry));
  }
  return methodItems(context, data, symbols);
}

function scopeItems(data, symbols) {
  const keywords = data.keywords.map((word) => ({ label: word, kind: "keyword", detail: "keyword" }));
  const prelude = data.prelude.map((entry) => entryItem(entry));
  const modules = Object.keys(data.modules).map((name) => ({ label: name, kind: "module", detail: `std.${name}` }));
  return [...symbols.names.values(), ...prelude, ...modules, ...keywords];
}

export function suggest(context, data, symbols, explicit) {
  if (context.member) return rankItems(memberItems(context, data, symbols), context.prefix);
  if (context.prefix === "" && !explicit) return [];
  return rankItems(scopeItems(data, symbols), context.prefix);
}
