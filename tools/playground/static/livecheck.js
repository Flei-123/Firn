const IDLE_MS = 500;

export function createLiveCheck(getSource, onResult, onBusy) {
  let timer = null;
  let version = 0;
  let controller = null;

  async function check(source, mine) {
    controller = new AbortController();
    onBusy(true);
    try {
      const response = await fetch("/api/check", { method: "POST", body: JSON.stringify({ source }), signal: controller.signal });
      const result = await response.json();
      if (mine === version && Array.isArray(result.diagnostics)) onResult(result);
    } catch (failure) {
      if (failure.name !== "AbortError") onResult(null);
    } finally {
      if (mine === version) onBusy(false);
    }
  }

  function cancel() {
    version += 1;
    clearTimeout(timer);
    if (controller) controller.abort();
  }

  function schedule() {
    cancel();
    onBusy(true);
    const mine = version;
    timer = setTimeout(() => check(getSource(), mine), IDLE_MS);
  }

  function stop() {
    cancel();
    onBusy(false);
  }

  return { schedule, cancel: stop };
}
