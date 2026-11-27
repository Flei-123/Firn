import { tokenizeSource } from "/tokenizer.js";
import { markTokens, rangeOnLine, errorLineSet } from "/problems.js";

function escapeHtml(text) {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function renderToken(token) {
  const text = escapeHtml(token.text);
  const classes = [token.kind ? `tok-${token.kind}` : "", token.mark ? `sq-${token.mark}` : ""].filter(Boolean).join(" ");
  return classes ? `<span class="${classes}">${text}</span>` : text;
}

function lineRanges(problems, number, length) {
  return problems.filter((problem) => problem.line === number).map((problem) => ({ ...rangeOnLine(problem, length), severity: problem.severity }));
}

function renderLine(tokens, ranges) {
  if (tokens.length === 0) return ranges.length ? "<span class=\"sq-error\"> </span>" : " ";
  return markTokens(tokens, ranges).map(renderToken).join("");
}

export function highlightSource(source, problems) {
  const errors = errorLineSet(problems);
  return tokenizeSource(source).map((tokens, index) => {
    const length = tokens.reduce((total, token) => total + token.text.length, 0);
    const cls = errors.has(index + 1) ? "line err" : "line";
    return `<div class="${cls}">${renderLine(tokens, lineRanges(problems, index + 1, length))}</div>`;
  }).join("");
}
