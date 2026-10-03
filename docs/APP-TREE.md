# The app tree — should a fUi / OrientOS program be one browser-like element tree?

Status: **research, concept and a small measurement** (29.09.2026). Nothing in
`lib/` was changed. The numbers come from `tools/fui/apptree_main.fi` (new) and
`tools/fui/apptree.sh` (new); the code facts refer to Firn `main` 7fbd6a39 and
OrientOS `main` 5fb1f9e2 unless a branch is named. Every claim about another
toolkit carries a source number from §9. What could not be verified is marked
**UNVERIFIED**; my own judgement is marked **(assessment)**.

---

## 0. The answer in one paragraph

**Yes — but as a typed, compiled, DOM-like *semantic* tree, not as a browser
inside every app.** Every visible element of a fUi program should be a node of
one `scene.Scene`; layout, hit testing, event dispatch, accessibility, queries,
tests, the inspector and AI agents all read that same tree, and painting is a
function of it. No HTML or CSS text is parsed at run time and there is no script
engine. On OrientOS the tree answers **"what is on the screen"**, and the action
bus stays the only door for **"what the app can do"** (typed actions, rights,
confirmation, audit, undo). fUi already has the skeleton — a scene tree, a
stylesheet with selectors, hit testing, a focus chain, ARIA-style roles and names
with a text dump. What is missing is stable keys, multi-touch pointers and
gestures, capture/bubble dispatch, a query API, change records, an inspector,
secret fields in the accessibility export, and incremental passes.

The measurement:

- **Cost per node.** The tree costs about **5.8 µs per node per frame** when it is
  rebuilt from scratch, and **816 bytes per node**.
- **At today's cap.** With 128 nodes the tree work is under 1 ms of a 16 ms frame,
  so it is cheap.
- **At 1009 nodes.** The tree work is 5.9 ms, a third of the frame before a single
  pixel is painted. Growing the cap therefore needs incremental passes first.
- **Queries, dumps, culling.**
  - A selector query over 1009 nodes costs 0.23 ms.
  - A full accessibility dump costs 0.9 ms.
  - A culling experiment cut the draw pass of the 1009-node tree by 17 %, with the
    same pixel checksum.

---

## 1. What "the app as one tree" means here

Every GUI toolkit keeps a tree of some kind — even Win32 has its HWND hierarchy
[1]. The real question is three questions:

1. **Which tree is the truth.** Is it the painted one, a widget object tree, or a
   semantic tree built next to them?
2. **How open it is.** Can a test, a script, an inspector, a screen reader or an
   AI agent find a node by role, name or selector, and watch it change?
3. **How fine it is.** Is a list row or a text field a node, or a private detail
   of one big widget?

"Browser-like" means taking the web's answers to those three questions:

- **One retained element tree.** Every element has a kind/role, an id, classes,
  state, a style and a layout box.
- **An event path through that tree.** Events go capture → target → bubble, with
  pointer ids and pointer capture.
- **Selectors, observers and an inspector over it.** There are selector queries
  (`querySelectorAll`), batched change records (`MutationObserver`) and live
  developer tools.
- **An accessibility tree derived from it.**

It does **not** mean HTML, CSS text or JavaScript at run time. That is Certus'
job (`lib/dom`, `lib/css`, `lib/browser` exist in this repository), and fUi
deliberately stays allocation-free and kernel-capable. The header of
`lib/fui/sheet.fi` states this: no CSS reader (lines 20–24).

The opposite, "like a normal Windows/Linux program", means one of two things:

- **Retained widget objects with a separate accessibility layer bolted on.** This
  is Win32 + MSAA/UIA, GTK + AT-SPI or Qt + QAccessible.
- **Immediate-mode painting with no retained tree at all.** This is Dear ImGui.

---

## 2. How the classic models work

### 2.1 Facts per model

**Win32 / USER / GDI**

- **Tree and messages.**
  - Each control is an HWND. Child windows are confined to their parent's client
    area [1].
  - A message loop (`GetMessage` → `DispatchMessage` → WndProc) delivers WM_*
    messages [2].
  - Hit testing runs through WM_NCHITTEST (r).
- **Touch and pen.** WM_TOUCH/WM_GESTURE came with Windows 7. WM_POINTER* (Windows
  8) unifies touch, pen and mouse [5].
- **Why the tree is coarse.** Windows are expensive:
  - "There is a limit of 10,000 USER handles per process, and you are likely to run
    out of desktop heap long before then" [3].
  - The per-process quota is configurable within 200–18,000 [4].
  - For that reason Internet Explorer reimplemented checkboxes, list boxes and edit
    boxes as windowless controls [3]. It had its own element tree, and HWND-based
    tools could no longer see inside it.

**WPF / WinUI 3**

- **Two trees.**
  - The logical tree holds the content model.
  - The visual tree also holds the template-generated visuals.
  - Routed events travel along the visual tree [7].
- **Routed events.**
  - WPF: tunnelling (`Preview*`, root → source), bubbling (source → root) and direct
    [8].
  - WinUI 3 bubbles a fixed set of events and cannot declare custom routed events
    [9].
- **Rendering.** WPF renders retained through milcore/DirectX [6].
- **Accessibility.** It comes from a *third*, parallel tree of `AutomationPeer`
  objects that feeds UI Automation (r).
- **Tools.** Snoop browses the visual, logical and automation trees of a running
  app and edits properties live [15]. Visual Studio has a Live Visual Tree (r).

**Microsoft UI Automation (UIA)**

- **Model.** Provider/client, with a broker in UIAutomationCore [10].
- **Three views.** Raw, Control (drops layout-only panels) and Content [11].
- **Control patterns.** Invoke, Value, Toggle, ExpandCollapse and others [10].
- **Identity.**
  - `AutomationId` is language-independent and "should be unique among sibling
    elements". It "is not guaranteed to be stable across different releases or
    builds".
  - `Name` "cannot be used as a unique identifier among siblings". Test automation
    should use AutomationId or RuntimeId [12].
- **Passwords.** `IsPassword`: clients should suppress keyboard echo, and "attempting
  to access the Value property of the protected element (edit control) may cause an
  error to occur" [12].
- **Speed.** Every property read is a cross-process call. Clients batch reads with
  `CacheRequest` [13].
- **Security.** UIPI blocks lower-integrity processes. `UIAccess` apps must be
  signed and installed in a secure location [14].

**GTK4 + AT-SPI2**

- **Events.**
  - Event controllers run in phases. "CAPTURE … runs from the toplevel down to the
    event widget". BUBBLE follows, then TARGET [16].
  - Gesture classes cover click, drag, swipe, zoom, rotate and long press [17].
- **Tools.** GtkInspector shows the live widget tree and edits CSS live [18].
- **Accessibility.**
  - `GtkAccessible` roles and states are modelled on WAI-ARIA, with an AT-SPI
    backend [19].
  - Password text reaches AT-SPI only as mask characters [20].
- **AT-SPI costs.**
  - It runs over a separate D-Bus. Every query is a round trip, with long-standing
    performance complaints [28].
  - "DBus specs on the user's session bus don't have any authorisation controls"
    [26].
  - Since at-spi2-core 2.56 and GNOME 48, key events are taken in the compositor.
    The D-Bus name is checked against an allowlist "so that no arbitrary process can
    act as a keylogger" [27].
  - The "Newton" prototype makes the compositor the authority, and apps *push* tree
    updates [25].

**Qt**

- **Object tree.** QObject parent/child is an ownership tree. `findChild` finds
  objects by `objectName` [21].
- **Qt Quick.** QQuickItems build a separate `QSGNode` render tree, rendered on a
  render thread with batching [22].
- **Accessibility.**
  - Bridges exist to MSAA, macOS Accessibility and AT-SPI.
  - On Linux, accessibility is only switched on when `org.a11y.Status.IsEnabled` and
    `ScreenReaderEnabled` are true, or when an environment variable is set [23].
- **Tools.** GammaRay inspects object, item and scene-graph trees live [24].

**Cocoa / AppKit**

- **Tree and events.** An NSView tree with an NSResponder chain and `hitTest:` (r).
- **Accessibility.** A semantic tree through the NSAccessibility protocol with
  `accessibilityIdentifier` [29, reference, not fetched].
- **Testing.** XCUITest queries walk that accessibility tree [30].
- **Controlling other apps.** This needs the user-granted "Accessibility" TCC
  permission (`AXIsProcessTrusted`) (r).
- **Secure input.** Secure event input stops other processes from reading
  keystrokes [31].

**Android Views**

- **Tree and layout.** A View/ViewGroup tree with measure → layout → draw. Google
  recommends flat hierarchies [34].
- **Touch.**
  - A parent's `onInterceptTouchEvent` can steal a gesture; the child then gets
    `ACTION_CANCEL` [32].
  - `ACTION_POINTER_DOWN` fires per extra finger, and pointer ids stay stable [33].
- **Accessibility.** An `AccessibilityNodeInfo` tree. AccessibilityServices can read
  and drive other apps.
- **Google Play policy** [35]:
  - Only tools for people with disabilities may declare `isAccessibilityTool`.
    "automation tools, assistants … password managers" may not.
  - For apps that are not such tools: "Any use of the Accessibility API that enables
    an app to autonomously initiate, plan, and execute actions or decisions is
    strictly prohibited."

**Jetpack Compose**

- **Phases.** Composition → layout → drawing [36].
- **Semantics tree.** A separate semantics tree serves accessibility *and* tests.
  Nodes can be merged (a button with two texts becomes one node) or read unmerged
  [37][38].
- **Test finders.** `onNodeWithText`, `hasTestTag` and `printToLog` query that tree
  [38].

**Flutter**

- **Three trees.** Immutable Widgets, persistent Elements, and RenderObjects for
  layout, paint and hit test [39]. Keys carry identity across rebuilds [44].
- **Gestures.** A `GestureArena` settles competing recognisers: "The first member to
  accept or the last member to not reject wins" [40].
- **Semantics on demand.** "When semanticsOwner is null, the PipelineOwner skips all
  steps relating to semantics" [41].
- **Web.** Flutter translates its semantics tree "into an accessible HTML DOM
  structure". "For performance reasons … not on by default": a user presses an
  invisible "Enable accessibility" button, or the app calls `ensureSemantics()`
  [42].
- **Tools.** The DevTools Inspector [43].

**Web DOM**

- **Event dispatch.** Capture → target → bubble along the tree path [45].
- **Passive listeners.** These let scrolling start without waiting for script [62].
- **Pointer Events Level 3** (W3C Recommendation, 30 June 2026) [47]:
  - `pointerId` is "a unique identifier for the pointer".
  - `isPrimary` marks the primary pointer.
  - `setPointerCapture` routes one pointer to one element.
  - Panning and zooming "cannot be suppressed by canceling a pointer event"; pages
    declare them with `touch-action` instead.
- **Observing changes.**
  - `MutationObserver` delivers *batched* records [46].
  - The old synchronous Mutation Events were removed in Chrome 127 (23 July 2024)
    because "they are slow … and prevent many UA run-time optimizations" and caused
    "many crashes and security bugs" [48].
- **Accessibility tree.** Derived from DOM + ARIA. Names follow accname [50]. DevTools
  and CDP expose the full tree [49][63].
- **Size.** Lighthouse warns above ~800 body nodes and errors above ~1,400 [51].
- **WebAssembly.**
  - "WebAssembly has no direct access to Web APIs". In Mozilla's experiment, removing
    the JS glue cut the time to apply DOM changes by 45 %.
  - The Component Model route is still in development [52].

**Immediate mode: Dear ImGui, egui + AccessKit**

- **Dear ImGui.**
  - No retained widget objects; IDs are hashes of the label plus the parent scope
    [53].
  - No accessibility; the AccessKit request is an open issue [54].
  - The Test Engine drives the UI by ID paths [55].
- **AccessKit.**
  - Apps *push* a full tree, then incremental updates with integer node IDs.
  - Adapters exist for UIA, NSAccessibility, AT-SPI, Android and iOS; web is planned.
  - It supports immediate-mode toolkits "as long as they can provide a stable ID for
    each UI element" [56].
- **egui.** egui only builds AccessKit trees once enabled, and "the performance
  impact for users that don't need accessibility is negligible" [57].

**AI agents over UI trees**

- **OSWorld (desktop tasks)** [58]:
  - Accessibility tree only: GPT-4 12.24 %, GPT-4o 11.36 %.
  - Screenshot only: GPT-4V 5.26 %, GPT-4o 5.03 %.
  - Humans: 72.36 %.
  - One tree observation needs about 6,000 tokens of context for 90 % of cases.
- **UFO2 (Windows)** [59]:
  - UIA + vision reaches 26.6 %, UIA alone 23.4 %, on Windows Agent Arena.
  - UFO2 reaches 27.9 % against Operator's 20.8 %.
- **Playwright MCP** "uses Playwright's accessibility tree, not pixel-based input".
  Its README also calls those trees "verbose" in token terms [60].

### 2.2 Comparison table

| Model | Tree(s) | Events / touch | Accessibility & automation | Dev tools |
|---|---|---|---|---|
| Win32 / GDI | HWND per control; costly, so coarse [1][3] | Messages; WM_POINTER ids (Win 8) [5] | MSAA/UIA proxies; custom-drawn = opaque | Spy++ / Inspect (r) |
| WPF / WinUI | Logical + visual + automation-peer tree [7] | Routed tunnel/bubble [8][9] | UIA via peers; AutomationId [12] | Snoop, Live Visual Tree [15] |
| UIA (layer) | Raw/Control/Content views [11] | — | Patterns, cross-process, CacheRequest [13] | Inspect, FlaUI (r) |
| GTK4 + AT-SPI | Widget tree | Capture/bubble phases, gesture classes [16][17] | ARIA-like; D-Bus, no authorisation [19][26] | GtkInspector [18] |
| Qt Widgets / Quick | QObject tree; QQuickItem → QSGNode [21][22] | Event filters; Quick pointer handlers | QAccessible bridges, on only if AT asks [23] | GammaRay [24] |
| AppKit | NSView + responder chain | Responder chain | NSAccessibility; XCUITest; TCC gate [30] | Accessibility Inspector |
| Android Views | View tree [34] | Intercept/cancel, pointer ids [32][33] | AccessibilityNodeInfo; Play restricts agents [35] | Layout Inspector |
| Compose | Layout nodes + semantics tree [36][37] | pointerInput / gestures | Semantics = a11y = tests [38] | Layout Inspector |
| Flutter | Widget / Element / RenderObject + semantics [39] | GestureArena [40] | Semantics on demand; ARIA DOM on web [41][42] | DevTools Inspector [43] |
| Web DOM | DOM + CSSOM + a11y tree | Capture/bubble; Pointer Events L3 [45][47] | ARIA/accname; Playwright/CDP [50][60] | DevTools |
| Dear ImGui | None (ID stack) [53] | Polled per frame | None [54]; Test Engine by path [55] | Metrics, ID stack tool |
| egui + AccessKit | None; pushes tree with stable IDs [56] | Polled per frame | UIA/AT-SPI/NSA via adapters [56][57] | Basic |
| **fUi today** | `scene.Scene`, 128 nodes (§3) | One pointer, focus chain, drag "track" | Roles/names/states + text dump; no bridge | Text dump only |

**Ratings (assessment):** `++` very good · `+` good · `0` mixed · `−` weak.

| Model | Speed | Memory | Testability | AI / script control | Accessibility | Dev tools | Learning curve |
|---|---|---|---|---|---|---|---|
| Win32 / GDI | + | − (handles) | − | 0 (messages, UIPI) | 0 | 0 | + |
| WPF / WinUI | 0 | − (3 trees) | + | + (UIA) | + | ++ | − |
| GTK4 + AT-SPI | + | 0 | 0 | 0 (D-Bus, open bus) | + | + | 0 |
| Qt | + | 0 | + | + in-process | + | + | 0 |
| AppKit | + | 0 | + | + (TCC-gated) | ++ | + | − |
| Android Views | 0 | − | + | ++ (but policy-restricted) | + | + | 0 |
| Compose | + | + | ++ | + | + | + | 0 |
| Flutter | + | − (4 trees) | ++ | + (Keys, semantics) | + | ++ | 0 |
| Web DOM | 0 | − | ++ | ++ | ++ | ++ | + |
| Dear ImGui | ++ | ++ | 0 | − | − | + | ++ |
| egui + AccessKit | ++ | + | 0 | + | + | 0 | + |
| **fUi target (§5)** | + | + | ++ | ++ | + | + | + |

### 2.3 What the field teaches (assessment, from §2.1)

- **L1 — the semantic tree is the one that matters for everyone who is not the
  painter.**
  - Mature stacks separate a render tree from a semantic tree: WPF visuals vs
    automation peers, Qt items vs QSGNodes, Compose layout vs semantics, Flutter
    render objects vs semantics, browsers' DOM vs accessibility tree.
  - Tests, screen readers and agents all use the semantic one ([30], [38], [11]).
- **L2 — retrofitting is the expensive path.**
  - IE rebuilt every control windowless [3]. ImGui still has no accessibility [54].
    Flutter web needs an opt-in placeholder [42].
  - OrientOS' own `docs/A11Y.md` (branch `archiv/a11y`) reaches the same conclusion
    from X11/Wayland history. After the fact, *every program* must be touched again;
    built in, it costs one place.
- **L3 — semantics on demand.** Flutter [41], egui/AccessKit [57] and Qt on Linux
  [23] build the accessibility side only when a client asks. Nobody listening means
  no cost.
- **L4 — stable identity is the automation key.** UIA AutomationId [12], Flutter
  Keys [44], Compose testTag [38], ImGui ID paths [55]. AccessKit even makes stable
  IDs its only requirement [56].
- **L5 — the event model has converged.**
  - Dispatch runs capture → target → bubble ([45], [8], [16]).
  - Each pointer carries an id and can be captured ([47], [33], [5]).
  - Competing gestures are arbitrated ([40], [32]).
- **L6 — pull across processes is slow, push is fast.**
  - UIA needs caching [13], and AT-SPI round trips are a known cost [28].
  - AccessKit and Newton push incremental updates [56][25].
- **L7 — whoever reads the tree reads the screen.**
  - Every platform gates it: UIPI/UIAccess [14], TCC (r), Play policy [35], the
    GNOME 48 allowlist [27].
  - Every platform hides password values: UIA [12], GTK [20], AppKit [31].
- **L8 — trees help agents, but are not enough on their own.**
  - For the models tested, the tree beats screenshots alone by about 2× [58].
  - The best systems add vision for custom-drawn parts [59].
  - Tree text is cheaper than HTML, but still thousands of tokens per observation
    [58][60].

---

## 3. What fUi has today — from the code

Line numbers are for Firn `main` 7fbd6a39.

| Capability | Where | State |
|---|---|---|
| **Element tree** | `lib/fui/scene.fi`: `struct Node` (200), `struct Scene` (271), `scene_add` (440) | Retained tree of 6 kinds (`N_BOX/TEXT/IMAGE/SVG/WIDGET/VIEWPORT`). Parent, first/last child and next-sibling links. Nodes are **indices in a fixed array** `[Node; 128]`. `SCENE_MAX = 128` (154) and `KIDS_MAX = 64` children per box (159). Append-only: no remove, move or insert-before. `scene_reset` (329) and a rebuild is the update path. |
| **Style** | `lib/fui/sheet.fi`: `struct Sel` (139), `sheet_match` (694) | Rule-based stylesheet built in code (no CSS text, 20–24). A selector has a kind, an id, up to 4 classes, a state and one ancestor (descendant combinator). Specificity is id 10000 / class 100 / kind 1 (44). Five properties inherit. `RULE_MAX = 64`, `CHAIN_MAX = 16` (621–622). Class and id names are FNV-1a numbers (`sheet_name`, 110). |
| **Passes** | `scene_style` (875), `scene_measure`, `scene_layout`, `scene_draw` (1328), `scene_run` (1482) | Four ordered passes. Flexbox comes from `lib/fui/flex.fi`. A drift guard counts size changes during draw. Overflow counters report nodes, kids, depth and limits. |
| **Hit test** | `hit_node` (1348), `scene_hit` (1381), `scene_viewport_at` | Topmost node under a point; respects the viewport clip. |
| **Events** | `lib/fui/control.fi`: `struct Panel` (104), `mouse_move/down/up` (284/326/358), `key_tab` (470), `activate_focused` (571), `track_begin` (631) | One Panel with **one** hovered, pressed, focused and "track" (drag capture) widget. Events return change bits `CH_REPAINT/VALUE/ACTIVATE/FOCUS` (89–93). There is **no capture/bubble, no per-node handler and no pointer id**; the app polls (`host_take_activated`, fuiwirt 415). The file never draws (S1, line 52). |
| **Host** | `lib/plat/fuiwirt.fi` (W1–W5, 24–50) | Pointer, wheel, Tab/Enter/arrows/page keys, scale, dirty tracking, sleep while idle. No text input (N1, 54). No double click, drag-and-drop or clipboard (N2, 57). `host_touch` (176) only marks the picture dirty; it is not touch input. A raw pointer hook `host_set_hook` (r77) exists **only on branch `fui-codehub`**. |
| **Touch** | `lib/window/android.fi` `host_input` (1000) · `demos/webdemo/firn.js` (436, 440) | Android turns pointer **index 0** into a mouse (1004); `ACTION_POINTER_DOWN/UP` are ignored. The web host uses Pointer Events with `setPointerCapture` but passes only x, y, buttons and a `MOD_TOUCH` bit — no `pointerId`. So there is **no multi-touch, no pinch/rotate/long press, and no fling** (Firn r71 open). |
| **Accessibility** | `lib/fui/a11y.fi`: 31 ARIA roles (148), `struct Ann` (157), `a11y_unnamed` (615), `a11y_dump` (1328), bridge plan (1115) | Name computation: explicit, labelled-by (**by node id**, 168), own text, content. Also states, value/range, position in set, and tab order from tree order. `a11y_unnamed` counts unnamed controls. `a11y_dump` writes the whole tree as comparable text, the "oracle" for a future AT-SPI/UIA bridge. The bridge itself is only described (fUi r19). Annotations are indexed by node number, `A11Y_MAX = 128` (100). |
| **Secrets** | `lib/fui/textbuf.fi` `secret` (80), `tb_copy` (562); `lib/fui/editor.fi` (408) | A password field refuses copy. **But the accessibility export knows no secret**: a textbox's value is its widget text, written verbatim (`"Textfeld und Klappfeld: ihr Text ist der WERT"`, a11y.fi 1283). Whether a password leaks depends on what the app puts into the node. |
| **Identity** | `node_set_id` (551), `scene_find_id` (1455) | Optional `u32` id per node, **not checked for uniqueness**; lookup is linear. The node *number* changes whenever a rebuild adds nodes in a different order. |
| **Animation** | `lib/fui/anim.fi` (`ANIM_MAX = 32`, 428) | Tweens, springs and transitions; `wait_ms` sleeps when idle. |
| **Long lists** | `lib/fui/viewport.fi` | A clipped scroll area; the app builds the visible rows (virtualisation by hand). |
| **Model / paint** | `control.fi` S1; `render.fi`, `wave2/3.fi`; `node_set_draw` | Event logic is drawing-free and testable. A node holds model, style and layout; painting is done by the render modules. Custom painters may paint anything (unknown ink bounds). |
| **Who uses the tree** | `grep` over the repos | gallery9, CodeHub (branch `fui-codehub`; rebuilds ~70 nodes **every frame**, `examples/codehub/codehub.fi` 25–27), x11demo, fuidemo, the web host `lib/plat/web.fi`, and `fui.app` (branch `fui-app`, where every widget id is its node number). **Not** FirnChat, OpenPlan's canvas, `lib/fuishell` (dock, file chooser) or OrientOS (own `wlib`, fUi r41). Those paint with fUi but are invisible to the tree. |
| **Web** | `lib/plat/web.fi`, `firn.js` | Canvas only; **no ARIA mirror**. A transparent `<textarea>` lies over the painted text field for IME (Firn r135), which is a precedent for a DOM mirror. |

**What is missing, in the terms of the question:**

- touch/multi-touch and gestures;
- capture/bubble;
- queries by name, role or selector;
- a mutation observer;
- an inspector with live editing;
- stable node IDs;
- an accessibility bridge.

Found on the way:

- no `secret` in the accessibility export;
- a fixed capacity of 128 nodes;
- full style, measure and layout on every frame;
- no culling of invisible subtrees;
- large parts of the ecosystem outside the tree.

**OrientOS today**

- **S-007.** The kernel accessibility tree of branch `archiv/a11y` (28.08.) is
  **not mergeable**. The Offen list calls for a new round A11Y-2 instead: 1085
  commits of drift, plus two silent collisions (`AX_OFF` 0x7A000 = `NETDEV_OFF`;
  syscalls 1960–1965 = `SYS_OSUM_BUS`).
- **Its security design is still the best template here.** From `docs/A11Y.md` in
  that branch:
  - Reading needs a grant, and only uid 0 grants it.
  - Packages declare the `a11y` right.
  - Push is only for oneself, since the kernel stamps the owner.
  - Password fields are protected by **two locks**: Ring 3 drops the value, and the
    kernel forces the value to 0 on push. A canary word must appear exactly once in
    the whole log.
- **S-008.** Magnifier, high contrast and sticky keys; it comes after S-007.
- **Action bus.** The adapter table (`docs/ACTION-BUS.md` §8) already plans a `ui`
  adapter "by names … OrientOS's own a11y tree (S-007)" (AB-008c, OrientOS r243).

---

## 4. The measurement

### 4.1 Set-up

- **Program.** `tools/fui/apptree_main.fi`, compiled with `--opt-level=release-fast`.
- **Canvas.** 1240 × 720, painted by the CPU rasteriser with fUi's coverage cache (24
  MiB default, warm).
- **Trees.**
  - The real **gallery9** page: 56 nodes, 12 controls.
  - **Synthetic app trees**: a wrapping row of 300-point cards, each with up to 16
    rows, each row a label, a button and a checkbox. Every control has an id.
- **Query.** A prototype `querySelectorAll`. It walks the tree like `scene_style`,
  builds the same ancestor chain and calls `sheet.sheet_match`, so it uses **no
  second selector engine**.
- **Three builds** (`tools/fui/apptree.sh`):
  - `cap128`: the library as it is.
  - `big`: a *copy* of `lib/` with `SCENE_MAX = A11Y_MAX = 1024`.
  - `cull`: `big` plus the culling experiment (§4.4).
- **Runs.** 3 runs × 15 rounds, medians. The pixel checksums (`pix=`) of all builds
  must agree, and they did.
- **Conditions.** A shared 20-core server with load average 7–13 during the runs.
  Single values scatter by roughly ±15 %; the slopes are stable.

### 4.2 Bytes

| | bytes |
|---|---:|
| `scene.Node` (stride) | **760** |
| — of which `widget.Widget` (incl. its style set) | 304 |
| — `sheet.Decl` | 136 |
| — `flex.FlexBox` | 72 |
| — links, layout results, painter hook | ~248 |
| accessibility annotation per node (`A11y` / 128) | **56** |
| **per node, scene + a11y** | **~816** |
| a tree of 128 / of 1024 nodes (fixed arrays) | ~104 KB / ~835 KB |
| accessibility text dump | ~40 per node |

The comment in `scene.fi` (lines 140–145) still says 744 bytes per node, measured
on 22.09.; the node has since grown by 16 bytes.

### 4.3 Time

gallery9 (56 nodes):

| | |
|---|---:|
| build (`scene_reset` + describe) | 8.7 µs |
| style / measure / layout | 67 / 339 / 61 µs |
| draw | 6.9 ms |
| hit test | 159 ns |
| lookup by id (misses) | 50 ns |
| selector query `button` in `.leiste` (7 hits) | 12.2 µs |
| accessibility dump (3,747 B) | 107 µs |
| unnamed-control check | 3.1 µs |

Synthetic trees (build `big`, µs unless noted):

| nodes | controls | build | style | measure | layout | **tree total** | draw (ms) | hit (ns) | id (ns) | query | a11y dump | unnamed |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 14 | 6 | 2.1 | 17 | 38 | 31 | **89** | 0.67 | 24 | 23 | 3.0 | 11 | 1.4 |
| 30 | 14 | 4.7 | 35 | 67 | 51 | **158** | 1.6 | 31 | 34 | 7.2 | 25 | 2.9 |
| 62 | 30 | 9.4 | 73 | 165 | 118 | **365** | 3.4 | 51 | 49 | 14.6 | 53 | 6.2 |
| 123 | 60 | 18 | 143 | 325 | 230 | **716** | 6.2 | 93 | 74 | 28.4 | 89 | 10.1 |
| 253 | 124 | 31 | 248 | 566 | 396 | **1,241** | 9.3 | 151 | 101 | 48.5 | 183 | 21 |
| 505 | 248 | 62 | 495 | 1,124 | 794 | **2,475** | 14.1 | 222 | 207 | 111 | 436 | 51 |
| 1009 | 496 | 148 | 1,175 | 2,657 | 1,887 | **5,868** | 18.6 | 332 | 635 | 229 | 882 | 101 |

What the table says:

- **The tree work is linear, about 5.8 µs per node per frame** when everything is
  rebuilt:
  - build 0.15 µs,
  - style 1.2 µs,
  - measure 2.6 µs (text measuring),
  - layout 1.9 µs.
- **At the cap it is cheap.** At the current cap (≤128 nodes) the tree costs ≤0.7 ms
  of a 16.7 ms frame. CodeHub's choice to rebuild every frame is fine at that size.
- **At 1009 nodes it is 5.9 ms** — a third of the frame before any pixel is painted.
  A web page of that size is ordinary; Lighthouse starts warning at ~800 nodes [51].
  So a whole-app tree needs **incremental passes** (dirty bits per node) before the
  cap is raised.
- **Painting dominates.** One visible row (label + button + checkbox) costs about
  150 µs to draw: 253 nodes means 62 visible rows and 9.3 ms. The tree's own work is
  8–12 % of the frame at ≤128 nodes.
- **Everything automation needs is cheap.**
  - A full selector query costs 0.23 µs per node (0.23 ms for 1009 nodes).
  - A full accessibility dump costs 0.9 µs per node and 40 B per node.
  - A lookup by id costs under 1 µs.
  - The unnamed check costs 0.1 µs per node.
  - An agent step costs seconds of model time (§2.3 L8), so none of these needs an
    index at app scale.

### 4.4 Experiment: skip what cannot be seen (culling)

**Hypothesis.** `draw_node` walks and paints every node, including nodes outside
the window. A subtree whose ink bounds miss the current clip can be skipped with
no visible change.

**Set-up.** Before drawing, every subtree gets its ink bounds, folded from the back.
This works because children always have higher numbers than their parents. The
bounds are the node's rectangle plus its children's, with 40 points of margin for
shadows and focus rings. Transformed nodes and nodes with their own painter are
never culled. `draw_node` returns early when the bounds miss `canvas.clip_*`. See
the patch in `tools/fui/apptree.sh`, which applies only to a copy of `lib/`.
Measured 7 × alternating (9 rounds each):

| nodes | visible | draw without culling, min / median | with culling, min / median | change |
|---:|---|---:|---:|---:|
| 253 | all | 9.32 / 9.52 ms | 9.37 / 11.26 ms | none (nothing to cull) |
| 505 | partly | 13.49 / 13.65 ms | 12.86 / 13.25 ms | −3 … −5 % |
| 1009 | about half | 15.54 / 15.86 ms | 12.90 / 13.29 ms | **−16 … −17 %** |

**Result.**

- The pictures are identical: the same checksum for all builds and runs.
- Without culling, every off-screen node still costs about 5 µs of draw time.
- With culling, the 505-node and 1009-node trees (same visible picture) cost the
  same.
- **Kept as a roadmap item, not merged** at the time. A real version needed ink
  overflow from the style instead of a fixed 40-point margin. It was built
  that way on 30.09.2026 (§4.5).

**The logbook in one line each:**

- **R1.** "The tree is too expensive to be the truth." → Refuted at ≤128 nodes (<1
  ms), confirmed at ~1000 nodes (5.9 ms).
- **R2.** "Queries and dumps need an index." → Refuted (0.23 ms and 0.9 ms at 1009
  nodes).
- **R3.** "Off-screen nodes cost draw time." → Confirmed; culling gives −17 % with the
  same octets.

---

### 4.5 Built (30.09.2026): culling, the measure memo, keys, queries, secrets, inspector

The first items of §7 are in the library now, each checked by
`tools/fui/dom_main.fi` (run.sh section 18n):

- **Secrets (r99).** `widget.w_set_secret` marks a password field. The field
  paints and measures masked (one bullet per character, the same octets as a
  field that holds bullets). The accessibility export writes role, name and the
  flag `secret` only — no `value=`, so not even the length leaves. A name read
  through the field (labelled-by, a row named by its children) skips its text.
  The canary `CANARY7hunter2` is in no dump, name, query or inspector text.
- **Keys and queries (r92, r93)** in `lib/fui/query.fi`. The key is the node id
  (`sheet_name` of a word); the key path (`login/pass/field`) is the automation
  id. `query_key_dups` counts duplicates; `query_keep`/`query_restore` carry
  focus, hover, press and drag across a rebuild that moved every number.
  Queries: by `sheet.Sel` through `sheet.sel_match` (the stylesheet's own
  matcher), by role + name, and by text — `button[name="Save"]`,
  `checkbox[name*="Remember"]`, `.actions button`, `#pass textbox`,
  `checkbox:checked`. A query that is not understood finds nothing.
- **Culling (r101)** in `scene_draw`, on by default. Ink bounds per subtree:
  40 points plus twice the theme's shadow radius, plus box shadow (lift + 2 ×
  blur), text shadow and outline from the style. Transforms, offsets and
  custom painters without `node_set_ink` are never culled; offsets move the
  bounds with the paint.
- **Measure memo (r100, first part)**: `scene_attach_memo`. A leaf whose
  fingerprint (widget without its outputs, text bytes, reserved text, node
  inputs, theme shape, scale, font) did not change takes its size from the memo.
  CodeHub attaches one.
- **Inspector (r97)** in `lib/fui/inspect.fi`: pick (also boxes and texts),
  describe (kind, key path, role, name, state, box, matching rules with
  specificity, every resolved value with the winning rule or "inherited"),
  overlay, live edit of rule values and the accent.

Measured with `tools/fui/apptree.sh` (build `big`, one run, shared server
under load, so ±15 %):

| nodes | measure | measure with memo | draw with culling | draw without |
|---:|---:|---:|---:|---:|
| 123 | 305 µs | 130 µs | 5.55 ms | 5.10 ms |
| 505 | 1,111 µs | 546 µs | 14.1 ms | 14.7 ms |
| 1009 | 2,228 µs | 1,175 µs | **14.1 ms** | 17.4 ms |

- The memo halves the measure pass.
- Culling saves about 19 % of the draw at 1009 nodes, where half the tree is off
  screen. When everything is visible it gains nothing, and small trees scatter
  in both directions.
- **Not reached:** the goal of ≤2 ms tree work at 1000 nodes. Style
  (~1.1 ms) and layout (~1.7 ms) still run in full. Keeping style and layout
  results per unchanged subtree is the rest of r100.
- **CodeHub** (1996 × 1211, best of 60, three alternating runs): a full frame
  takes 5.1 / 5.4 / 4.6 ms with the memo against 5.5 / 6.0 / 6.3 ms without.
  The pictures are the same octets as main at all three sizes.

### 4.6 Built (30.09.2026, later): typing a password, the style/layout memo, events

- **Typing into a password field (r108).** The field with an editor
  (`render.draw_textfield`) and the plain buffer field (`draw_field`) paint and
  measure every typed letter as a bullet — while typing, with the caret and a
  selection — the same octets as a field that holds bullets; copy is refused.
  `dom_main` 1b checks it key by key.
- **Style, box-measure and layout memo per unchanged subtree (r109, the rest
  of r100)** in `scene.fi`. A node whose parent style key and own description
  did not change takes its resolved style from the memo; a box whose
  children's sizes and its own inputs did not change takes its size; a
  subtree whose layout fingerprint and rectangle did not change is replayed
  (moved) instead of laid out. `apptree_main` compares every rectangle and
  style of a memo frame with a frame without memo — also on hover frames with
  a hover rule that changes colour and padding.

  Measured with `tools/fui/apptree.sh` (build `big`, median of three runs,
  shared server): tree work per frame (style + measure + layout)

  | nodes | without memo | with memo |
  |---:|---:|---:|
  | 123 | 646 µs | 129 µs |
  | 505 | 3,164 µs | 495 µs |
  | 1009 | 5,414 µs | **899 µs** |

  At 1009 nodes: style 1,378 → 231 µs, measure 2,569 → 324 µs, layout
  1,757 → 16 µs. **The goal of ≤ 2 ms tree work at 1000 nodes is reached.**

- **Events through the tree (r94, r95 core, r96)** in `lib/fui/event.fi`,
  checked by `tools/fui/event_main.fi` (run.sh section 18o) with synthetic
  event streams only:
  - *Dispatch:* capture → target → bubble along the path from `scene_hit`;
    handlers per key (so a rebuild keeps them); `H_STOP`, `H_STOP_NOW`,
    `H_PREVENT`; at the target capture handlers first. The default action is
    `control.fi`, unchanged (down/move/up of the primary pointer).
  - *Pointers:* id, type (mouse/touch/pen), `primary` per type, up to 10 at
    once; pointer capture per id, implicit for touch and pen, held by key
    path so it survives a rebuild.
  - *Gestures with an arena:* tap, double tap (after its second tap, like
    click/click/dblclick), long press (500 ms, by `ev_tick` or at the up),
    pan with fling (velocity over the last 100 ms, 50..8000 px/s), pinch with
    scale and angle, pan → pinch when a second finger comes. A recogniser
    joins only when someone on the path listens; exactly one wins; a win
    cancels the press in `control.fi` (`mouse_cancel`), so no click follows.
    A control that drags (slider, scrollbar) keeps its pointer. Mouse
    pointers only get tap/double tap.
  - *Change records:* `obs_observe` once per frame, diffed by key path,
    batched: ADDED, REMOVED, MOVED (order among keyed siblings), TEXT,
    VALUE, STATE, FOCUS. A renumbering rebuild gives none. A secret field
    never gives TEXT or VALUE — not even "changed".
  - Counter-checks (the test must fail when the code is broken): reversed
    capture order, no press cancel, no implicit capture, stop-now as stop,
    no MOVED, no slider exception, no fling threshold, no secret check —
    each makes `event_main` fail.
  - **Still open (r95 rest):** the hosts do not deliver pointer ids yet.
    Android turns pointer 0 into a mouse and ignores `ACTION_POINTER_*`; the
    web host drops `pointerId`; `fuiwirt` has no router. Until then the
    gestures run in tests and in programs that feed `event.fi` themselves.

### 4.7 Built (02.10.2026): real pointers from the hosts, transitions by key, the audit

- **Real pointers (r111).** The hosts now deliver what `event.fi` was written
  for. *Browser:* `firn.js` hands the DOM `pointerId`, `pointerType`, `isPrimary`
  and `timeStamp` to `firn_web_pointer_ex` (the old `firn_web_pointer` stays for
  old modules); `pointercancel` is its own event (`WE_CANCEL`, only for pages
  that ask — others still get an up); a page that handles touch itself gets
  `touch-action: none`. *Android:* `lib/window/pointers.fi` turns one
  `AMotionEvent` — which batches all fingers into one MOVE and names the finger
  of POINTER_DOWN/UP by *index* — into one record per finger with its stable
  id, tool type, `primary` and time; `window.real_pointers` / `window.next_pointer`
  hand them to the program, in addition to the old first-finger events.
  `fuiwirt.host_pointer` feeds every pointer to the router and the primary one
  also to the old way (hover, press, scroll bars, hook); the router runs in
  *host mode* (it dispatches and cancels a press that lost the arena but does not
  run the defaults twice; `H_PREVENT` is honoured). `app.on(a, node, kinds,
  handler)` gives `fui.app` programs the events; `examples/fui/touchpad.fi` shows
  them. Checked: `pointers_main` (synthetic Android streams, run.sh 18o2),
  `tools/wasm/touchcheck.py` (real multi-touch in headless Chromium: tap, pan
  120 px, pinch 100→160 %, rotation 90°, first finger lifting first, long press,
  cancel, mouse), and `tools/android/pointers_check.sh` (raw multi-touch on an
  emulator: ids 0/1, second finger not primary, pinch 100→170 %). With it
  `fui.app` runs on Android (`lib/@android/fui/apphost.fi` is the Linux host;
  soft keyboard and lifecycle polish are r87).
- **Transitions by key path (r110).** The transition registry
  (`anim.TransReg`) found its entry by the widget's address. After a rebuild
  the address belongs to another node: the button half way to its hover colour
  jumped back and its neighbour glowed. `anim.transreg_set_keyer` +
  `query.query_widget_key` key the entry by the key path (an unkeyed widget
  still by address). `tools/fui/animkey_main.fi` (18o3) measures the painted
  pixel: with the keyer the colour after a swap-and-insert rebuild is the same
  as before (0x4A4A56 at 50 %); without it the button falls back to rest.
- **The accessibility audit (r98).** `lib/fui/audit.fi` counts operable nodes
  without a name, duplicate keys under one parent and secrets that reach the
  export (a canary is put into every secret node for one dump). Every
  `fui.app` program is run through it with `FUI_AUDIT=1` (no window, exit code
  = result): `tools/fui/audit.sh`, run.sh 18p2. It found two real gaps at once:
  the text fields of `form.fi` had no name (now the hint is the name). The
  programs with their own main call it from their checks (`gallery9_main`,
  `examples/codehub/main.fi`). A program with a nameless field must fail
  (`audit_bad.fi`).
- **The fill rule (Justin, 02.10.2026).** A bordered widget must not have the
  fill of the ground it sits on. The resting field, the focused field and the
  button each differ from `base`, `surface` and `surface_raised` — and the resting
  field from the focused one — by an OKLab distance of at least 0.012
  (`themefile.check_fill_distinct`; theme files that break it are refused with
  the pair named; `contrast_main` prints the table for the built-in schemes).
  It caught the focused field of the dark scheme (it was the page colour), the
  light one (the white of the raised card), Nord (field = surface, button = raised
  card), Solarized (field = surface), High Contrast (field = page) and the CodeHub
  theme; all were moved by one step.

## 5. The decision

### 5.1 Options

| Option | For | Against | Verdict |
|---|---|---|---|
| **A. Paint like a classic program, annotate accessibility separately** | Least change | Two sources of truth. Every program must be touched again for a11y, tests and agents (L2). The ecosystem outside the tree stays invisible. | no |
| **B. A browser engine per app** (HTML/CSS/JS, Electron-like) | Everything exists in Certus | Memory, start-up and a script engine in every app. It gives up what fUi is for (kernel profile, no allocator, same pixels native/WASM). Certus remains the tool for web content. | no |
| **C. A typed, compiled DOM-like tree** (extend `scene` + `sheet` + `a11y`) | Builds on what exists; one truth for layout, events, a11y, query, tests, inspector and agents | Needs stable keys, dispatch, gestures, incremental passes and security work (§7) | **yes** |
| **D. Immediate mode + pushed semantic tree** (AccessKit style) | Small state, fast iteration | Needs stable IDs anyway (L4). Loses queries and observers inside the app. | only as the "rebuild every frame" way of writing C: CodeHub already does it, and keyed reconciliation keeps it valid |

### 5.2 The rules (recommendation)

- **T1 One tree.**
  - Every visible element is a scene node. Accessibility, query, tests, the
    inspector and agents read the *same* nodes; `a11y.fi` already works this way and
    keeps no second tree.
  - Widgets that paint internal parts (list rows, table cells, long text) expose
    **virtual children**, as UIA does for virtualised lists (`IsOffscreen`,
    `OptimizeForVisualContent` [12]).
- **T2 Stable keys.**
  - A `key` per node, required for controls and unique among siblings. It is checked,
    not hoped for; duplicates are counted like `nodeof`.
  - The **key path** (`settings/network/save`) is the automation id and survives
    rebuilds.
  - A rebuild is reconciled by key (Flutter-style [44]), so focus, animations and
    scroll positions stay attached.
  - The node *number* stays an internal detail.
- **T3 Dispatch like the DOM.**
  - Events go capture → target → bubble along the path from `scene_hit`, with
    stop-propagation and a default action per kind.
  - Handlers are registered per key, and `control.fi` stays drawing-free.
  - Pointers carry `pointerId`, `pointerType` and `isPrimary`. `Panel.track` becomes
    pointer capture per id [47].
- **T4 Gestures.** Tap, double tap, long press, pan + fling (Firn r71), pinch and
  rotate, with an arena that lets exactly one recogniser win [40], and cancel for the
  loser [32].
  - Hosts must deliver all pointers: Android `ACTION_POINTER_*` with ids, the web
    host `pointerId`.
  - Everything is tested with synthetic event streams, not by looking.
- **T5 Query.**
  - `scene_query(sc, sel)` through `sheet_match`; this is the measured prototype.
  - Queries by role and accessible name go through `a11y`.
  - A small text syntax lets scripts and agents write `button[name="Save"]`,
    `.card checkbox` or `#key`, like Playwright's `getByRole` [60]. A grammar exists
    in `lib/css/sel.fi` / `lib/browser/domjs.fi`, but those are GC-based; fUi needs a
    no-allocation subset.
- **T6 Change records, not synchronous events.**
  - Once per frame, the tree is diffed against the previous one by key: nodes added,
    removed or moved; text, value, state or focus changed.
  - The records are *batched*, as `MutationObserver` does [46][48].
  - They feed accessibility events, the inspector and "wait until …" in tests and
    agents.
  - OrientOS' `ax_push` already derived its events from the difference in the kernel
    (A11Y.md §5).
- **T7 Semantics on demand.** Bridges (AT-SPI, UIA, web ARIA), the inspector and agent
  access switch on when a client subscribes (L3). With nobody listening, the cost is
  zero.
- **T8 Secrets.**
  - Add a `secret` state for password fields. Its value **and its length** never leave
    the process, including the dump, bridges, inspector and change records.
  - A canary test, like OrientOS' `GEHEIMNIS7`, proves that the word never appears in
    any export.
- **T9 Inspector.** An in-process developer overlay, like GtkInspector [18], Snoop
  [15] or Flutter DevTools [43]:
  - pick an element;
  - see its box, key path, role and name;
  - see the resolved style with the winning rule and its specificity;
  - see the accessibility dump;
  - live-edit style values and the theme.
  - A remote view over the bridge is read-only by default.
- **T10 Performance before capacity.**
  - Dirty bits for style, measure, layout and paint. The goal is ≤2 ms of tree work
    per frame at 1000 nodes.
  - Draw culling, with measured −17 %.
  - A heap/arena node store for the app profile; the kernel profile keeps fixed
    arrays.
  - A compact node: rare fields move to side tables.
  - Only *then* raise `SCENE_MAX` and `KIDS_MAX`.
- **T11 Model and paint stay separate.** Painters are functions of the node. Custom
  painters (`node_set_draw`) declare their ink overflow so they can be culled.

### 5.3 Tree and action bus (OrientOS)

| | the tree | the action bus |
|---|---|---|
| answers | *what is on the screen now* | *what the app can do* |
| content | Roles, names, states, values, geometry, focus | Typed actions, arguments, levels (read / write / critical) |
| for | Screen readers, magnifier (follows focus), tests, agents' **perception**, visual checks | Scripts, automations, voice, agents' **actions** |
| guarantees | Read-only by default; secrets never exported | Rights, confirmation, dry run, write-ahead audit, undo |

**Recommended link.**

- A control may carry the **action id** it triggers, for example button "Save" →
  `notes.save`. An agent then sees *where* on the screen an action lives, and calls it
  **through the bus**, with its rights, confirmation and audit.
- Acting *through the tree* ("press the button named Save", an `AX_DO`) is only the
  fallback for UI without an action.
- That fallback runs **inside the broker**, as the planned `ui` adapter AB-008c does,
  never as a side door.
- The Play policy against autonomous agents on the accessibility API [35] shows why
  the platforms want a controlled path. OrientOS' bus already is one.

### 5.4 Security

- **Reading the tree means reading the screen** (L7).
  - Rights `a11y.read` (all of it) and `a11y.act` (tree actions) are granted like
    bus rights and written to the audit log.
  - Grants can be limited to one window.
  - A grant is dropped on `exec`; this was open point 1 in A11Y.md §7.
  - Only the system screen reader gets the lock screen's tree, and nothing
    synthetic reaches locked sessions. The latter is already an ACTION-BUS rule.
- **Process boundaries.**
  - The tree lives in the app. It is exported by **push** to a snapshot held by the
    window server/kernel, which stamps the owner (`T_ORIGIN`).
  - Readers never call into the app. There are no synchronous round trips and no code
    injection (L6; [25][56]).
  - A lying app can only describe *its own* windows.
- **Passwords.** Two locks, as in A11Y.md §3.2: the toolkit drops the value, and the
  receiver forces it empty. Only the role and the name leave the process.
- **Inspector and remote.** Read-only by default. Live editing is only for dev builds
  or one's own apps.

### 5.5 Foreign programs and other platforms

- **fUi programs on Linux/Windows *export* their tree** through AT-SPI or UIA (fUi
  r19). Then Orca, Narrator, FlaUI, dogtail and agents like UFO2 [59] see them like
  any native app.
  - Firn binaries are static, and dynamic libraries are excluded (Firn r34). The
    bridges must therefore speak the protocols themselves: D-Bus over a socket for
    AT-SPI, a COM provider for UIA. That is heavy.
  - **AccessKit** [56] is the reference design to copy, not to link: push updates with
    integer IDs, and one adapter per platform.
- **Reading foreign programs** (read and drive only) is a job for the host where they
  run.
  - On Windows that means UIA (the Jarvis helper).
  - On Linux it means AT-SPI.
  - On OrientOS there is no D-Bus and no Win32 (ACTION-BUS §8, FREMDSOFTWARE). Linux
    GUI programs under `wayd` expose **no** element tree today, so the `keys`/`ui`
    adapters by coordinates remain the honest limit.

### 5.6 Web / WASM and the real DOM (Firn r177)

- **fUi on the web stays a canvas** — the same pixels as native, which is the point
  of r27/r74. Accessibility and automation come from an **ARIA mirror**:
  - Invisible, positioned DOM elements carry `role` and `aria-*`, built from the
    `a11y` tree when it is switched on. This is Flutter's approach [42].
  - Clicks and focus from assistive technology go back to the node by key.
  - The IME `<textarea>` (r135) is the existing precedent.
  - Mirror updates are **batched once per frame** into one JS call, because the
    WASM↔JS boundary is paid per call (Mozilla: 45 % less time without glue [52]).
  - Playwright, CDP and agents in the browser then see a fUi app through the normal
    accessibility tree, with `getByRole` working. This is a free automation
    interface for FirnChat.
- **Relation to r177** (Firn programs driving the *real* DOM through a `dom.*`
  library):
  - The mirror would be r177's first real user, and needs only a narrow API: create,
    set attributes and position, remove, receive events.
  - Rendering fUi *with* DOM elements and letting the browser lay them out is **not
    recommended**. Two layout engines mean pixel differences between native and web.

---

## 6. Risks

| Risk | Why it matters | Mitigation |
|---|---|---|
| Scope creep ("we build a browser") | DOM + events + observers + inspector is a browser's worth of concepts | Typed APIs only; no CSS/HTML text at run time; reuse `sheet_match`; the web engine stays Certus |
| Frame time at scale | Full rebuild is 5.8 µs per node (§4.3) | Incremental passes and culling *before* raising caps (T10) |
| Memory | 816 B per node; 10,000 nodes = 8 MB | Compact node + side tables; heap store only in the app profile |
| ID discipline | Automation is only as stable as the keys (L4) | Keys required for controls; duplicate counter; build check |
| **Leaks through the tree** | Today a textbox value is exported verbatim (§3) | `secret` state + two locks + canary test (T8) *before* any bridge ships |
| Agent overreach | Tree access = full screen access | Rights, audit, window scope; actions through the bus (§5.3, §5.4) |
| Two OrientOS trees | `wlib` (kernel, old `ax.fi` protocol) vs fUi scene until r41 | Rebuild S-007 on fUi's a11y model; do not revive `ax.fi` verbatim (its offsets and syscall numbers collide) |
| Bridges are big | AT-SPI needs D-Bus, UIA needs COM, and Firn is static | One push protocol inside; bridges as separate, later steps; `a11y_dump` stays the test oracle |
| Gesture complexity | Arbitration bugs feel broken on touch | Recogniser arena + synthetic-stream tests; Android and web hosts first |
| Measurement noise | Shared server, ±15 % | Slopes over 7 sizes; alternating runs; checksums |

---

## 7. Roadmap

**Added to the fUi roadmap (group "App-Baum"):**

| id | item | from |
|---|---|---|
| r92 | Stable keys and key paths (automation id), duplicate check, reconciled by key on rebuild | T2 |
| r93 | Query API: selector through `sheet_match`, role/name through `a11y`, small text syntax | T5, §4.3 |
| r94 | DOM-like dispatch: capture → target → bubble, handler per key, default actions | T3 |
| r95 | `pointerId`/`pointerType`/`isPrimary`, pointer capture per id, multi-touch (Android `ACTION_POINTER_*`, web `pointerId`), gestures with an arena | T3, T4 |
| r96 | Change records once per frame (diff by key, batched) → a11y events, inspector, "wait until" | T6 |
| r97 | Inspector with live editing; remote view read-only | T9 |
| r98 | Accessibility name check in the build for every fUi program (unnamed = 0, duplicate keys = 0, secret canary) | L2, §4.3 |
| r99 | `secret` state in the accessibility export (value and length never leave the process) + canary test | T8, §3 |
| r100 | Incremental passes with dirty bits (goal: 1000 nodes ≤ 2 ms tree work per frame) | T10, §4.3 |
| r101 | Draw culling by subtree ink bounds (−17 % measured), ink overflow from the style | T10, §4.4 |
| r102 | Heap/arena node store in the app profile, compact node, then larger `SCENE_MAX`/`KIDS_MAX` | T10, §4.2 |
| r103 | Accessibility export on demand as pushed updates — base for r19 (AT-SPI/UIA) and the web ARIA mirror (Firn r177) | T7, §5.5, §5.6 |
| r104 | Action id per control, linked to the OrientOS action bus | §5.3 |

**Status 30.09.2026 (§4.5, §4.6):** done — r92 (keys, key paths, duplicates,
focus/hover/press/drag kept by key), r93, r94, r96, r97, r99, r100 (measure,
style and layout memo: 0.9 ms tree work at 1009 nodes), r101. Partly done —
r95 (pointer ids, capture, gestures and arena in `event.fi`; the hosts do not
deliver pointer ids yet). Open — r98, r102, r103, r104.

**Status 02.10.2026 (§4.7):** r111 (web `pointerId`, Android fingers), r110
(transitions by key path) and r98 (audit of every fUi program) are done; r95 is
complete with them.

**Linked in the OrientOS roadmap:**

- **r44, S-007 (A11Y-2).** Build it on fUi's a11y model (r92, r96, r99, r103), and
  keep the grant and two-lock design of `archiv/a11y`.
- **r45, S-008.** The magnifier follows focus through the change records (r96).
  High contrast comes through the fUi theme file.
- **r243, AB-008c.** The `ui` adapter queries by role and name through r93, inside
  the broker.
- **r248, AB-021 (new).** Controls carry bus action ids (r104). The rights
  `a11y.read`/`a11y.act` become bus rights, with an audit log and per-window grants,
  and are dropped on `exec`.

**Firn r177** (DOM from Firn) is noted as the base of the ARIA mirror (r103).

---

## 8. How to reproduce

```
tools/fui/apptree.sh            # cap128 and big (1024), 3 runs, medians; with/without culling, memo; checksum check
tools/fui/apptree.sh --quick    # the library as it is, one run
tools/fui/run.sh                # 18m runs apptree_main once, 18n runs dom_main (secrets, keys, queries, culling, memo, inspector)
```

---

## 9. Sources

**Legend.**

- ✓ — re-read by the author on 29.09.2026.
- (r) — read by a research sub-agent on 29.09.2026.
- (spec) — standard text, not re-read.
- (ref) — reference page, not fetched.

1. Microsoft Learn, *Window Features* (child/owner windows) (r) — https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features
2. Microsoft Learn, *Using Messages and Message Queues* (r) — https://learn.microsoft.com/en-us/windows/win32/winmsg/using-messages-and-message-queues
3. R. Chen, *Windowless controls are not magic*, The Old New Thing, 11.02.2005 ✓ — https://devblogs.microsoft.com/oldnewthing/20050211-00/?p=36473
4. Microsoft Learn, *User Objects* (handle quota 200–18,000) (r) — https://learn.microsoft.com/en-us/windows/win32/sysinfo/user-objects
5. Microsoft Learn, *WM_POINTERUPDATE* (r) — https://learn.microsoft.com/en-us/windows/win32/inputmsg/wm-pointerupdate
6. Microsoft Learn, *WPF architecture* (milcore, retained composition) (r) — https://learn.microsoft.com/en-us/dotnet/desktop/wpf/advanced/wpf-architecture
7. Microsoft Learn, *Trees in WPF* (r) — https://learn.microsoft.com/en-us/dotnet/desktop/wpf/advanced/trees-in-wpf
8. Microsoft Learn, *Routed events overview (WPF)* (r) — https://learn.microsoft.com/en-us/dotnet/desktop/wpf/events/routed-events-overview
9. Microsoft Learn, *Events and routed events overview (WinUI)* (r) — https://learn.microsoft.com/en-us/windows/apps/develop/platform/xaml/events-and-routed-events-overview
10. Microsoft Learn, *UI Automation overview* (r) — https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-uiautomationoverview
11. Microsoft Learn, *UI Automation tree overview* (r) — https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-treeoverview
12. Microsoft Learn, *Automation Element Property Identifiers* (AutomationId, Name, RuntimeId, IsPassword, IsOffscreen, OptimizeForVisualContent) ✓ — https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-automation-element-propids
13. Microsoft Learn, *Caching UI Automation properties and control patterns* (r) — https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-cachingforclients
14. Microsoft Learn, *UI Automation security overview* (UIPI, UIAccess) (r) — https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-securityoverview
15. Snoop (WPF spy) (r) — https://github.com/snoopwpf/snoopwpf
16. GTK 4, *PropagationPhase* ✓ — https://docs.gtk.org/gtk4/enum.PropagationPhase.html
17. GTK 4, *Gesture* (r) — https://docs.gtk.org/gtk4/class.Gesture.html
18. GNOME, *GTK Inspector* (r) — https://developer.gnome.org/documentation/tools/inspector.html
19. GTK blog, *Accessibility in GTK 4* (r) — https://blog.gtk.org/2020/10/21/accessibility-in-gtk-4/
20. GTK source `gtktext.c` / `a11y/gtkatspitext.c` (password display text) (r) — https://gitlab.gnome.org/GNOME/gtk/-/raw/main/gtk/gtktext.c
21. Qt, *Object Trees & Ownership* (r) — https://doc.qt.io/qt-5.15/objecttrees.html
22. Qt, *Qt Quick Scene Graph Default Renderer* (r) — https://doc.qt.io/qt/qtquick-visualcanvas-scenegraph-renderer.html
23. Qt, *QAccessible* (bridges, Linux activation) ✓ — https://doc.qt.io/qt/qaccessible.html
24. KDAB, *GammaRay* (r) — https://www.kdab.com/gammaray
25. M. Campbell, GNOME accessibility blog / Newton; LWN (r) — https://blogs.gnome.org/a11y/?p=37 · https://lwn.net/Articles/971541
26. *Wayland accessibility notes* ("no authorisation controls" on the session bus) ✓ — https://github.com/splondike/wayland-accessibility-notes/blob/main/README.md
27. heise, *GNOME: between financial difficulties and technical progress* (at-spi2-core 2.56, allowlist) ✓ — https://www.heise.de/en/news/Linux-desktop-Gnome-Between-financial-difficulties-and-technical-progress-10513277.html
28. freedesktop bug 31731, *AT-SPI2 is not performant* (r) — https://bugs.freedesktop.org/show_bug.cgi?id=31731
29. Apple, *NSAccessibility* (ref) — https://developer.apple.com/documentation/appkit/nsaccessibility
30. Apple, *XCUIElementQuery* (r) — https://developer.apple.com/documentation/xcuiautomation/xcuielementquery
31. Apple TN2150, *Using Secure Event Input Fairly* (r) — https://developer.apple.com/library/mac/technotes/tn2150/_index.html
32. Android, *Manage touch events in a ViewGroup* (r) — https://developer.android.com/develop/ui/views/touch-and-input/gestures/viewgroup
33. Android, *Handle multi-touch gestures* (r) — https://developer.android.com/develop/ui/views/touch-and-input/gestures/multi
34. Android, *Optimizing view hierarchies* (r) — https://developer.android.com/topic/performance/optimizing-view-hierarchies
35. Google Play Console Help, *Use of the AccessibilityService API* ✓ — https://support.google.com/googleplay/android-developer/answer/10964491
36. Android, *Compose phases* (r) — https://developer.android.com/develop/ui/compose/phases
37. Android, *Semantics in Compose* (r) — https://developer.android.com/jetpack/compose/semantics
38. Android, *Compose testing APIs* ✓ — https://developer.android.com/develop/ui/compose/testing/apis
39. Flutter, *Architectural overview* (r) — https://docs.flutter.dev/resources/architectural-overview
40. Flutter API, *GestureArenaManager* (r) — https://api.flutter.dev/flutter/gestures/GestureArenaManager-class.html
41. Flutter API, *PipelineOwner.semanticsOwner* ✓ — https://api.flutter.dev/flutter/rendering/PipelineOwner/semanticsOwner.html
42. Flutter, *Web accessibility* ✓ — https://docs.flutter.dev/ui/accessibility/web-accessibility
43. Flutter, *DevTools Inspector* (r) — https://docs.flutter.dev/tools/devtools/inspector
44. Flutter API, *Key* (r) — https://api.flutter.dev/flutter/foundation/Key-class.html
45. WHATWG DOM Standard, *dispatching events* (spec) — https://dom.spec.whatwg.org/#concept-event-dispatch
46. WHATWG DOM Standard, *mutation observers* (spec) — https://dom.spec.whatwg.org/#mutation-observers
47. W3C, *Pointer Events Level 3*, Recommendation 30.06.2026 ✓ — https://www.w3.org/TR/pointerevents3/
48. Chrome for Developers, *Mutation events will be removed from Chrome* (Chrome 127) ✓ — https://developer.chrome.com/blog/mutation-events-deprecation
49. Chrome for Developers, *Full accessibility tree in DevTools* (r) — https://developer.chrome.com/blog/full-accessibility-tree
50. W3C, *Accessible Name and Description Computation 1.2* (r) — https://www.w3.org/TR/accname-1.2/
51. Chrome for Developers, *Avoid an excessive DOM size* (Lighthouse) ✓ — https://developer.chrome.com/docs/lighthouse/performance/dom-size
52. Mozilla Hacks, *Making WebAssembly a first-class language on the web* (02.2026) ✓ — https://hacks.mozilla.org/2026/02/making-webassembly-a-first-class-language-on-the-web/
53. Dear ImGui FAQ (ID stack) (r) — https://github.com/ocornut/imgui/blob/master/docs/FAQ.md
54. Dear ImGui issue #8022 (AccessKit) (r) — https://github.com/ocornut/imgui/issues/8022
55. Dear ImGui Test Engine (r) — https://github.com/ocornut/imgui_test_engine
56. AccessKit ✓ — https://github.com/AccessKit/accesskit
57. egui PR #2294 (AccessKit integration, merged 04.12.2022) ✓ — https://github.com/emilk/egui/pull/2294
58. Xie et al., *OSWorld* (2024) ✓ — https://arxiv.org/html/2404.07972
59. Zhang et al., *UFO2: The Desktop AgentOS* (2025) ✓ — https://arxiv.org/html/2504.14603v2
60. Microsoft, *Playwright MCP* README ✓ — https://github.com/microsoft/playwright-mcp
61. Playwright, *ARIA snapshots* (r) — https://playwright.dev/docs/aria-snapshots
62. Chrome for Developers, *Scrolling intervention* (passive listeners) (r) — https://developer.chrome.com/blog/scrolling-intervention
63. Chrome DevTools Protocol, *Accessibility domain* (r) — https://chromedevtools.github.io/devtools-protocol/tot/Accessibility

**Internal:**

- Firn `lib/fui/{scene,sheet,control,a11y,anim,viewport,textbuf,editor}.fi`,
  `lib/plat/fuiwirt.fi`, `lib/window/android.fi`, `demos/webdemo/firn.js`
  (main 7fbd6a39).
- `examples/codehub/codehub.fi` (branch fui-codehub).
- `lib/fui/app.fi` (branch fui-app).
- OrientOS `docs/ACTION-BUS.md` (main 5fb1f9e2), `docs/ALTZWEIGE-AUGUST.md` §5, and
  `docs/A11Y.md` on branch `archiv/a11y`.
- The OrientOS Offen list (S-007, S-008, R-010).
