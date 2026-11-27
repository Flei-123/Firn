function countText(errors, warnings) {
  if (errors === 0 && warnings === 0) return "✓ no problems";
  const parts = [];
  if (errors) parts.push(`✖ ${errors}`);
  if (warnings) parts.push(`⚠ ${warnings}`);
  return parts.join("  ");
}

export function createStatusBar(onJump) {
  const counter = document.getElementById("problems");
  const message = document.getElementById("problem-message");
  const checking = document.getElementById("checking");
  const position = document.getElementById("position");
  const list = document.getElementById("problem-list");
  let current = [];

  function renderList() {
    list.replaceChildren(...current.map((problem) => {
      const row = document.createElement("button");
      row.type = "button";
      row.className = `problem-row ${problem.severity}`;
      const where = document.createElement("b");
      where.textContent = `${problem.line}:${problem.col}`;
      row.append(where, document.createTextNode(` ${problem.message}`));
      row.addEventListener("click", () => onJump(problem));
      return row;
    }));
    list.hidden = list.hidden || current.length === 0;
  }

  function update(problems, caretLine) {
    current = problems.slice().sort((a, b) => a.line - b.line || a.col - b.col);
    const errors = current.filter((problem) => problem.severity === "error").length;
    counter.textContent = countText(errors, current.length - errors);
    counter.className = errors ? "bad" : current.length ? "warn" : "ok";
    const shown = current.find((problem) => problem.line === caretLine) || current[0];
    message.hidden = !shown;
    message.textContent = shown ? `Ln ${shown.line}: ${shown.message}` : "";
    message.onclick = shown ? () => onJump(shown) : null;
    renderList();
  }

  counter.addEventListener("click", () => {
    list.hidden = current.length === 0 ? true : !list.hidden;
  });

  return {
    update,
    busy: (flag) => { checking.textContent = flag ? "checking…" : ""; },
    caret: (line, col) => { position.textContent = `Ln ${line}, Col ${col}`; },
    closeList: () => { list.hidden = true; },
  };
}
