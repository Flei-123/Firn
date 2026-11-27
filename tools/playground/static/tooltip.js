const OFFSET = 14;

export function createTooltip() {
  const box = document.getElementById("tooltip");

  function show(problem, x, y) {
    box.className = problem.severity;
    box.replaceChildren();
    const head = document.createElement("b");
    head.textContent = problem.message;
    box.append(head);
    if (problem.hint) {
      const hint = document.createElement("div");
      hint.textContent = problem.hint;
      box.append(hint);
    }
    box.hidden = false;
    const left = Math.min(x + OFFSET, window.innerWidth - box.offsetWidth - 8);
    const top = y + OFFSET + box.offsetHeight > window.innerHeight ? y - OFFSET - box.offsetHeight : y + OFFSET;
    box.style.left = `${Math.max(4, left)}px`;
    box.style.top = `${Math.max(4, top)}px`;
  }

  function hide() {
    box.hidden = true;
  }

  return { show, hide };
}
