import { highlightSource } from "/highlight.js";
import { kindBefore } from "/tokenizer.js";
import { editFor } from "/editing.js";
import { applyEdit, replaceAll, editorState, keyInfo } from "/textedit.js";
import { symbolsOf, contextAt, suggest } from "/completion.js";
import { createPopup } from "/popup.js";
import { createMetrics } from "/metrics.js";
import { createLiveCheck } from "/livecheck.js";
import { createStatusBar } from "/statusbar.js";
import { createTooltip } from "/tooltip.js";
import { normalize, shiftProblems, problemAt, errorLineSet } from "/problems.js";

const sourceBox = document.getElementById("source");
const highlightBox = document.getElementById("highlight");
const gutter = document.getElementById("gutter");
const codeHost = document.querySelector(".code");
const output = document.getElementById("output");
const runButton = document.getElementById("run");
const statusBox = document.getElementById("status");
const metaBox = document.getElementById("meta");
const examplesBox = document.getElementById("examples");
const STORAGE_KEY = "firn-playground-source";
const SILENT_KINDS = new Set(["comment", "str", "esc", "spec", "attr"]);
const CLOSE_POPUP_KEYS = new Set(["ArrowLeft", "ArrowRight", "Home", "End", "PageUp", "PageDown"]);
const LINE_HEIGHT = 20;

let problems = [];
let renderedText = "";
let completionData = null;
let suppressCompletion = false;

const metrics = createMetrics(codeHost);
const popup = createPopup(sourceBox, codeHost, metrics, acceptCompletion);
const tooltip = createTooltip();
const statusBar = createStatusBar(jumpToProblem);
const liveCheck = createLiveCheck(() => sourceBox.value, showLiveResult, statusBar.busy);

function caretPosition() {
  const before = sourceBox.value.slice(0, sourceBox.selectionStart);
  return { line: before.split("\n").length, col: before.length - (before.lastIndexOf("\n") + 1) + 1 };
}

function render() {
  const errors = errorLineSet(problems);
  highlightBox.innerHTML = highlightSource(sourceBox.value, problems);
  gutter.innerHTML = sourceBox.value.split("\n").map((_, index) => `<div class="${errors.has(index + 1) ? "err" : ""}">${index + 1}</div>`).join("");
  statusBar.update(problems, caretPosition().line);
  renderedText = sourceBox.value;
  syncScroll();
}

function syncScroll() {
  highlightBox.scrollTop = sourceBox.scrollTop;
  highlightBox.scrollLeft = sourceBox.scrollLeft;
  gutter.scrollTop = sourceBox.scrollTop;
}

function setProblems(diagnostics) {
  problems = normalize(diagnostics);
  render();
}

function setSource(text) {
  problems = [];
  if (sourceBox.value === "") sourceBox.value = text;
  else replaceAll(sourceBox, text);
  render();
  localStorage.setItem(STORAGE_KEY, sourceBox.value);
  liveCheck.schedule();
}

function showLiveResult(result) {
  if (result === null) return;
  setProblems(result.diagnostics);
}

function updateCompletions(explicit) {
  if (!completionData || sourceBox.selectionStart !== sourceBox.selectionEnd) return popup.hide();
  const caret = sourceBox.selectionStart;
  if (SILENT_KINDS.has(kindBefore(sourceBox.value, caret))) return popup.hide();
  const context = contextAt(sourceBox.value, caret);
  const items = suggest(context, completionData, symbolsOf(sourceBox.value), explicit);
  if (items.length === 1 && items[0].label === context.prefix) return popup.hide();
  popup.show(items);
}

function acceptCompletion(item) {
  const caret = sourceBox.selectionStart;
  const prefix = contextAt(sourceBox.value, caret).prefix;
  const end = caret - prefix.length + item.label.length;
  suppressCompletion = true;
  applyEdit(sourceBox, { from: caret - prefix.length, to: caret, text: item.label, start: end, end });
  suppressCompletion = false;
  popup.hide();
  sourceBox.focus();
}

function handlePopupKey(event) {
  if (!popup.isOpen()) return false;
  if (event.key === "ArrowDown" || event.key === "ArrowUp") popup.move(event.key === "ArrowDown" ? 1 : -1);
  else if ((event.key === "Enter" || event.key === "Tab") && !event.shiftKey) acceptCompletion(popup.current());
  else if (event.key === "Escape") popup.hide();
  else return false;
  event.preventDefault();
  return true;
}

function onKeyDown(event) {
  if (event.isComposing) return;
  const info = keyInfo(event);
  if (info.ctrl && info.key === "Enter") {
    event.preventDefault();
    run();
    return;
  }
  if (handlePopupKey(event)) return;
  if (info.ctrl && info.key === " ") {
    event.preventDefault();
    updateCompletions(true);
    return;
  }
  if (CLOSE_POPUP_KEYS.has(event.key)) popup.hide();
  const edit = editFor(editorState(sourceBox), info);
  if (!edit) return;
  event.preventDefault();
  if (edit.move) popup.hide();
  applyEdit(sourceBox, edit);
}

function onInput(event) {
  problems = shiftProblems(problems, renderedText, sourceBox.value);
  tooltip.hide();
  render();
  localStorage.setItem(STORAGE_KEY, sourceBox.value);
  liveCheck.schedule();
  const type = (event && event.inputType) || "";
  if (suppressCompletion || type.startsWith("history") || type === "insertFromPaste") popup.hide();
  else updateCompletions(false);
}

function onMouseMove(event) {
  const cell = metrics.cellAt(sourceBox, event.clientX, event.clientY);
  const found = problemAt(problems, sourceBox.value.split("\n"), cell.line, cell.col);
  if (found) tooltip.show(found, event.clientX, event.clientY);
  else tooltip.hide();
}

function onCaretMoved() {
  if (popup.isOpen() && sourceBox.selectionStart !== popup.anchor()) popup.hide();
  const { line, col } = caretPosition();
  statusBar.caret(line, col);
  statusBar.update(problems, line);
}

function element(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function addBlock(label, text, className) {
  output.append(element("div", "label", label));
  output.append(element("pre", className, text));
}

function jumpToLine(line, col = 1, length = 0) {
  const lines = sourceBox.value.split("\n");
  const offset = lines.slice(0, line - 1).reduce((total, current) => total + current.length + 1, 0) + Math.max(0, col - 1);
  sourceBox.focus();
  sourceBox.setSelectionRange(offset, offset + length);
  sourceBox.scrollTop = Math.max(0, (line - 4) * LINE_HEIGHT);
  syncScroll();
  onCaretMoved();
}

function jumpToProblem(problem) {
  statusBar.closeList();
  jumpToLine(problem.line, problem.col, 0);
}

function addDiagnostics(result) {
  output.append(element("div", "label", "Compiler errors"));
  const located = result.diagnostics.filter((item) => item.line !== null);
  for (const item of located) {
    const button = element("button", "diag");
    const where = element("b", "", `line ${item.line}:${item.col}`);
    button.append(where, document.createTextNode(`  ${item.message}`));
    button.addEventListener("click", () => jumpToLine(item.line, item.col));
    output.append(button);
  }
  addBlock("Full compiler output", result.compiler_output, "fail");
}

function describeRun(result) {
  if (result.timed_out) return ["Stopped: time limit reached", "fail"];
  if (result.truncated) return ["Stopped: output limit reached", "fail"];
  if (result.exit_code === 0) return ["Exit code 0", "pass"];
  return [`Exit code ${result.exit_code}`, "fail"];
}

function showResult(result) {
  output.replaceChildren();
  setProblems(result.diagnostics);
  if (result.error) {
    addBlock("Error", result.error, "fail");
    return;
  }
  if (result.stage === "input") {
    addBlock("Error", result.compiler_output, "fail");
    return;
  }
  if (result.stage === "compile") {
    addDiagnostics(result);
    metaBox.textContent = `compile ${result.compile_ms} ms`;
    return;
  }
  addBlock("Output", result.stdout || "(no output)", result.stdout ? "" : "hint");
  if (result.stderr) addBlock("stderr", result.stderr, "stderr");
  const [text, className] = describeRun(result);
  output.append(element("div", className, text));
  metaBox.textContent = `compile ${result.compile_ms} ms | run ${result.run_ms} ms`;
}

async function run() {
  liveCheck.cancel();
  runButton.disabled = true;
  statusBox.textContent = "running...";
  metaBox.textContent = "";
  try {
    const response = await fetch("/api/run", { method: "POST", body: JSON.stringify({ source: sourceBox.value }) });
    showResult(await response.json());
    statusBox.textContent = "";
  } catch (failure) {
    showResult({ error: String(failure), diagnostics: [] });
    statusBox.textContent = "failed";
  } finally {
    runButton.disabled = false;
  }
}

async function loadCompletions() {
  completionData = await (await fetch("/completions.json")).json();
}

async function loadExamples() {
  const examples = await (await fetch("/api/examples")).json();
  for (const example of examples) {
    examplesBox.append(new Option(example.title, example.id));
  }
  examplesBox.addEventListener("change", () => {
    const chosen = examples.find((example) => example.id === examplesBox.value);
    if (chosen) setSource(chosen.source);
  });
  const saved = localStorage.getItem(STORAGE_KEY);
  setSource(saved !== null ? saved : examples[0].source);
}

sourceBox.addEventListener("input", onInput);
sourceBox.addEventListener("keydown", onKeyDown);
sourceBox.addEventListener("scroll", () => { syncScroll(); popup.hide(); tooltip.hide(); });
sourceBox.addEventListener("blur", () => popup.hide());
sourceBox.addEventListener("mousedown", () => popup.hide());
sourceBox.addEventListener("mousemove", onMouseMove);
sourceBox.addEventListener("mouseleave", () => tooltip.hide());
document.addEventListener("selectionchange", () => { if (document.activeElement === sourceBox) onCaretMoved(); });
runButton.addEventListener("click", run);
loadCompletions();
loadExamples();
