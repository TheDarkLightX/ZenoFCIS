'use strict';
// Runs the actual portable desk page script (portable_desk.js before the script of review_desk.html, joined as
// portable_desk_build.py joins them) against a small DOM double. The worker is a double too: it answers from a
// table that portable_desk.py filled natively, so every reply is ZAL's own. No browser, WebAssembly, network or
// provider. It does not test native rendering, layout, real focus behavior or the engine inside a browser.
const vm = require('node:vm');
const fs = require('node:fs');
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');

const fixture = JSON.parse(process.env.ZAL_PORTABLE_DESK_FIXTURES ? fs.readFileSync(process.env.ZAL_PORTABLE_DESK_FIXTURES, 'utf8') : execFileSync('python3', ['-c', `
import json
from pathlib import Path
import portable_desk as portable
import portable_desk_build as build
import review_desk as desk
from test_review_desk import LIMIT, OVERFLOWS
from test_portable_desk import BROKEN, UNFIRED
EDITED = LIMIT.replace('[used < 3] -> accept unlocked, used += 1', '[used < 2] -> accept unlocked, used += 1')
texts = {'limit': LIMIT, 'order': (Path('examples') / 'order.zal').read_text(), 'broken': BROKEN, 'overflows': OVERFLOWS, 'edited': EDITED, 'unfired': UNFIRED}
table, opened = {}, {}
for label, text in texts.items():
    held = portable.Desk()
    ask = lambda **request: held.exchange(json.dumps(request))
    table['open|' + text] = ask(route='open', name='statement.zal', source=text)
    opened[label] = json.loads(table['open|' + text])['reply']
    if 'bundle' not in opened[label]:
        continue
    revision = opened[label]['bundle']['revision']
    table['edits|' + revision] = ask(route='edits', revision=revision)
    for entry in opened[label]['bundle']['declared']:
        if entry['concept'] == 'rule':
            table[f"fires|{revision}|{entry['id']}"] = ask(route='fires', revision=revision, rule=entry['id'])
    model, profile = desk.read_statement(text)
    for given in desk.input_tuples(model, profile):
        table[f"explain|{revision}|" + json.dumps(given, sort_keys=True, separators=(',', ':'))] = ask(route='explain', revision=revision, input=given)
    for other in ('limit', 'edited', 'broken'):
        table[f"candidate|{revision}|" + texts[other]] = ask(route='candidate', revision=revision, source=texts[other])
about = json.loads(portable.Desk().exchange(json.dumps({'route': 'about'})))['reply']
carried = [{'file': label + '.zal', 'source': texts[label], 'name': opened[label]['bundle']['name'], 'profile': opened[label]['bundle']['profile'],
            'status': opened[label]['bundle']['check']['status'], 'digest': opened[label]['digest']} for label in ('limit', 'order')]
pages = {}
for kind, policy in (('web', build.WEB_POLICY), ('extension', None)):
    payload = {'mode': 'device', 'bundles': [], 'portable': {'version': '0.1.0', 'checker': about['checker'], 'kind': kind,
               'runtime': {'package': 'pyodide', 'version': '314.0.7', 'license': 'MPL-2.0'}, 'examples': carried}}
    index, script = build.page_files(payload, policy)
    pages[kind] = {'index': index.decode(), 'script': script.decode()}
print(json.dumps({'texts': texts, 'table': table, 'opened': opened, 'about': {**about, 'runtime': '314.0.7'}, 'pages': pages}))
`], { cwd: __dirname, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, PYTHONDONTWRITEBYTECODE: '1' } }));

// ---- a DOM double: a real tree with text, attributes and listeners; no layout, styles or browser defaults
class Text { constructor(text) { this.nodeType = 3; this.textContent = String(text); this.parentNode = null; } }
class Element {
  constructor(tag, owner) { this.nodeType = 1; this.tagName = tag.toUpperCase(); this.ownerDocument = owner; this.childNodes = []; this.attributes = {}; this.listeners = {}; this.parentNode = null; this.scrollTop = 0; this.clientWidth = 700; this.value = ''; this.clicks = 0; }
  get children() { return this.childNodes.filter((node) => node.nodeType === 1); }
  get id() { return this.attributes.id || ''; }
  get textContent() { return this.childNodes.map((node) => node.textContent).join(''); }
  set textContent(text) { this.replaceChildren(new Text(text)); }
  append(...nodes) { for (const node of nodes) { if (node.parentNode) node.parentNode.childNodes = node.parentNode.childNodes.filter((other) => other !== node); node.parentNode = this; this.childNodes.push(node); } }
  replaceChildren(...nodes) { for (const node of this.childNodes) node.parentNode = null; this.childNodes = []; this.append(...nodes); }
  setAttribute(name, value) { this.attributes[name] = String(value); if (name === 'value') this.value = String(value); }
  getAttribute(name) { return name in this.attributes ? this.attributes[name] : null; }
  removeAttribute(name) { delete this.attributes[name]; }
  addEventListener(type, listener) { (this.listeners[type] = this.listeners[type] || []).push(listener); }
  contains(node) { for (; node; node = node.parentNode) if (node === this) return true; return false; }
  focus() { if (this.ownerDocument.body.contains(this)) this.ownerDocument.activeElement = this; }
  click() { this.clicks += 1; }
  scrollIntoView() { this.ownerDocument.scrolledTo = this.id; }
}
const all = (root, test) => { const found = []; (function walk(node) { if (node.nodeType !== 1) return; if (test(node)) found.push(node); node.childNodes.forEach(walk); })(root); return found; };
const withClass = (root, name) => all(root, (node) => (node.attributes.class || '').split(' ').includes(name));
const fire = (node, type, event = {}) => { assert(node, 'no element to ' + type); const sent = { target: node, preventDefault() {}, stopPropagation() {}, ...event }; return Promise.all((node.listeners[type] || []).map((listener) => listener(sent))); };
const press = (node) => { assert(node && !('disabled' in node.attributes), 'no enabled control to press'); return fire(node, 'click'); };
const button = (root, label) => all(root, (node) => node.tagName === 'BUTTON' && node.textContent === label)[0];
const settle = async () => { for (let turn = 0; turn < 8; turn += 1) await new Promise((resolve) => setImmediate(resolve)); };
const visible = (node) => { for (; node; node = node.parentNode) if (node.attributes && 'hidden' in node.attributes) return false; return true; };
const sorted = (value) => Array.isArray(value) ? value.map(sorted) : value && typeof value === 'object' ? Object.fromEntries(Object.keys(value).sort().map((key) => [key, sorted(value[key])])) : value;

// What ZAL, run natively by portable_desk.py, replied to the same request.
function replyTo(request) {
  const key = request.route === 'open' ? 'open|' + request.source : request.route === 'explain' ? `explain|${request.revision}|${JSON.stringify(sorted(request.input))}`
    : request.route === 'candidate' ? `candidate|${request.revision}|${request.source}` : request.route === 'fires' ? `fires|${request.revision}|${request.rule}` : `${request.route}|${request.revision}`;
  assert(key in fixture.table, 'the test has no reply from ZAL for ' + key.slice(0, 90));
  return fixture.table[key];
}

function load(kind = 'web', { wide = true, hash = '', storage = 'works', seed = {}, answers = true, ready = true, change = null, clipboard = 'works' } = {}) {
  const { index, script } = fixture.pages[kind];
  const document = { activeElement: null, scrolledTo: null };
  document.createElement = (tag) => new Element(tag, document);
  document.createElementNS = (_, tag) => new Element(tag, document);
  document.createTextNode = (text) => new Text(text);
  document.documentElement = new Element('html', document);
  document.body = new Element('body', document);
  document.documentElement.append(document.body);
  document.getElementById = (id) => all(document.body, (node) => node.id === id)[0] || null;
  const markup = index.slice(index.indexOf('<body>') + 6, index.indexOf('<script'));       // the page's own static markup
  const open = [document.body];
  for (const [, closing, tag, rest] of markup.matchAll(/<(\/?)([a-z]+)([^>]*)>/g)) {
    if (closing) { open.pop(); continue; }
    const node = new Element(tag, document);
    for (const [, name, value] of rest.matchAll(/([a-z-]+)="([^"]*)"/g)) node.setAttribute(name, value);
    open[open.length - 1].append(node); open.push(node);
  }
  assert.equal(open.length, 1, 'page markup is balanced');
  const kept = { ...seed }, timers = [], workers = [], fetched = [], registered = [], copied = [], windowEvents = {}, location = { hash, reloads: 0, reload() { this.reloads += 1; } };
  const localStorage = { getItem(key) { if (storage === 'throws') throw Error('storage is blocked'); return key in kept ? kept[key] : null; },
    setItem(key, value) { if (storage !== 'works') throw Error('storage is blocked'); kept[key] = String(value); } };
  class Worker {                                                 // the engine's double: it keeps what it was sent and answers when told to
    constructor(url, options) { this.url = url; this.options = options; this.sent = []; this.stopped = false; workers.push(this); if (ready) setImmediate(() => this.ready()); }
    postMessage(message) { this.sent.push(message); if (answers && !this.stopped) setImmediate(() => this.answer(message)); }
    terminate() { this.stopped = true; }
    ready(about = fixture.about) { if (!this.stopped) this.onmessage({ data: { type: 'ready', about } }); }
    answer(message, text) { if (!this.stopped) this.onmessage({ data: { type: 'reply', id: message.id, text: text || replyTo(JSON.parse(message.request)) } }); }
    asked() { return this.sent.map((message) => JSON.parse(message.request)); }
  }
  const navigator = { serviceWorker: { register(url) { registered.push(url); return Promise.resolve(); } },
    clipboard: { writeText(text) { if (clipboard !== 'works') return Promise.reject(Error('refused')); copied.push(text); return Promise.resolve(); } } };
  const matchMedia = (query) => ({ matches: query.includes('min-width') ? wide : false, addEventListener() {} });
  const window = { matchMedia, addEventListener(type, listener) { windowEvents[type] = listener; } };
  const context = vm.createContext({ document, window, localStorage, location, navigator, Worker, console, fetch(url) { fetched.push(url); return Promise.reject(Error('No network in this deterministic test')); },
    history: { replaceState(_, __, to) { location.hash = to; } }, setInterval() { throw Error('the portable desk starts no poll'); },
    setTimeout(run) { timers.push(run); return timers.length; }, clearTimeout(id) { if (id) timers[id - 1] = null; } });
  const payload = change ? script.replace(/const DESK = (\{.*?\});\n/s, (_, data) => `const DESK = ${JSON.stringify(change(JSON.parse(data)))};\n`) : script;
  vm.runInContext(payload, context, { filename: 'desk.js' });
  const get = (id) => document.getElementById(id);
  const type = (text) => { get('source').value = text; return fire(get('source'), 'input'); };
  return { context, document, get, kept, timers, workers, fetched, registered, copied, windowEvents, location, type, run: (code) => vm.runInContext(code, context), text: (id) => get(id).textContent,
    engine: () => workers[workers.length - 1], later: () => { timers.splice(0).forEach((run) => run && run()); } };
}

const checks = [];
let finished = false;                                            // a wait that never ends would otherwise end the run quietly, as if it had passed
process.on('exit', (code) => { if (!finished && code === 0) { console.error('The checks did not run to the end.'); process.exitCode = 1; } });
const check = (name, condition) => { assert(condition, name); checks.push(name); };
const NOISE = /undefined|NaN|\[object Object\]|\bnull\b/;
const T = fixture.texts, limit = fixture.opened.limit.bundle, order = fixture.opened.order.bundle;
const plantedOf = (bundle) => JSON.parse(fixture.table['edits|' + bundle.revision]).reply;

(async () => {
  // ---- starting: the engine is a worker of the page, and the first example is read by ZAL
  const page = load('web', { ready: false, answers: false });
  const engine = page.engine();
  check('the page starts one module worker from its own file and reaches nothing else', page.workers.length === 1 && engine.url === 'portable_desk_engine.js' && engine.options.type === 'module' && page.fetched.length === 0);
  check('while ZAL starts, the page says so and offers only the Write tab', page.text('verdict').includes('ZAL is starting on this device') && page.text('tabs') === 'Write' && visible(page.get('writer')) && !visible(page.get('panel')));
  check('the first example is sent to ZAL as it is carried, under its own name', JSON.stringify(engine.asked()) === JSON.stringify([{ route: 'open', name: 'limit.zal', source: T.limit }]));
  check('the web page registers its offline copy', JSON.stringify(page.registered) === JSON.stringify(['service_worker.js']));
  engine.ready(); await settle();
  check('once ZAL is ready the page says it is reading the text', page.text('verdict').includes('ZAL is reading your text') && !page.text('verdict').includes('starting'));
  engine.answer(engine.sent[0]); await settle();
  check("the verdict leads with the statement's name and ZAL's exact status", page.text('verdict').startsWith(limit.name) && withClass(page.get('verdict'), 'status')[0].textContent === limit.check.status);
  check('the statement is shown in ZAL\'s English and in symbols beside the instruments', withClass(page.get('statement'), 'clause').length > 5 && withClass(page.get('statement'), 'fm').length > 5 && page.text('tabs') === 'WriteStatementEvidenceTry an inputReview a change');
  check('the planted edits are asked for after the statement is on the screen', engine.asked().length === 2 && engine.asked()[1].route === 'edits' && engine.asked()[1].revision === limit.revision);
  check('until they arrive the verdict and the Evidence tab say the edits are being re-checked', page.text('verdict').includes('ZAL is re-checking the planted edits') && page.text('panel').includes('ZAL is re-checking each planted edit on this device'));
  engine.answer(engine.sent[1]); await settle();
  const planted = plantedOf(limit);
  check('the planted edits that arrive are the ones ZAL re-checked', page.text('verdict').includes(`${planted.rows.length} planted defects`) && !page.text('verdict').includes('re-checking') && withClass(page.get('panel'), 'row').length === planted.rows.length);
  check("an example whose result is the build's says so under Evidence", page.text('panel').includes("this device's result for it is the one recorded when the desk was built") && page.text('panel').includes('as WebAssembly on this device (pyodide 314.0.7)'));
  check('the page names itself, grants nothing and offers no agreement', page.text('top').includes('portable desk') && page.text('top').includes('ZAL runs on this device. Nothing is sent anywhere.') &&
    page.text('foot').includes('authority: none') && page.text('foot').includes('This page records no agreement.') && !all(page.document.body, (node) => node.tagName === 'BUTTON' && /^(accept|agree|approve|export|sign|send)\b/i.test(node.textContent)).length);
  check('every message to the engine is one request of the desk and nothing else', engine.sent.every((message) => Object.keys(message).sort().join() === 'id,request' && Object.keys(JSON.parse(message.request)).every((key) => ['route', 'name', 'source', 'revision', 'input', 'rule'].includes(key))));

  // ---- asking ZAL about what the desk shows
  const live = load('web');
  await settle();
  const tab = (id) => press(live.get('tab-' + id)).then(settle);
  await tab('try');
  const explained = live.engine().asked().filter((request) => request.route === 'explain');
  check('an input is explained by asking ZAL, with the revision the desk shows', explained.length === 1 && explained[0].revision === limit.revision && withClass(live.get('panel'), 'say')[0].textContent === JSON.parse(replyTo(explained[0])).reply.sentence);
  await press(withClass(live.get('panel'), 'opt').find((node) => node.textContent === 'withdraw')); await settle();
  const second = live.engine().asked().filter((request) => request.route === 'explain')[1];
  check('each new input is asked of ZAL', second.input.event === 'withdraw' && withClass(live.get('panel'), 'say')[0].textContent === JSON.parse(replyTo(second)).reply.sentence);
  const openLine = async (key) => { const line = all(live.get('statement'), (node) => node.attributes['data-clause'] === key)[0]; await press(withClass(line, 'cl-head')[0]); await settle(); return all(live.get('statement'), (node) => node.attributes['data-clause'] === key)[0]; };
  await press(button(await openLine('rule:lock_it'), 'Show a step where it fires')); await settle();
  check('a step where a rule fires is asked of ZAL and opened', live.engine().asked().some((request) => request.route === 'fires' && request.rule === 'lock_it') && withClass(live.get('panel'), 'say')[0].textContent.includes('rule lock_it'));
  await tab('changes');
  check('with the text unchanged there is no edit to review', live.text('panel').includes('there is no edit to review') && !live.get('compare') && !live.get('candidate'));
  check('the planted edits are listed for review', withClass(live.get('panel'), 'chg').length === planted.rows.length);

  // ---- writing
  await tab('write');
  const area = live.get('source');
  check('the Write tab shows the text ZAL read and says so', visible(live.get('writer')) && !visible(live.get('panel')) && area.value === T.limit && live.text('write-said').includes(`ZAL read this text. The desk shows it as revision ${limit.revision.slice(0, 12)}`));
  check('an example says that editing it makes a copy', live.text('write-said').includes('Editing it makes your own copy') && 'disabled' in live.get('label').attributes);
  area.focus();
  await live.type(T.edited);
  check('nothing is sent to ZAL by typing', !live.engine().asked().some((request) => request.source === T.edited));
  check('typing in an example makes a copy kept on this device and leaves the example alone', live.run('DESK.portable.examples[0].source') === T.limit && live.text('shelf').startsWith('limit.zal') && live.text('write-said').includes('your own copy, kept on this device as limit.zal') && !('disabled' in live.get('label').attributes));
  check('typing redraws neither the text box nor the statement, and the keyboard stays in the box', live.get('source') === area && live.document.activeElement === area && area.value === T.edited);
  check('the verdict says the text has changed since ZAL read it', live.text('verdict').includes(`You have changed the text since ZAL read it. The desk shows revision ${limit.revision.slice(0, 12)}`));
  check('the text is written to this browser shortly after typing, not on every key', !(live.kept['zal-portable-desk-statements'] || '').includes('used < 2') && (live.later(), (JSON.parse(live.kept['zal-portable-desk-statements'] || '[]')[0] || {}).source === T.edited) && live.kept['zal-portable-desk-open'].startsWith('mine:'));
  await tab('changes');
  await press(live.get('compare')); await settle();
  const compared = live.engine().asked().filter((request) => request.route === 'candidate');
  check('reviewing an edit asks ZAL to compare the text under Write with the revision shown, and replaces nothing', compared.length === 1 && compared[0].source === T.edited && compared[0].revision === limit.revision &&
    live.text('panel').includes('What your candidate would change') && live.text('panel').includes('Your candidate fails') && live.run('DESK.bundles[0].revision') === limit.revision && live.get('source').value === T.edited);
  await tab('write');
  await press(button(live.get('writer'), 'Undo my edits')); await settle();
  check('Undo my edits brings back the text ZAL read', live.get('source').value === T.limit && !button(live.get('writer'), 'Undo my edits') && !live.text('verdict').includes('You have changed the text'));
  await press(button(live.get('writer'), '=>'));
  check('a symbol button puts the symbol into the text', live.get('source').value === T.limit + '=>' && live.document.activeElement === live.get('source'));

  // ---- a text ZAL refuses, with an accepted one already on the desk
  await live.type(T.broken);
  await press(live.get('check')); await settle();
  const located = fixture.opened.broken.error;
  check("a refused text shows ZAL's located diagnostic under Write", ['ZAL does not accept this text.', `line ${located.location.line}`, located.source_line, located.reason, 'ZAL suggests: ' + located.suggested_fix, `Go to line ${located.location.line}`].every((text) => live.text('write-said').includes(text)));
  check('the desk keeps showing the text ZAL last accepted and says which', live.text('verdict').includes(`ZAL does not accept your newer text. The desk still shows revision ${limit.revision.slice(0, 12)}`) && live.run('DESK.bundles[0].revision') === limit.revision && live.document.scrolledTo === 'write-said');
  await tab('try');
  await press(withClass(live.get('panel'), 'opt').find((node) => node.textContent === 'reset')); await settle();
  const reread = live.engine().asked().slice(-2);
  check('a question about the text on the desk has ZAL read that text again first', reread[0].route === 'open' && reread[0].source === T.limit && reread[1].route === 'explain' && reread[1].revision === limit.revision && withClass(live.get('panel'), 'say').length === 1);
  await tab('write');
  await live.type(T.overflows);
  await press(button(live.get('verdict'), 'Check the new text')); await settle();
  const failing = fixture.opened.overflows.bundle;
  check("a text ZAL accepts replaces what the desk shows, with ZAL's own status", live.run('DESK.bundles[0].revision') === failing.revision && withClass(live.get('verdict'), 'status')[0].textContent === 'counterexample' && live.text('write-said').includes('ZAL read this text.'));
  check('a statement that does not pass has no planted edits and none are asked for', !live.engine().asked().some((request) => request.route === 'edits' && request.revision === failing.revision) && (await tab('evidence'), live.text('panel').includes('No defects were planted, because the statement itself does not pass.')));

  // ---- a new statement, its name, a file, the clipboard, deleting
  await press(live.get('new')); await settle();
  const sentBefore = live.engine().sent.length;
  check('New opens a blank statement under Write and asks ZAL nothing', live.get('source').value === '' && live.get('label').value === 'statement.zal' && 'disabled' in live.get('check').attributes && live.text('verdict').includes('Nothing is written yet') &&
    live.text('tabs') === 'Write' && live.engine().sent.length === sentBefore && live.document.activeElement === live.get('source'));
  live.get('label').value = 'daily limit.zal'; await fire(live.get('label'), 'change');
  check('a statement can be renamed, and its name is kept', live.text('shelf').includes('daily limit.zal') && JSON.parse(live.kept['zal-portable-desk-statements']).some((kept) => kept.name === 'daily limit.zal'));
  live.get('label').value = '../etc/passwd'; await fire(live.get('label'), 'change');
  check('a name ZAL would not accept is not taken', live.get('label').value === 'daily limit.zal');
  await live.type(T.limit); await press(live.get('check')); await settle();
  check('a written text is sent to ZAL under its name when Check is pressed', (() => { const sent = live.engine().asked().filter((request) => request.route === 'open').pop(); return sent.name === 'daily limit.zal' && sent.source === T.limit; })() && live.run('DESK.bundles[0].revision') === limit.revision);
  check("a text of the reader's own is not compared with a carried example", (await tab('evidence'), !live.text('panel').includes('recorded when the desk was built')));
  await tab('write');
  await press(button(live.get('writer'), 'Copy')); await settle();
  check('Copy hands the text to the clipboard', live.copied.length === 1 && live.copied[0] === T.limit && live.text('write-said').includes('Copied.'));
  await press(button(live.get('writer'), 'Open a file'));
  check('Open a file asks the browser for one', live.get('file').clicks === 1);
  await fire(live.get('file'), 'change', { target: { files: [{ name: 'my order (v2).zal', text: async () => T.order }], value: 'x' } }); await settle();
  check('a chosen file becomes a statement on this device, under a name ZAL accepts, and ZAL reads it', live.get('label').value === 'my order -v2-.zal' && live.get('source').value === T.order && live.run('DESK.bundles[0].revision') === order.revision);
  const count = () => JSON.parse(live.kept['zal-portable-desk-statements']).length;
  await tab('write');
  const before = count();
  await press(button(live.get('writer'), 'Delete from this device'));
  check('deleting asks to be pressed again', count() === before && !!button(live.get('writer'), 'Press again to delete it'));
  await press(button(live.get('writer'), 'Press again to delete it')); await settle();
  check('the second press deletes the statement from this device and opens another', count() === before - 1 && !live.text('shelf').includes('my order -v2-.zal') && live.get('source').value !== T.order);
  check('nothing in the page reads as a placeholder', !NOISE.test(live.document.body.textContent) && live.fetched.length === 0);

  // ---- nothing accepted yet: a refused text, an error, a rule that never fires
  const kept = (items, at) => ({ 'zal-portable-desk-statements': JSON.stringify(items), 'zal-portable-desk-open': at });
  const refused = load('web', { seed: kept([{ id: 'a', name: 'mine.zal', source: T.broken }, { id: 'b', name: 'other.zal', source: T.unfired }], 'mine:a') });
  await settle();
  check("a returning reader's statements and the one that was open come back", refused.text('shelf').startsWith('mine.zalother.zal') && refused.get('source').value === T.broken && refused.engine().asked()[0].name === 'mine.zal');
  check('a refused text with nothing accepted before shows only the Write tab and the diagnostic', refused.text('tabs') === 'Write' && refused.text('verdict').includes('ZAL does not accept this text. Its diagnostic is under Write.') && refused.text('write-said').includes(located.reason) && refused.text('statement').includes('once ZAL accepts the text'));
  await refused.type(T.broken + '\n');
  check('once the refused text is edited the page no longer calls it refused', refused.text('verdict').includes('ZAL has not read this text yet.') && !refused.text('write-said').includes('ZAL does not accept'));
  await fire(refused.get('shelf'), 'change', { target: { value: 'mine:b' } }); await settle();
  const idle = all(refused.get('statement'), (node) => node.attributes['data-clause'] === 'rule:unfired')[0];
  await press(withClass(idle, 'cl-head')[0]); await settle();
  await press(button(refused.get('statement'), 'Show a step where it fires')); await settle();
  check('a rule that no declared input fires says so, by ZAL\'s own answer', all(refused.get('statement'), (node) => node.attributes['data-clause'] === 'rule:unfired')[0].textContent.includes('No declared input makes this rule fire.'));
  const faulty = load('web', { answers: false });
  await settle();
  faulty.engine().answer(faulty.engine().sent[0], JSON.stringify({ ok: true, reply: { error: { reason: 'TypeError: planted', unexpected: true } } })); await settle();
  check('an error is not presented as a refusal by ZAL', faulty.text('verdict').includes('This is not a refusal by ZAL') && faulty.text('write-said').includes('TypeError: planted') && !faulty.document.body.textContent.includes('ZAL does not accept'));

  // ---- replies that arrive late, stopping, and an engine that does not start
  const slow = load('web', { answers: false });
  await settle();
  const first = slow.engine().sent[0];
  await fire(slow.get('shelf'), 'change', { target: { value: 'example:order.zal' } }); await settle();
  slow.engine().answer(first); await settle();
  check('a reply for a statement that is no longer open is not shown', slow.run('DESK.bundles.length') === 0 && slow.text('verdict').includes('ZAL is reading your text'));
  slow.engine().answer(slow.engine().sent[1]); await settle();
  check('the reply for the open statement is shown', slow.run('DESK.bundles[0].revision') === order.revision && slow.get('source').value === T.order);
  await slow.type(T.limit); press(slow.get('check')); await settle();   // not awaited: this engine answers only when told to
  slow.engine().answer(slow.engine().sent[3]); await settle();
  slow.engine().answer(slow.engine().sent[2]); await settle();     // the edits planted for the earlier text arrive only now
  check('planted edits that arrive for an earlier text are not shown with the current one', slow.run('DESK.bundles[0].revision') === limit.revision && slow.run('DESK.bundles[0].pending') === 'edits' && slow.run('DESK.bundles[0].edits') === null);
  const working = slow.engine();
  await press(slow.get('tab-write')); await settle();
  await press(button(slow.get('writer'), 'Stop')); await settle();
  check('Stop ends the worker, starts another and says the edits were not completed', working.stopped && slow.workers.length === 2 && !slow.engine().stopped && (await press(slow.get('tab-evidence')), await settle(), slow.text('panel').includes('The planted edits were not completed: stopped at your request')));
  const dead = load('web', { ready: false, answers: false });
  dead.engine().onmessage({ data: { type: 'failed', reason: 'WebAssembly is not allowed here' } }); await settle();
  check('an engine that does not start is reported with its reason, and nothing can be checked', dead.text('verdict').includes('ZAL could not start on this device.') && dead.text('write-said').includes('WebAssembly is not allowed here') && 'disabled' in dead.get('check').attributes);
  await press(button(dead.get('writer'), 'Try again')); await settle();
  check('Try again starts the engine anew', dead.workers.length === 2);
  const other = load('web', { ready: false, answers: false });
  other.engine().ready({ ...fixture.about, checker: '0'.repeat(64) }); await settle();
  check('a checker other than the one the desk was built with is not used', other.text('write-said').includes('The checker in this copy is not the one the desk was built with.') && 'disabled' in other.get('check').attributes);
  const differs = load('web', { change: (data) => { data.portable.examples[0].digest = '0'.repeat(64); return data; } });
  await settle();
  check("an example whose result differs from the build's is called out", differs.text('verdict').includes("This device's result for this example is not the one recorded when the desk was built. Do not rely on this copy of the desk."));

  // ---- storage that fails, a narrow screen, the extension
  const blocked = load('web', { storage: 'read-only' });
  await settle();
  await blocked.type(T.edited);
  blocked.later();
  check('a browser that will not keep the text is named, and the page goes on', blocked.text('write-said').includes('This browser is not keeping your text.') && blocked.get('source').value === T.edited);
  check('a browser whose storage throws still opens the desk', (() => { const closed = load('web', { storage: 'throws' }); return closed.text('tabs') === 'Write' && closed.engine().asked()[0].source === T.limit; })());
  check('kept statements that cannot be read are left out, not shown', (() => { const odd = load('web', { seed: kept([{ id: 'a', name: '../x', source: T.limit }, { id: 7 }, null], 'mine:a') }); return odd.text('shelf').startsWith('daily_limit') && odd.engine().asked()[0].name === 'limit.zal'; })());
  const narrow = load('web', { wide: false });
  await settle();
  check('on a narrow screen the statement is read first and Write is one tab away', narrow.text('tabs').startsWith('WriteStatementEvidenceTry an inputReview a change') && withClass(narrow.get('panel'), 'clause').length > 5 && !visible(narrow.get('writer')));
  await press(narrow.get('tab-write')); await settle();
  check('the Write tab replaces the panel and keeps its own text box', visible(narrow.get('writer')) && !visible(narrow.get('panel')) && narrow.get('source').value === T.limit);
  const extension = load('extension');
  await settle();
  check('the extension page registers no offline copy and behaves the same', extension.registered.length === 0 && extension.run('DESK.bundles[0].revision') === limit.revision && extension.fetched.length === 0);
  await narrow.type(T.edited); (narrow.windowEvents.pagehide || (() => {}))();
  check('the text is written when the page is left, without waiting', (narrow.kept['zal-portable-desk-statements'] || '').includes('used < 2'));

  finished = true;
  console.log(`${checks.length} portable desk checks passed (DOM double and a worker double answering from ZAL's native replies; no browser, WebAssembly or network)`);
})().catch((error) => { console.error(error); process.exit(1); });
