function direction(edit) {
  return edit.backward ? "backward" : "forward";
}

export function applyEdit(box, edit) {
  if (edit.move) {
    box.setSelectionRange(edit.start, edit.end, direction(edit));
    return;
  }
  box.setSelectionRange(edit.from, edit.to);
  const done = edit.text === "" ? document.execCommand("delete") : document.execCommand("insertText", false, edit.text);
  if (!done) {
    box.setRangeText(edit.text, edit.from, edit.to, "end");
    box.dispatchEvent(new Event("input", { bubbles: true }));
  }
  box.setSelectionRange(edit.start, edit.end, direction(edit));
}

export function replaceAll(box, text) {
  if (document.activeElement !== box) box.focus();
  applyEdit(box, { from: 0, to: box.value.length, text, start: 0, end: 0 });
}

export function editorState(box) {
  return { value: box.value, start: box.selectionStart, end: box.selectionEnd, backward: box.selectionDirection === "backward" };
}

export function keyInfo(event) {
  const altGraph = event.getModifierState && event.getModifierState("AltGraph");
  return {
    key: event.key,
    ctrl: !altGraph && (event.ctrlKey || event.metaKey),
    alt: !altGraph && event.altKey,
    shift: event.shiftKey,
  };
}
