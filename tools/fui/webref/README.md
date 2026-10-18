# webref -- reference screenshots of the mobile Jarvis web app

Mock data only: `mockserver.py` serves `/root/jarvis/public` read-only and answers `/api/*` with hard-coded data
(no login, no production data, no service restart). `capture.py` starts it and runs `capture.mjs`
(node playwright from /root/jarvis/node_modules + /usr/bin/chromium): viewport 412x915 CSS px, touch/mobile, de-AT.

    python3 tools/fui/webref/capture.py                 # dpr 1 and 3 into this dir (dpr 3 PNGs are ~300-900 KB)
    python3 tools/fui/webref/capture.py --dpr 1 --out DIR --screens chat,board

Files: `<screen>-<dark|claude>.png` (dpr 1, committed, <= 150 KB). `dark` = theme "fleitec" (navy), `claude` = theme "claude"
(warm dark grey). The web app has NO light theme in its own CSS (appearance presets are server data, not mocked),
so "light" does not exist. `@3x` files (Xiaomi 14T Pro ~ dpr 3) are produced on demand and not committed (too big).

Screens: chat (user/assistant bubbles, code block, composer) | board (projects overview = Leitstand tab) | needs (the
"Braucht dich" box) | tasks (modal) | devices (command-center card) | settings (modal) | login.

Mock quirks: WebSocket is stubbed (no live updates); `app.js` is patched IN MEMORY for the tasks modal (production
bug, see docs/fui-mobile-gap.md section 5); command-center tasks are never "overdue" (same bug).
Compare with `tools/fui/pixcmp.py`.
