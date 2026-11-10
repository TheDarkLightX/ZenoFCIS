"use strict";
/* The portable desk's own part of the page: the statements kept on this device, the Write tab, and the worker
   in which ZAL itself runs. portable_desk_build.py places this file before the script of review_desk.html and
   it uses that script's helpers and state (h, $, S, DESK, render, select, go, status, refusal and others), which
   exist by the time anything here is called. `review_desk.py serve` and `snapshot` never load it.
   It records no agreement, decides nothing about a statement, and sends nothing anywhere. */
const PORTABLE = (() => {
  const KEPT = "zal-portable-desk-statements", OPEN = "zal-portable-desk-open";
  const LABEL = /^[A-Za-z0-9][A-Za-z0-9 ._-]{0,79}$/;            /* the same names portable_desk.py accepts */
  const LARGEST = 65536;                                         /* ZAL reads a source of at most 64 KiB */
  const TOKENS = ["=>", "&", "|", "!", "'", "==", "!=", "<=", ">=", "->", "[", "]", ";"];
  const P = { engine: null, state: "starting", about: null, reason: null, waiting: new Map(), sent: 0, loaded: null,
              shelf: [], kept: true, at: null, draft: "", checked: null, accepted: null, error: null, busy: null, agrees: null,
              unfired: new Set(), saving: null, flagged: false, doomed: null, notice: null, nodes: null };
  const examples = () => DESK.portable.examples;
  const mine = () => (P.at && P.at.kind === "mine" ? P.shelf.find((s) => s.id === P.at.key) : null);
  const example = () => (P.at && P.at.kind === "example" ? examples().find((e) => e.file === P.at.key) : null);
  const label = () => (mine() || { name: P.at ? P.at.key : "statement.zal" }).name;
  const shown = () => DESK.bundles[0] || null;
  const changed = () => P.checked !== null && P.draft !== P.checked;
  const short = (revision) => revision.slice(0, 12);

  /* ---------- the engine: ZAL in a worker of this page ---------- */
  function fault(reason, unexpected) { const error = new Error(reason); error.unexpected = !!unexpected; return error; }
  function start() {
    P.state = "starting"; P.reason = null; P.loaded = null;
    try { P.engine = new Worker("portable_desk_engine.js", { type: "module" }); } catch (e) { fail(e.message); return; }
    P.engine.onmessage = heard;
    P.engine.onerror = (e) => fail((e && e.message) || "The worker that runs ZAL stopped");
  }
  function fail(reason) {
    P.state = "failed"; P.reason = reason; P.busy = null;
    const waiting = [...P.waiting.values()]; P.waiting.clear();
    waiting.forEach((asked) => asked.reject(fault(reason, true)));
    render();
  }
  function heard(event) {
    const said = event.data;
    if (said.type === "ready") {
      if (said.about.checker !== DESK.portable.checker) { fail("The checker in this copy is not the one the desk was built with."); return; }
      P.state = "ready"; P.about = said.about; render(); return;
    }
    if (said.type === "failed") { fail(said.reason); return; }
    const asked = P.waiting.get(said.id);
    if (!asked) return;
    P.waiting.delete(said.id);
    let told;
    try { told = JSON.parse(said.text); } catch (e) { told = { ok: false, error: "The reply from ZAL could not be read", unexpected: true }; }
    if (told.ok) asked.resolve(told.reply); else asked.reject(fault(told.error, told.unexpected));
  }
  function question(request) {
    return new Promise((resolve, reject) => {
      if (P.state === "failed") { reject(fault(P.reason, true)); return; }
      const id = ++P.sent; P.waiting.set(id, { resolve, reject });
      P.engine.postMessage({ id, request: JSON.stringify(request) });
    });
  }
  async function ask(route, body) {                              /* a question of the desk about the statement it shows */
    if (body.revision && body.revision !== P.loaded && P.accepted !== null) {   /* the engine holds another text, or was restarted: it reads this one again */
      const again = await question({ route: "open", name: label(), source: P.accepted });
      P.loaded = again.bundle ? again.bundle.revision : null;
    }
    return question({ route, ...body });
  }
  function stop() {                                              /* give up what ZAL is computing: the worker is replaced */
    if (P.engine) P.engine.terminate();
    const waiting = [...P.waiting.values()]; P.waiting.clear();
    waiting.forEach((asked) => asked.reject(fault("stopped at your request", false)));
    P.busy = null;
    start(); render();
  }

  /* ---------- the statements kept on this device ---------- */
  const valid = (s) => s && typeof s.id === "string" && typeof s.name === "string" && LABEL.test(s.name) && typeof s.source === "string" && s.source.length <= 4 * LARGEST;
  function keep() {
    try { localStorage.setItem(KEPT, JSON.stringify(P.shelf)); localStorage.setItem(OPEN, P.at ? `${P.at.kind}:${P.at.key}` : ""); P.kept = true; }
    catch (e) { P.kept = false; }
  }
  function recall() {
    try { const list = JSON.parse(localStorage.getItem(KEPT) || "[]"); P.shelf = Array.isArray(list) ? list.filter(valid) : []; return localStorage.getItem(OPEN) || ""; }
    catch (e) { P.shelf = []; P.kept = false; return ""; }
  }
  function later() { clearTimeout(P.saving); P.saving = setTimeout(keep, 400); }
  function flush() { clearTimeout(P.saving); P.saving = null; keep(); }
  function named(wanted, own) {                                  /* a name ZAL accepts that no other kept statement has */
    let base = String(wanted || "").replace(/[^A-Za-z0-9 ._-]/g, "-").replace(/^[^A-Za-z0-9]+/, "").slice(0, 70) || "statement.zal";
    if (!base.endsWith(".zal")) base += ".zal";
    const taken = new Set(P.shelf.filter((s) => s !== own).map((s) => s.name));
    for (let n = 1, name = base; ; n += 1, name = base.replace(/\.zal$/, `-${n}.zal`)) if (!taken.has(name)) return name;
  }
  function create(name, source) {
    const made = { id: Date.now().toString(36) + "-" + (P.shelf.length + 1), name: named(name), source };
    P.shelf.push(made); return made;
  }

  /* ---------- opening and checking ---------- */
  function open(kind, key, arrive) {
    const item = kind === "mine" ? P.shelf.find((s) => s.id === key) : examples().find((e) => e.file === key);
    if (!item) return false;
    if (P.at) flush();
    P.at = { kind, key }; P.draft = item.source; P.checked = null; P.accepted = null; P.error = null; P.busy = null; P.agrees = null; P.flagged = false;
    P.unfired = new Set(); P.doomed = null; P.notice = null;
    DESK.bundles = [];
    P.nodes.area.value = P.draft;
    keep();
    if (arrive) S.tab = P.draft.trim() ? (wide.matches ? "evidence" : "statement") : "write";
    if (P.draft.trim()) check(); else render();
    return true;
  }
  async function check() {                                       /* ask ZAL to read the text as it is now */
    const at = P.at, text = P.draft;
    if (!at || !text.trim() || P.busy === "check") return;
    P.busy = "check"; P.notice = null; render();
    let told;
    try { told = await question({ route: "open", name: label(), source: text }); }
    catch (e) { told = { error: { reason: e.message, unexpected: e.unexpected, stopped: e.message === "stopped at your request" } }; }
    if (P.at !== at) return;                                     /* another statement was opened meanwhile */
    P.busy = null;
    if (told.error && told.error.stopped) { render(); return; }
    P.checked = text; P.flagged = changed();
    if (told.error) { P.error = told.error; P.loaded = null; render(); if (S.tab === "write" || !shown()) P.nodes.told.scrollIntoView({ block: "nearest" }); return; }
    const carried = example();
    P.error = null; P.accepted = text; P.loaded = told.bundle.revision;
    P.agrees = carried && carried.source === text ? told.digest === carried.digest : null;
    DESK.bundles = [{ ...told.bundle, pending: told.edits_follow ? "edits" : null }];
    select(0);                                                   /* the page's own reset for a statement it has not shown before */
    if (told.edits_follow) plant(at, told.bundle.revision);
  }
  async function plant(at, revision) {                           /* the desk's planted edits follow, so the statement is on the screen first */
    let edits = null, failed = null;
    try { edits = await question({ route: "edits", revision }); } catch (e) { failed = e.message; }
    const b = shown();
    if (P.at !== at || !b || b.revision !== revision || b.pending !== "edits") return;
    DESK.bundles = [{ ...b, edits, pending: null, edits_failed: failed }];
    render();
  }
  async function fires(rule) {
    const b = shown(); let given = null;
    try { given = await ask("fires", { revision: b.revision, rule }); } catch (e) { return; }
    if (shown() !== b) return;
    if (given) tryInput(given); else { P.unfired.add(rule); render(); }
  }

  /* ---------- writing ---------- */
  function adopt() {                                             /* an example that is edited becomes the reader's own copy */
    const made = create(P.at.key, P.draft);
    P.at.kind = "mine"; P.at.key = made.id; P.agrees = null;
    P.notice = `This is now your own copy, kept on this device as ${made.name}.`;
    P.nodes.name.value = made.name; P.nodes.name.removeAttribute("disabled");
    renderTop();
  }
  function typed() {
    P.draft = P.nodes.area.value; P.doomed = null;
    if (example()) adopt();
    const own = mine(); if (own) own.source = P.draft;
    later();
    if (changed() !== P.flagged) { P.flagged = changed(); if (shown()) renderVerdict(); else bare(); }
    said();
  }
  function insert(token) {                                       /* a symbol of the typed syntax, where the caret is */
    const area = P.nodes.area, text = area.value, from = typeof area.selectionStart === "number" ? area.selectionStart : text.length, to = typeof area.selectionEnd === "number" ? area.selectionEnd : from;
    area.value = text.slice(0, from) + token + text.slice(to);
    if (area.setSelectionRange) area.setSelectionRange(from + token.length, from + token.length);
    area.focus(); typed();
  }
  function replaceText(text) { P.nodes.area.value = text; typed(); render(); }
  function rename() {
    const own = mine(), wanted = P.nodes.name.value.trim();
    if (!own) return;
    if (wanted !== own.name && LABEL.test(wanted)) own.name = named(wanted, own);
    P.nodes.name.value = own.name; flush(); renderTop();
  }
  function fresh() { flush(); const made = create("statement.zal", ""); open("mine", made.id, true); P.nodes.area.focus(); }
  function remove() {
    const own = mine();
    if (!own) return;
    if (P.doomed !== own.id) { P.doomed = own.id; said(); return; }            /* the second press deletes */
    P.shelf = P.shelf.filter((s) => s !== own); P.at = null;
    if (!(P.shelf.length && open("mine", P.shelf[0].id, true))) open("example", examples()[0].file, true);
  }
  async function picked(event) {                                 /* a file the reader chose from this device */
    const file = event.target.files && event.target.files[0];
    if (!file) return;
    const text = await file.text();
    event.target.value = "";
    if (text.length > 4 * LARGEST) { P.notice = "That file is larger than a ZAL source can be (64 KiB), so it was not opened."; said(); return; }
    flush(); open("mine", create(file.name, text).id, true);
  }
  async function copy() {
    try { await navigator.clipboard.writeText(P.draft); P.notice = "Copied."; }
    catch (e) { P.nodes.area.focus(); if (P.nodes.area.select) P.nodes.area.select(); P.notice = "The text is selected. Copy it with your device's own copy command."; }
    said();
  }
  function toLine(line) {                                        /* put the caret on the line ZAL names */
    const area = P.nodes.area, before = area.value.split("\n").slice(0, line - 1).join("\n").length + (line > 1 ? 1 : 0);
    area.focus(); if (area.setSelectionRange) area.setSelectionRange(before, before + (area.value.split("\n")[line - 1] || "").length);
  }
  function readingNow() {
    if (P.state === "starting") return "ZAL is starting on this device. The first start fetches about 14 MB; later starts use this device's own copy.";
    return "ZAL is reading your text…";
  }
  function said() {                                              /* what stands under the text; the text itself is never redrawn */
    const b = shown(), own = mine(), blank = !P.draft.trim(), out = [];
    const acts = [h("button", { class: "btn primary", id: "check", disabled: blank || P.busy === "check" || P.state === "failed", onclick: check }, P.busy === "check" ? "Checking…" : "Check"),
      P.busy === "check" || (b && b.pending) ? h("button", { class: "btn", onclick: stop }, "Stop") : null,
      changed() ? h("button", { class: "btn", onclick: () => replaceText(P.checked) }, "Undo my edits") : null,
      h("button", { class: "btn", disabled: blank, onclick: copy }, "Copy"),
      h("button", { class: "btn", onclick: () => P.nodes.file.click() }, "Open a file"),
      own ? h("button", { class: "btn", onclick: remove }, P.doomed === own.id ? "Press again to delete it" : "Delete from this device") : null];
    P.nodes.acts.replaceChildren(...acts.filter(Boolean));
    if (P.state === "failed") out.push(h("p", { class: "v-line" }, h("span", { class: "status", "data-s": "fail" }, "error"), h("span", {}, "ZAL could not start on this device, so nothing can be checked here.")),
      h("p", { class: "en", style: "font-size:15px" }, P.reason), h("div", {}, h("button", { class: "btn small", onclick: () => { start(); render(); } }, "Try again")));
    else if (P.busy === "check") out.push(h("p", { class: "v-line" }, h("span", { class: "status", "data-s": "quiet" }, P.state === "starting" ? "starting" : "checking"), h("span", {}, readingNow())));
    else if (P.error && !changed()) out.push(P.error.unexpected
        ? h("p", { class: "v-line" }, h("span", { class: "status", "data-s": "fail" }, "error"), h("span", {}, "Reading this text raised an error. This is not a refusal by ZAL, and the text may be fine."))
        : h("p", { class: "v-line" }, h("span", { class: "status", "data-s": "quiet" }, "refused"), h("span", {}, "ZAL does not accept this text.")),
      refusal(P.error, P.error.reason),
      P.error.location && P.error.location.line ? h("div", {}, h("button", { class: "btn small", onclick: () => toLine(P.error.location.line) }, `Go to line ${P.error.location.line}`)) : null);
    else if (blank) out.push(h("p", { class: "note" }, "Nothing is written yet. Write or paste a statement, or pick an example from the list at the top."));
    else if (changed() || P.checked === null) out.push(h("p", { class: "note" }, P.checked === null ? "ZAL has not read this text yet." : "You have changed the text since ZAL read it.",
      b ? ` The desk still shows revision ${short(b.revision)}.` : ""),
      b && changed() ? h("div", {}, h("button", { class: "btn small", onclick: () => go("changes") }, "See what the change would do, before checking it")) : null);
    else if (b) out.push(h("p", { class: "v-line" }, status(b.check.status), h("span", {}, `ZAL read this text. The desk shows it as revision ${short(b.revision)}.`)),
      h("div", {}, h("button", { class: "btn small", onclick: () => go(wide.matches ? "evidence" : "statement") }, wide.matches ? "See the evidence" : "Read the statement")));
    if (example()) out.push(h("p", { class: "note" }, "This is an example the desk carries. Editing it makes your own copy; the example stays as it is."));
    if (P.notice) out.push(h("p", { class: "note" }, P.notice));
    out.push(h("p", { class: "note" }, P.kept ? "Your text is kept in this browser, on this device. Nothing is sent anywhere, and the desk records no agreement."
      : "This browser is not keeping your text. Copy it somewhere before you close this page."));
    P.nodes.told.replaceChildren(...out.flat().filter(Boolean));
  }
  function build() {                                             /* the Write tab's own nodes, made once so that typing is never interrupted by a redraw */
    const area = h("textarea", { class: "src", id: "source", spellcheck: "false", autocapitalize: "off", autocomplete: "off", autocorrect: "off",
      "aria-label": "The text of the statement", placeholder: "A ZAL statement, in ZAL's English or in its typed syntax.", oninput: typed });
    const name = h("input", { class: "name", id: "label", type: "text", maxlength: 80, "aria-label": "The name of this statement on this device", onchange: rename });
    const acts = h("div", { class: "rrow" }), told = h("div", { class: "told", id: "write-said", "aria-live": "polite" });
    const file = h("input", { type: "file", id: "file", accept: ".zal,.txt,text/plain", hidden: true, onchange: picked });
    const writer = h("div", { class: "panel writer wrap", id: "writer", role: "tabpanel", "aria-labelledby": "tab-write", hidden: true }, h("section", { class: "block" }, h("h3", {}, "Write"),
      h("p", { class: "lede" }, "The text is yours. Check asks ZAL to read it. ZAL decides nothing for you, and reading a text agrees to nothing."),
      name, h("div", { class: "tokens", role: "group", "aria-label": "Put a symbol of the typed syntax where the caret is" }, TOKENS.map((t) => h("button", { class: "opt", onclick: () => insert(t) }, t))),
      area, acts, told), file);
    $("panel").parentNode.append(writer);
    P.nodes = { area, name, acts, told, file, writer };
  }
  function sync() {                                              /* after every redraw of the page */
    const writing = S.tab === "write" || !shown(), own = mine();
    if (writing) P.nodes.writer.removeAttribute("hidden"); else P.nodes.writer.setAttribute("hidden", "");
    if (writing) $("panel").setAttribute("hidden", ""); else $("panel").removeAttribute("hidden");
    P.nodes.name.value = label();
    if ($("tab-write")) $("tab-write").setAttribute("aria-controls", "writer");
    if (own) P.nodes.name.removeAttribute("disabled"); else P.nodes.name.setAttribute("disabled", "");
    said();
  }

  /* ---------- what the page asks of this file ---------- */
  function top(bar) {
    const is = (kind, key) => !!P.at && P.at.kind === kind && P.at.key === key;
    const pick = h("select", { id: "shelf", onchange: (e) => { const at = e.target.value.indexOf(":"); open(e.target.value.slice(0, at), e.target.value.slice(at + 1), true); } },
      P.shelf.length ? h("optgroup", { label: "On this device" }, P.shelf.map((s) => h("option", { value: "mine:" + s.id, selected: is("mine", s.id) }, s.name))) : null,
      h("optgroup", { label: "Examples" }, examples().map((e) => h("option", { value: "example:" + e.file, selected: is("example", e.file) }, `${e.name}  (${e.file})`))));
    bar.append(h("div", { class: "pick" }, h("label", { for: "shelf" }, "Statement"), pick, h("button", { class: "btn small", id: "new", onclick: fresh }, "New")));
  }
  function verdict(v) {                                          /* added under the verdict of the statement the desk shows */
    const b = shown();
    if (P.agrees === false) v.append(h("div", { class: "alert bad" }, "This device's result for this example is not the one recorded when the desk was built. Do not rely on this copy of the desk."));
    if (P.busy === "check") v.append(h("div", { class: "banner" }, "ZAL is reading your newer text…"));
    else if (P.error && !changed()) v.append(h("div", { class: "banner" }, `ZAL does not accept your newer text. The desk still shows revision ${short(b.revision)}, the text it last accepted.`,
      h("button", { class: "btn small", onclick: () => go("write") }, "See why")));
    else if (changed()) v.append(h("div", { class: "banner" }, `You have changed the text since ZAL read it. The desk shows revision ${short(b.revision)}.`,
      h("button", { class: "btn small", onclick: check }, "Check the new text")));
  }
  function bare() {                                              /* no statement on the desk yet: the top bar, what ZAL said, and the Write tab */
    const mark = focusMark(), v = $("verdict"), failed = P.state === "failed", wrong = P.error && !changed();
    renderTop();
    v.setAttribute("data-s", failed || (wrong && P.error.unexpected) ? "fail" : "quiet");
    v.replaceChildren(h("div", { class: "v-head" }, h("h1", { class: "v-name" }, P.at ? label() : "ZAL")),
      h("p", { class: "v-line" }, failed ? [h("span", { class: "status", "data-s": "fail" }, "error"), h("span", {}, "ZAL could not start on this device.")]
        : P.busy === "check" ? [h("span", { class: "status", "data-s": "quiet" }, P.state === "starting" ? "starting" : "checking"), h("span", {}, readingNow())]
        : wrong ? (P.error.unexpected ? [h("span", { class: "status", "data-s": "fail" }, "error"), h("span", {}, "Reading this text raised an error. This is not a refusal by ZAL.")]
          : [h("span", { class: "status", "data-s": "quiet" }, "refused"), h("span", {}, "ZAL does not accept this text. Its diagnostic is under Write.")])
        : [h("span", {}, P.draft.trim() ? "ZAL has not read this text yet." : "Nothing is written yet.")]));
    $("statement").replaceChildren(h("div", { class: "doc", id: "doc" }, h("p", { class: "empty" }, "The statement appears here, in English and in symbols, once ZAL accepts the text.")));
    $("tabs").replaceChildren(h("button", { class: "tab", role: "tab", id: "tab-write", "data-tab": "write", "aria-selected": "true", "aria-controls": "writer" }, "Write"));
    $("panel").replaceChildren();
    $("foot").replaceChildren(h("span", {}, "authority: none"), h("span", {}, "This page records no agreement."), h("span", {}, "ZAL runs on this device; nothing is sent anywhere"),
      h("span", {}, "checker ", h("code", {}, short(DESK.portable.checker))));
    sync();
    if (mark) refocus(mark);
  }
  function candidate(panel) {                                    /* on the Review a change tab: the candidate is the text under Write */
    const b = shown();
    panel.append(h("section", { class: "block" }, h("h3", {}, "Your edit"), changed()
      ? [h("p", { class: "lede" }, `Your text differs from revision ${short(b.revision)}, which the desk shows. ZAL can say which inputs your edit would decide differently before you check it. Nothing is replaced and nothing is agreed.`),
         h("div", { class: "rrow" }, h("button", { class: "btn primary", id: "compare", disabled: S.busy, onclick: () => { S.draft = P.draft; checkCandidate(); } }, S.busy ? "Checking…" : "See what my edit would change"))]
      : h("p", { class: "lede" }, "Your text is the one ZAL last read, so there is no edit to review. Change it under Write, then come back to see what the change would do.")));
  }
  function provenance() {                                        /* said beside the Python version, under Evidence */
    const runtime = DESK.portable.runtime;
    return [` as WebAssembly on this device (${runtime.package} ${runtime.version}).`,
      P.agrees === true ? " This statement is an example the desk carries, and this device's result for it is the one recorded when the desk was built." : ""];
  }
  function boot() {                                              /* called once, before the first draw */
    build();
    const last = recall(), at = last.indexOf(":");
    start();
    if (DESK.portable.kind === "web" && typeof navigator !== "undefined" && navigator.serviceWorker) navigator.serviceWorker.register("service_worker.js").catch(() => {});
    window.addEventListener("pagehide", flush);
    if (!(at > 0 && open(last.slice(0, at), last.slice(at + 1), false))) open("example", examples()[0].file, false);
  }
  return { boot, top, verdict, bare, sync, ask, candidate, fires, provenance, unfired: (rule) => P.unfired.has(rule) };
})();
