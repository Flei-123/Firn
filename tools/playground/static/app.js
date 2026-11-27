import { highlightSource } from "/highlight.js";

const sourceBox = document.getElementById("source");
const highlightBox = document.getElementById("highlight");
const gutter = document.getElementById("gutter");
const output = document.getElementById("output");
const runButton = document.getElementById("run");
const statusBox = document.getElementById("status");
const metaBox = document.getElementById("meta");
const examplesBox = document.getElementById("examples");
const STORAGE_KEY = "firn-playground-source";
const INDENT = "    ";

let errorLines = new Set();

function render() {
  const lines = sourceBox.value.split("\n");
  highlightBox.innerHTML = highlightSource(sourceBox.value, errorLines);
  gutter.innerHTML = lines.map((_, index) => `<div class="${errorLines.has(index + 1) ? "err" : ""}">${index + 1}</div>`).join("");
  syncScroll();
}

function syncScroll() {
  highlightBox.scrollTop = sourceBox.scrollTop;
  highlightBox.scrollLeft = sourceBox.scrollLeft;
  gutter.scrollTop = sourceBox.scrollTop;
}

function setSource(text) {
  sourceBox.value = text;
  errorLines = new Set();
  render();
  localStorage.setItem(STORAGE_KEY, text);
}

function insertAtCursor(text) {
  const { selectionStart: start, selectionEnd: end, value } = sourceBox;
  sourceBox.value = value.slice(0, start) + text + value.slice(end);
  sourceBox.selectionStart = sourceBox.selectionEnd = start + text.length;
}

function onKeyDown(event) {
  if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
    event.preventDefault();
    run();
  } else if (event.key === "Tab") {
    event.preventDefault();
    insertAtCursor(INDENT);
    onInput();
  } else if (event.key === "Enter") {
    event.preventDefault();
    const before = sourceBox.value.slice(0, sourceBox.selectionStart);
    const line = before.slice(before.lastIndexOf("\n") + 1);
    const indent = line.match(/^ */)[0] + (line.trimEnd().endsWith("{") ? INDENT : "");
    insertAtCursor("\n" + indent);
    onInput();
  }
}

function onInput() {
  errorLines = new Set();
  render();
  localStorage.setItem(STORAGE_KEY, sourceBox.value);
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

function jumpToLine(line) {
  const lines = sourceBox.value.split("\n");
  const offset = lines.slice(0, line - 1).reduce((total, current) => total + current.length + 1, 0);
  sourceBox.focus();
  sourceBox.setSelectionRange(offset, offset + (lines[line - 1] || "").length);
  sourceBox.scrollTop = Math.max(0, (line - 4) * 20);
  syncScroll();
}

function addDiagnostics(result) {
  output.append(element("div", "label", "Compiler errors"));
  const located = result.diagnostics.filter((item) => item.line !== null);
  for (const item of located) {
    const button = element("button", "diag");
    const where = element("b", "", `line ${item.line}:${item.col}`);
    button.append(where, document.createTextNode(`  ${item.message}`));
    button.addEventListener("click", () => jumpToLine(item.line));
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
  errorLines = new Set(result.diagnostics.filter((item) => item.line !== null && item.severity === "error").map((item) => item.line));
  render();
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
sourceBox.addEventListener("scroll", syncScroll);
runButton.addEventListener("click", run);
loadExamples();
