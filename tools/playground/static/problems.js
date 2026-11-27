export function normalize(diagnostics) {
  return diagnostics
    .filter((item) => item.line !== null)
    .map((item) => ({
      line: item.line, col: item.col, length: Math.max(1, item.length || 1),
      severity: item.severity, message: item.message, hint: item.hint || "",
    }));
}

export function rangeOnLine(problem, lineLength) {
  if (lineLength === 0) return { start: 0, end: 0 };
  const start = Math.min(problem.col - 1, lineLength - 1);
  return { start, end: Math.min(start + problem.length, lineLength) };
}

export function errorLineSet(problems) {
  return new Set(problems.filter((item) => item.severity === "error").map((item) => item.line));
}

export function markTokens(tokens, ranges) {
  if (ranges.length === 0) return tokens;
  const result = [];
  let position = 0;
  for (const token of tokens) {
    const tokenEnd = position + token.text.length;
    let cursor = position;
    const cuts = ranges.flatMap((range) => [range.start, range.end]).filter((point) => point > position && point < tokenEnd).sort((a, b) => a - b);
    for (const cut of [...cuts, tokenEnd]) {
      if (cut === cursor) continue;
      const hit = ranges.find((range) => range.start <= cursor && cursor < range.end);
      result.push({ kind: token.kind, text: token.text.slice(cursor - position, cut - position), mark: hit ? hit.severity : "" });
      cursor = cut;
    }
    position = tokenEnd;
  }
  return result;
}

function offsetOf(text, line, col) {
  const lines = text.split("\n");
  let offset = 0;
  for (let index = 0; index < line - 1 && index < lines.length; index += 1) offset += lines[index].length + 1;
  return offset + col - 1;
}

function positionOf(text, offset) {
  const before = text.slice(0, offset);
  const line = before.split("\n").length;
  return { line, col: offset - (before.lastIndexOf("\n") + 1) + 1 };
}

export function shiftProblems(problems, oldText, newText) {
  if (oldText === newText) return problems;
  const limit = Math.min(oldText.length, newText.length);
  let prefix = 0;
  while (prefix < limit && oldText[prefix] === newText[prefix]) prefix += 1;
  let suffix = 0;
  while (suffix < limit - prefix && oldText[oldText.length - 1 - suffix] === newText[newText.length - 1 - suffix]) suffix += 1;
  const changeEnd = oldText.length - suffix;
  const delta = newText.length - oldText.length;
  const kept = [];
  for (const problem of problems) {
    const start = offsetOf(oldText, problem.line, problem.col);
    const end = start + problem.length;
    if (end < prefix) kept.push(problem);
    else if (start >= changeEnd) kept.push({ ...problem, ...positionOf(newText, start + delta) });
  }
  return kept;
}

export function problemAt(problems, lines, line, col) {
  return problems.find((problem) => {
    if (problem.line !== line) return false;
    const range = rangeOnLine(problem, (lines[line - 1] || "").length);
    return col - 1 >= range.start && col - 1 < Math.max(range.end, range.start + 1);
  });
}
