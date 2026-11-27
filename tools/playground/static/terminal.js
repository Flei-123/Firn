const KIND_CLASS = { out: "term-out", err: "term-err", in: "term-in", sys: "term-sys" };

function node(tag, className, text) {
  const item = document.createElement(tag);
  if (className) item.className = className;
  if (text !== undefined) item.textContent = text;
  return item;
}

export function createTerminal(host, handlers) {
  const screen = node("pre", "term-screen");
  const typed = node("span", "term-in");
  const cursor = node("span", "term-cursor", " ");
  const rest = node("span", "term-in");
  const live = node("span", "term-live");
  const input = node("input", "term-input");
  let running = false;

  live.append(typed, cursor, rest);
  screen.append(live);
  input.type = "text";
  input.id = "term-input";
  input.setAttribute("autocomplete", "off");
  input.setAttribute("autocapitalize", "off");
  input.setAttribute("autocorrect", "off");
  input.setAttribute("spellcheck", "false");
  input.setAttribute("aria-label", "Terminal input");

  function scrollToEnd() {
    host.scrollTop = host.scrollHeight;
  }

  function mirror() {
    const at = input.selectionStart ?? input.value.length;
    typed.textContent = input.value.slice(0, at);
    cursor.textContent = input.value.slice(at, at + 1) || " ";
    rest.textContent = input.value.slice(at + 1);
    scrollToEnd();
  }

  function write(kind, text) {
    const last = live.previousSibling;
    if (last && last.className === KIND_CLASS[kind]) last.textContent += text;
    else screen.insertBefore(node("span", KIND_CLASS[kind], text), live);
    scrollToEnd();
  }

  function clearInput() {
    input.value = "";
    mirror();
  }

  function submitLine() {
    const line = input.value;
    write("in", line + "\n");
    clearInput();
    handlers.line(line + "\n");
  }

  function endOfInput() {
    if (input.value !== "") {
      const partial = input.value;
      write("in", partial);
      clearInput();
      handlers.line(partial);
      return;
    }
    write("sys", "^D\n");
    handlers.eof();
  }

  function onKeyDown(event) {
    if (event.isComposing) return;
    const ctrl = event.ctrlKey || event.metaKey;
    if (ctrl && event.key === "Enter") return;
    if (event.key === "Enter") {
      event.preventDefault();
      submitLine();
    } else if (ctrl && event.key.toLowerCase() === "d") {
      event.preventDefault();
      endOfInput();
    } else if (ctrl && event.key.toLowerCase() === "c" && input.selectionStart === input.selectionEnd) {
      event.preventDefault();
      write("sys", "^C\n");
      handlers.interrupt();
    }
  }

  function start() {
    running = true;
    screen.replaceChildren(live);
    input.disabled = false;
    live.hidden = false;
    clearInput();
    host.replaceChildren(screen, input);
    input.focus({ preventScroll: true });
  }

  function finish(text, className) {
    running = false;
    input.disabled = true;
    live.hidden = true;
    if (!screen.isConnected) host.replaceChildren(screen);
    const last = live.previousSibling;
    if (last && !last.textContent.endsWith("\n")) write("sys", "\n");
    host.append(node("div", `term-exit ${className}`, text));
    scrollToEnd();
  }

  input.addEventListener("keydown", onKeyDown);
  input.addEventListener("input", mirror);
  input.addEventListener("keyup", mirror);
  input.addEventListener("select", mirror);
  host.addEventListener("click", () => {
    if (running && !window.getSelection().toString()) input.focus({ preventScroll: true });
  });

  return { start, write, finish };
}
