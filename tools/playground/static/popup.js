import { LINE_HEIGHT } from "/metrics.js";

const KIND_BADGES = { function: "ƒ", method: "ƒ", keyword: "k", module: "m", type: "T", variable: "v", constant: "c", field: "f" };

export function createPopup(box, host, metrics, onAccept) {
  const list = document.createElement("ul");
  list.id = "completion";
  list.hidden = true;
  list.setAttribute("role", "listbox");
  host.append(list);
  let items = [];
  let selected = 0;
  let anchor = -1;

  function place() {
    const point = metrics.caretPoint(box);
    const left = Math.max(0, Math.min(point.left, host.clientWidth - list.offsetWidth - 4));
    const top = point.top + list.offsetHeight > host.clientHeight ? Math.max(0, point.top - LINE_HEIGHT - list.offsetHeight) : point.top;
    list.style.top = `${top}px`;
    list.style.left = `${left}px`;
  }

  function paint() {
    list.replaceChildren(...items.map((item, index) => {
      const row = document.createElement("li");
      row.className = index === selected ? "selected" : "";
      row.setAttribute("role", "option");
      const badge = document.createElement("i");
      badge.className = `kind-${item.kind}`;
      badge.textContent = KIND_BADGES[item.kind] || "·";
      const label = document.createElement("b");
      label.textContent = item.label;
      const detail = document.createElement("span");
      detail.textContent = item.detail || "";
      row.append(badge, label, detail);
      row.addEventListener("mousedown", (event) => {
        event.preventDefault();
        onAccept(item);
      });
      return row;
    }));
    list.querySelector(".selected")?.scrollIntoView({ block: "nearest" });
  }

  function show(next) {
    items = next;
    selected = 0;
    anchor = box.selectionStart;
    list.hidden = items.length === 0;
    if (items.length === 0) return;
    paint();
    place();
  }

  function hide() {
    list.hidden = true;
    items = [];
  }

  function move(step) {
    selected = (selected + step + items.length) % items.length;
    paint();
  }

  return {
    show,
    hide,
    move,
    isOpen: () => !list.hidden,
    anchor: () => anchor,
    current: () => items[selected],
  };
}
