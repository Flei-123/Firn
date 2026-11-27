export const LINE_HEIGHT = 20;
export const PAD_TOP = 8;
export const PAD_LEFT = 12;
const PROBE_LENGTH = 20;

export function createMetrics(host) {
  let width = 0;

  function charWidth() {
    if (width) return width;
    const probe = document.createElement("span");
    probe.style.cssText = "position:absolute;visibility:hidden;white-space:pre;font:inherit";
    probe.textContent = "M".repeat(PROBE_LENGTH);
    host.append(probe);
    width = probe.getBoundingClientRect().width / PROBE_LENGTH;
    probe.remove();
    return width;
  }

  function caretPoint(box) {
    const before = box.value.slice(0, box.selectionStart);
    const line = before.split("\n").length - 1;
    const column = before.length - (before.lastIndexOf("\n") + 1);
    return {
      top: PAD_TOP + (line + 1) * LINE_HEIGHT - box.scrollTop,
      left: PAD_LEFT + column * charWidth() - box.scrollLeft,
    };
  }

  function cellAt(box, clientX, clientY) {
    const rect = box.getBoundingClientRect();
    const line = Math.floor((clientY - rect.top - PAD_TOP + box.scrollTop) / LINE_HEIGHT) + 1;
    const col = Math.floor((clientX - rect.left - PAD_LEFT + box.scrollLeft) / charWidth()) + 1;
    return { line, col };
  }

  return { charWidth, caretPoint, cellAt };
}
