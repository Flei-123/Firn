const EVENTS = ["status", "started", "out", "err", "exit", "failed"];
const TERMINAL_EVENTS = new Set(["exit", "failed"]);
const STATUS_MESSAGES = { 429: "Too many runs, wait a moment.", 503: "Server is busy, try again.", 413: "Request too large." };

async function readError(response) {
  const reply = await response.json().catch(() => ({}));
  return reply.error || STATUS_MESSAGES[response.status] || `HTTP ${response.status}`;
}

export function startSession(source, handlers) {
  let id = null;
  let events = null;
  let closed = false;
  let stopRequested = false;
  let queue = Promise.resolve();

  function post(path, body) {
    return fetch(`/api/session/${id}/${path}`, { method: "POST", body: JSON.stringify(body), keepalive: true }).catch(() => null);
  }

  function close() {
    closed = true;
    if (events) events.close();
  }

  function listen() {
    events = new EventSource(`/api/session/${id}/events`);
    for (const name of EVENTS) {
      events.addEventListener(name, (message) => {
        if (closed) return;
        if (TERMINAL_EVENTS.has(name)) close();
        handlers[name](JSON.parse(message.data));
      });
    }
    events.onerror = () => {
      if (closed || events.readyState !== EventSource.CLOSED) return;
      close();
      handlers.error("Connection lost.");
    };
  }

  async function open() {
    try {
      const response = await fetch("/api/session", { method: "POST", body: JSON.stringify({ source }) });
      if (!response.ok) throw new Error(await readError(response));
      id = (await response.json()).id;
    } catch (failure) {
      if (!closed) handlers.error(failure.message);
      close();
      return;
    }
    if (stopRequested) post("stop", {});
    if (!closed) listen();
  }

  open();
  return {
    send(text) {
      if (id) queue = queue.then(() => post("stdin", { data: text }));
    },
    eof() {
      if (id) queue = queue.then(() => post("stdin", { eof: true }));
    },
    stop() {
      stopRequested = true;
      if (id) post("stop", {});
    },
    close,
  };
}
