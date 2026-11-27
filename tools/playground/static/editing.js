export const INDENT = "    ";
const PAIRS = { "{": "}", "(": ")", "[": "]", "\"": "\"" };
const CLOSERS = new Set(["}", ")", "]", "\""]);
const WORD_CHAR = /[A-Za-z0-9_]/;
const COMMENT_MARK = "// ";

function lineStart(value, offset) {
  return value.lastIndexOf("\n", offset - 1) + 1;
}

function lineEnd(value, offset) {
  const index = value.indexOf("\n", offset);
  return index < 0 ? value.length : index;
}

function currentIndent(value, offset) {
  return value.slice(lineStart(value, offset), offset).match(/^[ \t]*/)[0];
}

function quoteCount(text) {
  let count = 0;
  for (let index = 0; index < text.length; index += 1) {
    if (text[index] === "\\") index += 1;
    else if (text[index] === "\"") count += 1;
  }
  return count;
}

function insertEdit(state, text, caretOffset) {
  const caret = state.start + caretOffset;
  return { from: state.start, to: state.end, text, start: caret, end: caret };
}

function wrapSelection(state, open) {
  const selected = state.value.slice(state.start, state.end);
  return {
    from: state.start, to: state.end, text: open + selected + PAIRS[open],
    start: state.start + 1, end: state.end + 1,
  };
}

function skipEdit(state) {
  return { move: true, start: state.start + 1, end: state.start + 1 };
}

function shouldPair(state, open) {
  const before = state.value[state.start - 1] || "";
  const after = state.value[state.end] || "";
  if (open === "\"") {
    const linePrefix = state.value.slice(lineStart(state.value, state.start), state.start);
    if (quoteCount(linePrefix) % 2 === 1) return false;
    if (WORD_CHAR.test(before) && !(before === "f" && !WORD_CHAR.test(linePrefix[linePrefix.length - 2] || ""))) return false;
  }
  return !WORD_CHAR.test(after);
}

function typeCharacter(state, key) {
  const collapsed = state.start === state.end;
  if (CLOSERS.has(key) && collapsed && state.value[state.start] === key) return skipEdit(state);
  if (!(key in PAIRS)) return null;
  if (!collapsed) return wrapSelection(state, key);
  if (!shouldPair(state, key)) return null;
  return insertEdit(state, key + PAIRS[key], 1);
}

function backspace(state) {
  if (state.start !== state.end || state.start === 0) return null;
  const closer = PAIRS[state.value[state.start - 1]];
  if (!closer || closer !== state.value[state.start]) return null;
  return { from: state.start - 1, to: state.start + 1, text: "", start: state.start - 1, end: state.start - 1 };
}

function enter(state) {
  const { value, start } = state;
  const indent = currentIndent(value, start);
  const before = value.slice(lineStart(value, start), start).trimEnd();
  const opens = /[{(\[]$/.test(before);
  if (opens && value[state.end] === PAIRS[before[before.length - 1]] && state.start === state.end && value[start - 1] === before[before.length - 1]) {
    const inner = "\n" + indent + INDENT;
    return insertEdit(state, inner + "\n" + indent, inner.length);
  }
  const text = "\n" + indent + (opens ? INDENT : "");
  return insertEdit(state, text, text.length);
}

function touchedRange(state) {
  const { value } = state;
  const first = lineStart(value, state.start);
  const endsAtLineStart = state.end > state.start && value[state.end - 1] === "\n";
  const last = lineEnd(value, endsAtLineStart ? state.end - 1 : state.end);
  return { first, last: Math.max(first, last) };
}

function adjustOffset(offset, lines, isStart, hasSelection) {
  let shift = 0;
  for (const line of lines) {
    if (offset < line.offset) break;
    const column = offset - line.offset;
    const within = offset <= line.offset + line.oldLength;
    if (!within) {
      shift += line.delta;
      continue;
    }
    if (line.delta >= 0) {
      const moves = column > line.at || (column === line.at && !(isStart && hasSelection));
      return offset + shift + (moves ? line.delta : 0);
    }
    const removed = -line.delta;
    if (column >= line.at + removed) return offset + shift + line.delta;
    return line.offset + shift + Math.min(column, line.at);
  }
  return offset + shift;
}

function transformLines(state, transform) {
  const { first, last } = touchedRange(state);
  const oldLines = state.value.slice(first, last).split("\n");
  let offset = first;
  const info = [];
  const newLines = oldLines.map((line) => {
    const result = transform(line);
    info.push({ offset, oldLength: line.length, delta: result.text.length - line.length, at: result.at });
    offset += line.length + 1;
    return result.text;
  });
  if (info.every((line) => line.delta === 0)) return { move: true, start: state.start, end: state.end, backward: state.backward };
  const selected = state.end > state.start;
  return {
    from: first, to: last, text: newLines.join("\n"),
    start: adjustOffset(state.start, info, true, selected),
    end: adjustOffset(state.end, info, false, selected),
    backward: state.backward,
  };
}

function indentLine(line) {
  return line.length === 0 ? { text: line, at: 0 } : { text: INDENT + line, at: 0 };
}

function dedentLine(line) {
  const match = line.match(/^(?: {1,4}|\t)/);
  return { text: match ? line.slice(match[0].length) : line, at: 0 };
}

function tab(state, shift) {
  if (shift) return transformLines(state, dedentLine);
  const multiline = state.value.slice(state.start, state.end).includes("\n");
  if (multiline) return transformLines(state, indentLine);
  return insertEdit(state, INDENT, INDENT.length);
}

function toggleComment(state) {
  const { first, last } = touchedRange(state);
  const lines = state.value.slice(first, last).split("\n");
  const filled = lines.filter((line) => line.trim() !== "");
  const commented = filled.length > 0 && filled.every((line) => line.trimStart().startsWith("//"));
  const margin = Math.min(...filled.map((line) => line.match(/^[ \t]*/)[0].length), Infinity);
  return transformLines(state, (line) => {
    if (line.trim() === "") return { text: line, at: 0 };
    const column = line.match(/^[ \t]*/)[0].length;
    if (!commented) return { text: line.slice(0, margin) + COMMENT_MARK + line.slice(margin), at: margin };
    const rest = line.slice(column);
    const removed = rest.startsWith(COMMENT_MARK) ? COMMENT_MARK.length : 2;
    return { text: line.slice(0, column) + rest.slice(removed), at: column };
  });
}

function duplicateLines(state) {
  const { first, last } = touchedRange(state);
  const block = state.value.slice(first, last);
  const shift = block.length + 1;
  return { from: last, to: last, text: "\n" + block, start: state.start + shift, end: state.end + shift, backward: state.backward };
}

function moveLines(state, direction) {
  const { value } = state;
  const { first, last } = touchedRange(state);
  const block = value.slice(first, last);
  if (direction < 0) {
    if (first === 0) return null;
    const previousFirst = lineStart(value, first - 1);
    const previous = value.slice(previousFirst, first - 1);
    const shift = -(previous.length + 1);
    return { from: previousFirst, to: last, text: block + "\n" + previous, start: state.start + shift, end: state.end + shift, backward: state.backward };
  }
  if (last >= value.length) return null;
  const nextEnd = lineEnd(value, last + 1);
  const next = value.slice(last + 1, nextEnd);
  const shift = next.length + 1;
  return { from: first, to: nextEnd, text: next + "\n" + block, start: state.start + shift, end: state.end + shift, backward: state.backward };
}

function deleteLines(state) {
  const { value } = state;
  const { first, last } = touchedRange(state);
  const column = state.start - lineStart(value, state.start);
  const hasNext = last < value.length;
  const from = hasNext || first === 0 ? first : first - 1;
  const to = hasNext ? last + 1 : last;
  const after = value.slice(0, from) + value.slice(to);
  const caretLine = lineStart(after, Math.min(from, after.length));
  const caret = hasNext ? caretLine + Math.min(column, lineEnd(after, caretLine) - caretLine) : lineStart(after, after.length) + Math.min(column, after.length - lineStart(after, after.length));
  return { from, to, text: "", start: caret, end: caret };
}

function home(state, shift) {
  const { value } = state;
  const head = state.backward || state.start === state.end ? state.start : state.end;
  const anchor = head === state.start ? state.end : state.start;
  const begin = lineStart(value, head);
  const indentEnd = begin + value.slice(begin).match(/^[ \t]*/)[0].length;
  const target = head === indentEnd ? begin : indentEnd;
  if (!shift) return { move: true, start: target, end: target };
  return { move: true, start: Math.min(anchor, target), end: Math.max(anchor, target), backward: target < anchor };
}

const SHORTCUTS = [
  [(event) => event.ctrl && !event.alt && event.key === "/", toggleComment],
  [(event) => event.ctrl && !event.shift && event.key.toLowerCase() === "d", duplicateLines],
  [(event) => event.ctrl && event.shift && event.key.toLowerCase() === "k", deleteLines],
  [(event) => event.alt && !event.ctrl && event.key === "ArrowUp", (state) => moveLines(state, -1)],
  [(event) => event.alt && !event.ctrl && event.key === "ArrowDown", (state) => moveLines(state, 1)],
];

export function editFor(state, event) {
  const plain = !event.ctrl && !event.alt;
  if (plain && event.key === "Tab") return tab(state, event.shift);
  if (plain && event.key === "Enter" && !event.shift) return enter(state);
  if (plain && event.key === "Backspace") return backspace(state);
  if (plain && event.key === "Home") return home(state, event.shift);
  if (plain && event.key.length === 1) return typeCharacter(state, event.key);
  for (const [matches, apply] of SHORTCUTS) {
    if (matches(event)) return apply(state);
  }
  return null;
}

export function applyToText(state, edit) {
  if (edit.move) return { value: state.value, start: edit.start, end: edit.end };
  return {
    value: state.value.slice(0, edit.from) + edit.text + state.value.slice(edit.to),
    start: edit.start,
    end: edit.end,
  };
}
