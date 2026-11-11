'use strict';
// Runs the actual review desk page script against a small DOM double, with fixtures made by the Python domain.
// No browser, Playwright, network or provider. It does not test native rendering, layout or real focus behavior.
const vm = require('node:vm');
const fs = require('node:fs');
const assert = require('node:assert/strict');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

const fixture = JSON.parse(process.env.ZAL_REVIEW_DESK_FIXTURES ? fs.readFileSync(process.env.ZAL_REVIEW_DESK_FIXTURES, 'utf8') : execFileSync('python3', ['-c', `
import json
from pathlib import Path
from unittest import mock
import diagnostics
import review_desk as desk
from behavior import Refusal
from test_review_desk import LIMIT, OVERFLOWS, REFUND, STARVED, LEDGER, UNRESOLVED, TOUR
from workflow import checker_identity
example = lambda name: (Path('examples') / name).read_text()
sources = [('limit.zal', LIMIT), ('order.zal', example('order.zal')), ('refund.zal', REFUND), ('starved.zal', STARVED),
           ('ledger.zal', LEDGER), ('unresolved.zal', UNRESOLVED), ('overflows.zal', OVERFLOWS),
           ('keeper2.zal', example('keeper2.zal')), ('ladder_symmetry2.zal', example('ladder_symmetry2.zal')), ('tour.zal', TOUR)]
bundles = [desk.statement_bundle(text, name)[0] for name, text in sources]
# A live page whose statement has no table in the page, so every decision is asked of ZAL.
with mock.patch.object(desk, 'MAX_TABLE_TUPLES', 0):
    asked = desk.statement_bundle(LIMIT, 'limit.zal')[0]
model, profile = desk.read_statement(LIMIT)
checker = checker_identity()
answers = {}
for given in desk.input_tuples(model, profile):
    told = desk.explain_input(model, profile, given, checker)
    told['sentence'] = desk.decision_sentence(model, profile, given)
    told['nearest'] = desk.nearest_opposite(model, profile, given, told['outcome'], checker)
    answers[desk.input_key(asked['axes'], given)] = told
broken = LIMIT.replace('rule take:', 'rule take now:')
candidates = {'different_profile': desk.compare_candidate(model, profile, example('order.zal'), checker)[0],
              'different_domain': desk.compare_candidate(model, profile, LIMIT.replace('0..=3', '0..=4'), checker)[0],
              'unsupported': desk.compare_candidate(model, profile, UNRESOLVED, checker)[0],
              'same': desk.compare_candidate(model, profile, asked['symbolic'], checker)[0],
              'overflows': desk.compare_candidate(model, profile, asked['symbolic'].replace('[used < 3]', '[true]'), checker)[0],
              'freer': desk.compare_candidate(model, profile, asked['symbolic'].replace('rule lock_it: unlocked + lock [true] -> accept locked;\\n', ''), checker)[0],
              'refused': desk.compare_candidate(model, profile, asked['symbolic'].replace('rule take:', 'rule take now:'), checker)[0]}
try:
    desk.read_statement(broken)
except Refusal as error:
    located = diagnostics.describe(error, broken, Path('limit.zal'))
page = lambda payload, token='': desk.page(payload, token)
print(json.dumps({
    'bundles': bundles, 'asked': asked, 'answers': answers, 'candidates': candidates, 'located': located,
    'structural': sorted(desk.STRUCTURAL_CHECKS),
    'snapshot': page({'mode': 'snapshot', 'bundles': bundles}),
    'live': page({'mode': 'live', 'file': 'limit.zal', 'bundles': [bundles[0]]}, 'page-token'),
    'live_asked': page({'mode': 'live', 'file': 'limit.zal', 'bundles': [asked]}, 'page-token'),
    'live_refused': page({'mode': 'live', 'file': 'limit.zal', 'bundles': [], 'error': located}, 'page-token'),
    'live_error': page({'mode': 'live', 'file': 'limit.zal', 'bundles': [], 'error': {'reason': 'TypeError: planted', 'unexpected': True}}, 'page-token'),
}))
`], { cwd: __dirname, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, PYTHONDONTWRITEBYTECODE: '1' } }));

// ---- a DOM double: a real tree with text, attributes and listeners; no layout, styles or browser defaults
class Text { constructor(text) { this.nodeType = 3; this.textContent = String(text); this.parentNode = null; } }
class Element {
  constructor(tag, owner) { this.nodeType = 1; this.tagName = tag.toUpperCase(); this.ownerDocument = owner; this.childNodes = []; this.attributes = {}; this.listeners = {}; this.parentNode = null; this.scrollTop = 0; this.clientWidth = 700; this.value = ''; }
  get children() { return this.childNodes.filter((node) => node.nodeType === 1); }
  get id() { return this.attributes.id || ''; }
  get textContent() { return this.childNodes.map((node) => node.textContent).join(''); }
  set textContent(text) { this.replaceChildren(new Text(text)); }
  append(...nodes) { for (const node of nodes) { if (node.parentNode) node.parentNode.childNodes = node.parentNode.childNodes.filter((other) => other !== node); node.parentNode = this; this.childNodes.push(node); } }
  replaceChildren(...nodes) { for (const node of this.childNodes) node.parentNode = null; this.childNodes = []; this.append(...nodes); }
  setAttribute(name, value) { this.attributes[name] = String(value); if (name === 'value') this.value = String(value); }
  getAttribute(name) { return name in this.attributes ? this.attributes[name] : null; }
  addEventListener(type, listener) { (this.listeners[type] = this.listeners[type] || []).push(listener); }
  contains(node) { for (; node; node = node.parentNode) if (node === this) return true; return false; }
  focus() { if (this.ownerDocument.body.contains(this)) this.ownerDocument.activeElement = this; }
  scrollIntoView() { this.ownerDocument.scrolledTo = this.id; }
}
const all = (root, test) => { const found = []; (function walk(node) { if (node.nodeType !== 1) return; if (test(node)) found.push(node); node.childNodes.forEach(walk); })(root); return found; };
const withClass = (root, name) => all(root, (node) => (node.attributes.class || '').split(' ').includes(name));
const tagged = (root, tag) => all(root, (node) => node.tagName === tag.toUpperCase());
const fire = (node, type, event = {}) => { assert(node, 'no element to ' + type); const sent = { target: node, preventDefault() {}, stopPropagation() {}, ...event }; return Promise.all((node.listeners[type] || []).map((listener) => listener(sent))); };
const press = (node) => fire(node, 'click');
const button = (root, label) => all(root, (node) => node.tagName === 'BUTTON' && node.textContent === label)[0];
const textOf = (node) => (node ? node.textContent : '');
const said = (page) => textOf(withClass(page.get('panel'), 'say')[0]);   // the result sentence, or '' while none is shown
const settle = async () => { for (let turn = 0; turn < 6; turn += 1) await new Promise((resolve) => setImmediate(resolve)); };
const classed = (node, name) => (node.attributes.class || '').split(' ').includes(name);
const lineOf = (root, key) => all(root, (node) => node.attributes['data-clause'] === key && classed(node, 'clause'))[0];   // one line of the statement
const headOf = (line) => withClass(line, 'cl-head')[0];
const keyed = (root, key) => all(root, (node) => node.attributes['data-k'] === key);
const plainOf = (segments) => (segments || []).filter((segment) => segment[2] !== 'note').map((segment) => segment[0]).join('');
const keysOf = (entry) => [...new Set([...(entry.formal || []), ...(entry.more || [])].map((segment) => segment[1]).filter(Boolean))];

function load(html, { wide = true, hash = '', storage = 'works', answer = null, seed = {} } = {}) {
  const document = { activeElement: null, scrolledTo: null };
  document.createElement = (tag) => new Element(tag, document);
  document.createElementNS = (_, tag) => new Element(tag, document);
  document.createTextNode = (text) => new Text(text);
  document.documentElement = new Element('html', document);
  document.body = new Element('body', document);
  document.documentElement.append(document.body);
  document.getElementById = (id) => all(document.body, (node) => node.id === id)[0] || null;
  // The page's own static markup, read from the file between <body> and <script>.
  const markup = html.slice(html.indexOf('<body>') + 6, html.indexOf('<script>'));
  const open = [document.body];
  for (const [, closing, tag, rest] of markup.matchAll(/<(\/?)([a-z]+)([^>]*)>/g)) {
    if (closing) { open.pop(); continue; }
    const node = new Element(tag, document);
    for (const [, name, value] of rest.matchAll(/([a-z-]+)="([^"]*)"/g)) node.setAttribute(name, value);
    open[open.length - 1].append(node); open.push(node);
  }
  assert.equal(open.length, 1, 'page markup is balanced');
  const kept = { ...seed }, calls = [], timers = [], windowEvents = {}, location = { hash, reloads: 0, reload() { this.reloads += 1; } };
  const localStorage = { getItem(key) { if (storage === 'throws') throw Error('storage is blocked'); return key in kept ? kept[key] : null; }, setItem(key, value) { if (storage === 'throws') throw Error('storage is blocked'); kept[key] = String(value); } };
  const fetch = (url, options = {}) => { const call = { url, method: options.method || 'GET', headers: options.headers || {}, body: options.body ? JSON.parse(options.body) : null }; calls.push(call);
    return Promise.resolve(answer ? answer(call) : { status: 500, body: { error: 'No network in this deterministic test' } }).then(({ status = 200, body }) => ({ ok: status === 200, status, json: async () => body })); };
  const matchMedia = (query) => ({ matches: query.includes('min-width') ? wide : false, addEventListener() {} });
  const window = { matchMedia, addEventListener(type, listener) { windowEvents[type] = listener; } };
  const context = vm.createContext({ document, window, localStorage, location, fetch, console, history: { replaceState(_, __, to) { location.hash = to; } }, setInterval(tick) { timers.push(tick); return timers.length; } });
  const script = html.slice(html.indexOf('<script>') + 8, html.lastIndexOf('</script>'));
  vm.runInContext(script, context, { filename: 'review_desk.html' });
  const get = (id) => document.getElementById(id);
  return { context, document, get, calls, timers, kept, location, windowEvents, run: (code) => vm.runInContext(code, context), text: (id) => get(id).textContent };
}

const checks = [];
let finished = false;                                            // a wait that never ends would otherwise end the run quietly, as if it had passed
process.on('exit', (code) => { if (!finished && code === 0) { console.error('The checks did not run to the end.'); process.exitCode = 1; } });
const check = (name, condition) => { assert(condition, name); checks.push(name); };
const count = (bundle, kind) => bundle.edits.rows.filter((row) => row.class === kind).length;
const named = (name) => fixture.bundles.findIndex((bundle) => bundle.file === name);
const NOISE = /undefined|NaN|\[object Object\]|\bnull\b/;

(async () => {
  // ---- a snapshot, wide layout: the statement beside the instruments
  const page = load(fixture.snapshot);
  const pick = (name) => fire(page.get('model'), 'change', { target: { value: String(named(name)) } });
  const tab = (id) => press(page.get('tab-' + id));
  check('page and Python agree on which checks are structural', JSON.stringify(Object.keys(page.run('CHECKS')).filter((key) => page.run('CHECKS')[key][1] === 's').sort()) === JSON.stringify(fixture.structural));
  check('snapshot asks the server nothing and starts no poll', page.calls.length === 0 && page.timers.length === 0);
  check('page and Python name every input the same way', fixture.bundles.every((bundle, index) => bundle.decisions.status !== 'complete' ||
    Object.keys(bundle.decisions.index).every((key) => page.run(`keyOf(DESK.bundles[${index}], parseKey(DESK.bundles[${index}], ${JSON.stringify(key)}))`) === key)));

  for (const [index, bundle] of fixture.bundles.entries()) {
    await fire(page.get('model'), 'change', { target: { value: String(index) } });
    const verdict = page.text('verdict'), statement = page.text('statement');
    check(`${bundle.file}: the verdict leads with the machine's name and ZAL's exact status`, verdict.startsWith(bundle.name) && withClass(page.get('verdict'), 'status')[0].textContent === bundle.check.status);
    check(`${bundle.file}: every declaration is shown in ZAL's English and in symbols, or as ZAL prints it when it has no formula form`, bundle.declared.filter((entry) => !['header', 'name'].includes(entry.concept))
      .every((entry) => statement.includes(entry.formal ? plainOf(entry.formal) : entry.symbolic) && (entry.id ? statement.includes(entry.english.slice(entry.english.indexOf(': ') + 2)) : statement.includes(entry.english))));
    check(`${bundle.file}: every symbol and name of a line is an element the reading key can point at`, bundle.declared.every((entry, at) => !entry.formal ||
      keysOf(entry).every((key) => keyed(lineOf(page.get('statement'), `${entry.concept}:${entry.id || at}`), key).length >= 1)));
    check(`${bundle.file}: the statement shows no leaked placeholder`, !NOISE.test(statement));
    check(`${bundle.file}: footer keeps authority none, no agreement, revision and checker`, ['authority: none', 'This page records no agreement.', bundle.revision.slice(0, 12), bundle.checker.slice(0, 12)].every((text) => page.text('foot').includes(text)));
    for (const id of ['evidence', 'try', 'changes']) {
      await tab(id);
      check(`${bundle.file}: ${id} renders without leaked placeholders`, page.text('panel').length > 40 && !NOISE.test(page.text('panel')) && !NOISE.test(verdict));
    }
    if (bundle.edits) {
      await tab('evidence');
      const rows = withClass(page.get('panel'), 'row'), summary = withClass(page.get('verdict'), 'alert').map((node) => node.textContent).join(' | ');
      check(`${bundle.file}: one matrix row for every planted edit`, rows.length === bundle.edits.rows.length);
      check(`${bundle.file}: the summary counts are the planted edits' classes`, bundle.edits.rows.length === 0 ||
        summary.includes(`${bundle.edits.rows.length} planted defect${bundle.edits.rows.length === 1 ? '' : 's'}: ${count(bundle, 'claim')} noticed by a claim, ${count(bundle, 'structure')} by structure only, ${count(bundle, 'unnoticed')} unnoticed`));
      const dots = withClass(page.get('panel'), 'dot').filter((node) => node.parentNode.tagName === 'TD').length;
      check(`${bundle.file}: one mark for every claim ZAL names on an edit`, dots === (bundle.edits.breakdown ? bundle.edits.rows.reduce((sum, row) => sum + row.noticed_by.length, 0) : 0));
    }
    if (bundle.decisions.status === 'complete') {
      await tab('try');
      const shown = () => said(page), current = () => page.run('keyOf(B(), { state: S.sim.state, variables: S.sim.variables, event: S.sim.event, context: S.sim.context })');
      const sentence = (key) => bundle.decisions.kept.sentence[bundle.decisions.rows[bundle.decisions.index[key]][5]];
      const truths = withClass(page.get('panel'), 'truth'), requirements = bundle.declared.filter((entry) => entry.concept === 'require');
      check(`${bundle.file}: the Try tab writes each requirement exactly as the statement does (${requirements.length})`, requirements.every((entry) => {
        const shown = truths.find((node) => node.attributes['data-clause'] === 'require:' + entry.id); return shown && withClass(shown, 'fm')[0].textContent === plainOf(entry.formal); }));
      let agreed = shown() === sentence(current()), pressed = 0;
      for (let at = 0; at < withClass(page.get('panel'), 'opt').length; at += 1) {
        await press(withClass(page.get('panel'), 'opt')[at]); pressed += 1;
        agreed = agreed && shown() === sentence(current());
      }
      check(`${bundle.file}: every option pressed shows ZAL's sentence for exactly that input (${pressed} pressed)`, agreed && pressed >= 3);
    }
  }

  // ---- what a pass covers, said first
  await pick('order.zal');
  check('a pass without claims says so in the first line', page.text('verdict').includes('No claim about the application is stated, so nothing about it was checked.'));
  check('ZAL advisory is raised to the verdict by its own kind', withClass(page.get('verdict'), 'alert').some((node) => node.textContent === 'Advisory: no-application-invariants'));
  check('a statement without claims is shown as making none', page.text('statement').includes('Claims (0)') && page.text('statement').includes('This statement makes no claim.'));
  check('verdict tone warns when a pass says little', page.get('verdict').attributes['data-s'] === 'warn');
  await pick('limit.zal');
  check('a pass with claims uses ZAL status and the plain gloss', page.text('verdict').includes("Every stated claim passed ZAL's finite check.") && page.get('verdict').attributes['data-s'] === 'pass');
  await pick('starved.zal');
  check('an assumption the claimant can starve is raised to the verdict', withClass(page.get('verdict'), 'alert').some((node) => node.textContent === 'customer can win by starving the assumption merchant_returns'));
  await pick('refund.zal'); await tab('evidence');
  check('a pass that lost its assumption is named as passing for the wrong reason', page.text('verdict').includes('1 passing for the wrong reason') && withClass(page.get('panel'), 'row').some((row) => row.textContent.includes('Delete the rule') && row.textContent.includes('passes for the wrong reason')));
  await pick('unresolved.zal');
  check('unsupported keeps ZAL status and names what is not checked', page.text('verdict').includes('1 obligation is stated but not checked.') && page.text('statement').includes('send a confirmation email when the order is confirmed'));

  // ---- a failing statement: ZAL's own story, no planted edits
  await pick('overflows.zal'); await tab('evidence');
  const failing = fixture.bundles[named('overflows.zal')];
  check('counterexample leads with what failed', page.text('verdict').includes('A check failed: range.') && page.get('verdict').attributes['data-s'] === 'fail');
  check("counterexample is told in ZAL's sentences, in order", JSON.stringify(tagged(withClass(page.get('panel'), 'story')[0], 'li').map((node) => node.textContent)) === JSON.stringify(failing.story.english));
  check('no defect is planted in a statement that does not pass', page.text('panel').includes('No defects were planted, because the statement itself does not pass.'));

  // ---- the statement as a document
  await pick('limit.zal');
  const limit = fixture.bundles[named('limit.zal')], statement = () => page.get('statement'), line = (key) => lineOf(statement(), key);
  check('a rule no claim holds is marked free, a held rule is marked held', line('rule:lock_it').attributes['data-cov'] === 'free' && line('rule:new_day_open').attributes['data-cov'] === 'free' && line('rule:take').attributes['data-cov'] === 'held');
  check('a claim shows how many planted defects it notices', line('require:counts_each_one').textContent.includes('notices 1 of 8') && line('require:counts_each_one').attributes['data-cov'] === 'teeth');
  await press(headOf(line('rule:lock_it')));
  check('an opened rule says what no claim would notice', line('rule:lock_it').textContent.includes('No claim notices if this rule is deleted.') && headOf(line('rule:lock_it')).attributes['aria-expanded'] === 'true' && classed(line('rule:lock_it'), 'open'));
  check('opening a line shows the readings that are switched off', line('rule:lock_it').textContent.includes('rule lock_it: unlocked + lock [true] -> accept locked;') && !line('rule:take').textContent.includes('rule take:'));
  check('no button sits inside another button', tagged(page.document.body, 'button').every((node) => { for (let up = node.parentNode; up; up = up.parentNode) if (up.tagName === 'BUTTON') return false; return true; }));
  await press(button(line('rule:lock_it'), 'Show a step where it fires'));
  check('a rule can be opened on a step where it fires', page.run('S.tab') === 'try' && said(page).includes('rule lock_it accepts and moves to locked'));

  // ---- three readings of one typed model
  const reading = (label) => button(statement(), label), take = limit.declared.find((entry) => entry.id === 'take'), visible = limit.declared.filter((entry) => !['header', 'name'].includes(entry.concept));
  check('a first visit reads English and symbols, and leaves the typed lines out', reading('English').attributes['aria-pressed'] === 'true' && reading('Symbols').attributes['aria-pressed'] === 'true' && reading('Typed').attributes['aria-pressed'] === 'false' && !line('rule:take').textContent.includes('rule take: unlocked + withdraw [used < 3]'));
  check('Symbols are formal symbols, not the typed spelling', line('rule:take').textContent.includes('state = unlocked ∧ event = withdraw ∧ used < 3 ⇒ accepted ∧ state′ = unlocked ∧ used′ = used+1') && plainOf(take.formal) === 'state = unlocked ∧ event = withdraw ∧ used < 3 ⇒ accepted ∧ state′ = unlocked ∧ used′ = used+1');
  const lock = limit.declared.find((entry) => entry.id === 'lock_it');
  check('what a rule leaves unchanged is folded until the line is read', textOf(withClass(line('rule:lock_it'), 'fm-more')[0]) === lock.more.map((segment) => segment[0]).join('') && lock.more.map((segment) => segment[0]).join('') === ' ∧ used′ = used' && withClass(line('rule:lock_it'), 'fm-fold')[0].textContent === '∧ 1 unchanged' && withClass(line('rule:take'), 'fm-fold').length === 0);
  check('a reason is a note beside the formula, not part of it', withClass(statement(), 'fm-note').some((node) => node.textContent === 'reason not_enabled') && !plainOf(limit.declared.find((entry) => entry.concept === 'default').formal).includes('reason'));
  check('a long line breaks before its main connective', withClass(line('rule:take'), 'fm-l').length === 2 && withClass(line('rule:take'), 'fm-l')[1].textContent.startsWith('⇒') && withClass(line('require:reset_clears'), 'fm-l').length === 1);
  await press(reading('Typed'));
  check('Typed adds each line as ZAL prints it', visible.every((entry) => page.text('statement').includes(entry.symbolic)) && page.kept['zal-review-desk-readings'] === 'english symbols typed');
  await press(reading('English')); await press(reading('Typed'));
  check('Symbols alone leaves out the English and the typed lines', page.text('statement').includes(plainOf(take.formal)) && !page.text('statement').includes('when the state is unlocked and the event is withdraw') && !page.text('statement').includes('rule take: unlocked') && page.kept['zal-review-desk-readings'] === 'symbols');
  check('a line without a formula form is shown as ZAL prints it', page.text('statement').includes('step atomic; frame control_and_variables;'));
  await press(reading('Symbols'));
  check('the last reading cannot be switched off', reading('Symbols').attributes['aria-pressed'] === 'true' && page.text('statement').includes(plainOf(take.formal)));

  // ---- the reading key follows the line being read
  const dock = () => page.get('key'), rowsShown = () => withClass(dock(), 'krow').map((row) => row.attributes['data-k']), point = (node) => fire(statement(), 'mouseover', { target: node });
  const first = load(fixture.snapshot).get('key');
  check('the key starts with every symbol the statement uses', first.textContent.includes('the whole statement') && Object.keys(limit.key.entries).every((key) => keyed(first, key).length === 1) && withClass(first, 'krow').length === 0);
  check('opening a line turned the key to it', dock().textContent.includes('rule lock_it'));
  const counts = limit.declared.find((entry) => entry.id === 'counts_each_one');
  await point(headOf(line('require:counts_each_one')));
  check('pointing at a line turns the key to that line: its kind in ZAL words, then only its symbols and its names, in reading order', dock().textContent.includes('requirement counts_each_one') && dock().textContent.includes(limit.key.kinds.require) &&
    JSON.stringify(rowsShown()) === JSON.stringify([...keysOf(counts).filter((key) => !key.startsWith('name:')), ...keysOf(counts).filter((key) => key.startsWith('name:'))]) && classed(line('require:counts_each_one'), 'at') && !classed(line('rule:take'), 'at'));
  check('each row gives ZAL reading and meaning of its symbol, and a name gives its declaration', withClass(dock(), 'krow').every((row) => { const key = row.attributes['data-k'];
    return key.startsWith('name:') ? row.textContent.includes(limit.key.names[key.slice(5)].english) : row.textContent.includes(limit.key.entries[key].meaning) && row.textContent.includes(limit.key.entries[key].read) && (limit.key.entries[key].also || []).every((text) => row.textContent.includes('or ' + text)); }));
  check('a symbol with more than one reading gives the formal one first and then the others, with its name', (() => { const row = keyed(dock(), 'implies').find((node) => classed(node, 'krow')), text = row.textContent;
    return limit.key.entries.implies.read === 'implies' && text.indexOf('implies') >= 0 && text.indexOf('implies') < text.indexOf('or if A, then B') && text.includes('Implication. ') && dock().textContent.includes("A symbol's first reading is the formal one"); })());
  check('pointing at a symbol in a line names every reading of it', keyed(line('require:counts_each_one'), 'implies')[0].attributes.title.startsWith('implies; or if A, then B. ') && keyed(line('require:counts_each_one'), 'prime')[0].attributes.title.startsWith('prime; or after this event. '));
  check('a row names the typed spelling of its symbol', keyed(dock(), 'implies')[0].textContent.includes('Typed =>') && keyed(dock(), 'prime')[0].textContent.includes("Typed '"));
  check('with the English switched off the key offers it as a check, and does not show it unasked', !dock().textContent.includes(counts.english) && !!button(dock(), "Check your reading against ZAL's English"));
  await press(button(dock(), "Check your reading against ZAL's English"));
  check('the English reading is ZAL own sentence for that line', dock().textContent.includes(counts.english));
  await fire(statement(), 'focusin', { target: headOf(line('rule:take')) });
  check('the keyboard moves the key as the pointer does, and a rule is explained as a step formula', dock().textContent.includes('rule take') && dock().textContent.includes(limit.key.kinds.rule) && dock().textContent.includes(limit.key.wording.rule) && classed(line('rule:take'), 'at') && !classed(line('require:counts_each_one'), 'at'));
  await point(keyed(line('rule:take'), 'implies')[0]);
  check('a symbol under the pointer is lit in the line and in the key', classed(keyed(line('rule:take'), 'implies')[0], 'hot') && keyed(dock(), 'implies').some((node) => classed(node, 'hot')) && !keyed(dock(), 'and').some((node) => classed(node, 'hot')));
  check('the same symbol in other lines stays unlit', keyed(page.get('doc'), 'implies').filter((node) => classed(node, 'hot')).length === 1 && keyed(page.get('doc'), 'implies').length > 5);
  await point(keyed(dock(), 'prime')[0]);
  check('a row under the pointer lights its symbol in the statement, and the key stays on its line', keyed(line('rule:take'), 'prime').every((node) => classed(node, 'hot')) && !classed(keyed(line('rule:take'), 'implies')[0], 'hot') && page.run('K.at') === 'rule:take');
  await fire(statement(), 'mouseleave');
  check('nothing stays lit when the pointer leaves', keyed(statement(), 'prime').every((node) => !classed(node, 'hot')));
  await press(button(keyed(dock(), 'implies')[0], 'I know this'));
  check('a learned symbol shrinks to a word, is kept, and is never removed', page.kept['zal-review-desk-learned'] === '["implies"]' && !rowsShown().includes('implies') && keyed(dock(), 'implies').some((node) => classed(node, 'kchip') && classed(node, 'known')) && dock().textContent.includes('Learned:') && page.run('K.at') === 'rule:take');
  await press(keyed(dock(), 'implies')[0] || dock());
  check('a learned symbol opens to its meaning again when pressed', rowsShown().includes('implies') && keyed(dock(), 'implies')[0].textContent.includes(limit.key.entries.implies.meaning) && button(keyed(dock(), 'implies')[0], 'Learned').attributes['aria-pressed'] === 'true');
  await press(button(dock(), 'Whole statement'));
  check('the whole key lists every symbol as a word, learned or not', dock().textContent.includes('the whole statement') && withClass(dock(), 'kchip').length === Object.keys(limit.key.entries).length && withClass(dock(), 'kchip').filter((node) => classed(node, 'known')).length === 1);
  await press(keyed(dock(), 'unchanged')[0]);
  check("a row the desk words itself says so", rowsShown().join() === 'unchanged' && textOf(withClass(dock(), 'krow')[0]).includes("The desk's wording.") && !keyed(dock(), 'unchanged').some((node) => classed(node, 'krow') && node.textContent.includes('Typed')));
  await press(button(dock(), 'Show every meaning again'));
  check('learning can be undone for every symbol at once', page.kept['zal-review-desk-learned'] === '[]' && withClass(dock(), 'known').length === 0);
  await press(button(dock(), 'Hide'));
  check('the key can be put away and brought back', withClass(dock(), 'key-b').length === 0 && page.kept['zal-review-desk-key'] === 'hidden' && !!button(dock(), 'Show'));
  await press(button(dock(), 'Show'));
  await press(reading('English'));
  check('with the English shown the key does not repeat it', withClass(dock(), 'key-b').length === 1 && !button(dock(), "Check your reading against ZAL's English"));
  const returning = load(fixture.snapshot, { seed: { 'zal-review-desk-readings': 'symbols typed', 'zal-review-desk-learned': '["and","eq"]', 'zal-review-desk-key': 'hidden', 'zal-review-desk-model': 'limit.zal' } });
  check('a returning reader finds the readings, the learned symbols and the key as they left them', returning.run('JSON.stringify([S.read, [...K.known], K.hidden])') === JSON.stringify([{ english: false, symbols: true, typed: true }, ['and', 'eq'], true]) && withClass(returning.get('key'), 'key-b').length === 0);
  check('kept learning that cannot be read is ignored', (() => { try { return load(fixture.snapshot, { seed: { 'zal-review-desk-learned': '{', 'zal-review-desk-readings': 'none' } }).run('JSON.stringify([S.read, K.known.size])') === JSON.stringify([{ english: true, symbols: true, typed: false }, 0]); } catch (error) { return false; } })());

  // ---- the statement answers the instrument in use
  await tab('try'); page.run('S.sim = startInput(B()); S.sim.event = "withdraw"; render()');
  check('while an input is tried, each line says what it does on that input', textOf(withClass(line('rule:take'), 'now')[0]) === 'accepts this input' && line('rule:take').attributes['data-now'] === 'yes' && textOf(withClass(line('require:counts_each_one'), 'now')[0]) === 'holds on this step' && withClass(line('rule:lock_it'), 'now').length === 0 && page.text('statement').includes('While you try an input, each line says what it does on that input.'));
  const guard = withClass(page.get('panel'), 'truth').find((node) => node.attributes['data-clause'] === 'rule:take'), claim = withClass(page.get('panel'), 'truth').find((node) => node.attributes['data-clause'] === 'require:counts_each_one');
  check('a guard is written in symbols with the truth of each part on this input', !!guard && textOf(withClass(guard, 'fm')[0]) === 'used < 3' && withClass(guard, 'tv')[0].attributes['data-v'] === 'true' && textOf(withClass(guard, 'where')[0]) === 'where used = 0');
  check('a requirement shows which parts are true, and what it read', !!claim && withClass(claim, 'tv').every((node) => node.attributes['data-v'] === 'true') && keyed(claim, 'implies').some((node) => classed(node, 't')) && textOf(withClass(claim, 'where')[0]) === 'where accepted = true, event = withdraw, used′ = 1, used = 0');
  page.run('S.sim.event = "lock"; render()');
  const idle = withClass(page.get('panel'), 'truth').find((node) => node.attributes['data-clause'] === 'require:counts_each_one');
  check('a false premise is shown as false, and the implication it makes true as true', !!idle && withClass(idle, 'tv').some((node) => node.attributes['data-v'] === 'false') && keyed(idle, 'implies').every((node) => classed(node, 't')) && keyed(idle, 'and').every((node) => classed(node, 'f')));
  await fire(page.get('panel'), 'mouseover', { target: withClass(idle, 'fm')[0] });
  check('pointing at a formula on the Try tab turns the key and the statement to its line', page.run('K.at') === 'require:counts_each_one' && dock().textContent.includes('requirement counts_each_one') && classed(line('require:counts_each_one'), 'at'));
  await press(button(page.get('panel'), 'Show it in the statement'));
  check('the rule that decided can be shown in the statement', page.run('K.at') === 'rule:lock_it' && classed(line('rule:lock_it'), 'at') && textOf(withClass(line('rule:lock_it'), 'now')[0]) === 'accepts this input');
  page.run('S.sim.state = "locked"; S.sim.event = "withdraw"; render()');
  check('the default line says when it decides', textOf(withClass(lineOf(statement(), 'default:' + limit.declared.findIndex((entry) => entry.concept === 'default')), 'now')[0]) === 'rejects this input');
  await tab('evidence');
  check('away from the Try tab the lines stop reporting on an input', withClass(statement(), 'now').length === 0);
  check('the page says where the symbols come from and what ZAL checked of them', (await tab('evidence'), page.text('panel')).includes("The symbols are the desk's spelling.") && page.text('panel').includes(`Each of ${limit.cross_checks.formal.rules} rules, written as a step formula, was evaluated by ZAL on ${limit.cross_checks.formal.inputs} of the ${limit.cross_checks.formal.of} declared inputs`));

  // ---- evidence: which claim ZAL names, and what it was not asked
  await pick('ledger.zal'); await tab('evidence');
  const ledgerRow = (group, label) => { const rows = tagged(withClass(page.get('panel'), 'matrix')[0], 'tr'); const start = rows.findIndex((row) => row.textContent === 'rule ' + group); return rows.slice(start + 1).find((row) => row.children[0].textContent === label); };
  const invariantEdit = ledgerRow('close_ledger', 'Remove its updates'), requirementEdit = ledgerRow('post_entry', 'Remove its updates');
  check('an edit that breaks an invariant marks the invariants and shows the other claim as not asked', withClass(invariantEdit.children[2], 'dot').length === 1 && invariantEdit.children[3].textContent === '–' && page.text('panel').includes('not asked: an invariant fails first'));
  check('an edit that breaks a requirement marks that requirement only', withClass(requirementEdit.children[2], 'dot').length === 0 && withClass(requirementEdit.children[3], 'dot').length === 1);
  check('a claim is not counted against edits it was never asked about', lineOf(page.get('statement'), 'require:posting_counts').textContent.includes('notices 1 of 3'));
  await press(invariantEdit);
  check('a matrix row opens the edit under Review a change', page.run('S.tab') === 'changes' && page.text('panel').includes('Remove the updates of rule close_ledger') && page.text('panel').includes('The invariants fail. ZAL stops there, so no other claim is asked about this edit.'));
  check('the failed claim is quoted in ZAL English', withClass(page.get('panel'), 'quote').some((node) => node.textContent === fixture.bundles[named('ledger.zal')].declared.find((entry) => entry.id === 'closed_has_nothing_pending').english.split(': ').slice(1).join(': ')));

  // ---- review a change: ZAL's before and after sentences
  await pick('limit.zal'); await tab('changes');
  const unnoticed = limit.edits.rows.find((row) => row.label === 'Delete rule lock_it');
  await press(withClass(page.get('panel'), 'chg').find((node) => node.textContent.includes('Delete rule lock_it')));
  const cards = withClass(page.get('panel'), 'wit');
  check('a planted edit shows the removed line and ZAL status', withClass(page.get('panel'), 'del')[0].textContent === unnoticed.before && page.text('panel').includes('Every check still passes. No claim notices this edit.'));
  check('each distinguishing input carries ZAL sentence before and after', cards.length === unnoticed.diff.witnesses.length && unnoticed.diff.witnesses.every((witness, at) => cards[at].textContent.includes(witness.sentences.before) && cards[at].textContent.includes(witness.sentences.after)));
  check('the number of inputs decided differently is ZAL count', page.text('panel').includes(`${unnoticed.diff.witness_count} inputs of 32 are decided differently.`));
  await press(button(cards[0], 'Open this input in the unedited machine'));
  check('a distinguishing input opens in the unedited machine', page.run('S.tab') === 'try' && said(page) === unnoticed.diff.witnesses[0].sentences.before);
  check('a snapshot offers no candidate box', tagged(page.get('panel'), 'textarea').length === 0);

  // ---- try an input: steps, nearest opposite decision, focus
  await press(button(page.get('panel'), 'Back to the start') || withClass(page.get('panel'), 'opt')[0]);
  page.run('S.sim = startInput(B()); render()');
  const option = (text) => withClass(page.get('panel'), 'opt').find((node) => node.textContent === text);
  await press(option('withdraw'));
  for (let step = 1; step <= 3; step += 1) await press(button(page.get('panel'), 'Take this step'));
  check('taking steps follows ZAL outcomes and keeps the run so far', page.run('S.sim.variables.used') === 3 && tagged(withClass(page.get('panel'), 'trace')[0], 'li').length === 3 && said(page).includes('no rule is enabled, so the default rejects'));
  check('a rejected input offers no step', all(page.get('panel'), (node) => node.textContent === 'Take this step').length === 0);
  const nearest = limit.decisions.kept.nearest[limit.decisions.rows[limit.decisions.index['unlocked|3|withdraw']][6]];
  check("the nearest opposite decision is ZAL's why-not answer", page.text('panel').includes('What would it take to be accepted?') && page.text('panel').includes(`needs ${nearest.distance} change`) && page.text('panel').includes(`${nearest.count} inputs are that near`));
  await press(button(page.get('panel'), 'Go there'));
  check('Go there moves to the input ZAL named', page.run('JSON.stringify([S.sim.state, S.sim.variables, S.sim.event])') === JSON.stringify([nearest.first.input.state, nearest.first.input.variables, nearest.first.input.event]));
  await press(option('locked'));
  check('an unreached configuration is said to be unreachable but still covered', page.text('panel').includes('This configuration is not reachable from the start.') === !limit.reachable.includes(page.run('configKey(B(), S.sim.state, S.sim.variables)')));
  const focused = option('reset'); focused.focus(); await press(focused);
  check('the pressed option keeps the keyboard after the redraw', page.document.activeElement !== focused && page.document.activeElement.textContent === 'reset' && page.document.activeElement.attributes['aria-pressed'] === 'true');
  await pick('ledger.zal'); await tab('try'); page.run('S.sim.state = "closed"; S.sim.variables.pending = 2; render()');
  check('a configuration that breaks an invariant is said not to have its requirements checked', page.text('panel').includes('This configuration breaks an invariant, so the requirements are not checked from it.'));
  check('each invariant is shown as ZAL evaluates it on the step', withClass(page.get('panel'), 'req').some((row) => row.textContent.startsWith('broken here') && row.textContent.includes('closed_has_nothing_pending')));
  page.run('S.sim.state = "open"; S.sim.variables.pending = 1; S.sim.event = "close"; render()');
  check('an invariant that holds before and after the step is shown as holding', withClass(page.get('panel'), 'req').some((row) => row.textContent.startsWith('holds') && row.textContent.includes('closed_has_nothing_pending')));
  await pick('overflows.zal'); await tab('try'); page.run('S.sim.state = "unlocked"; S.sim.variables.used = 3; S.sim.event = "withdraw"; render()');
  check('an update that leaves its range is shown as the trap ZAL reports, with no step to take', withClass(page.get('panel'), 'oc')[0].textContent === 'Trap' && said(page).includes('would set used to 4, outside 0..=3, so the step is refused') && !button(page.get('panel'), 'Take this step'));
  check('a long integer range is a bounded number field', (() => { page.run('globalThis.picked = null; globalThis.field = integer("n", 0, 20, 5, (value) => { picked = value; })'); const box = tagged(page.run('field'), 'input')[0]; box.listeners.change[0]({ target: { value: '33' } }); const high = page.run('picked'); box.listeners.change[0]({ target: { value: 'x' } }); return high === 20 && page.run('picked') === 5 && box.attributes.max === '20'; })());

  // ---- narrow layout, blocked storage, deep link
  let narrow = null;
  try { narrow = load(fixture.snapshot, { wide: false, storage: 'throws', hash: '#statement' }); } catch (error) { narrow = null; }
  check('the page starts when the browser blocks storage', narrow !== null);
  check('narrow layout puts the statement in its own tab', narrow.run('S.tab') === 'statement' && narrow.text('panel').includes('The statement') && narrow.text('panel').includes(plainOf(take.formal)));
  check('a narrow screen has no docked key, and keeps the whole key at the foot of the statement', narrow.get('key') === null && narrow.text('panel').includes('open a line to see how to read it') && Object.keys(limit.key.entries).every((key) => keyed(narrow.get('panel'), key).some((node) => classed(node, 'kchip'))));
  await press(headOf(lineOf(narrow.get('panel'), 'rule:take')));
  check('on a narrow screen an opened line carries its own key', (() => { const opened = lineOf(narrow.get('panel'), 'rule:take'); return opened.textContent.includes('How to read it') && opened.textContent.includes(limit.key.kinds.rule) &&
    JSON.stringify(withClass(opened, 'krow').map((row) => row.attributes['data-k'])) === JSON.stringify([...keysOf(take).filter((key) => !key.startsWith('name:')), ...keysOf(take).filter((key) => key.startsWith('name:'))]); })());
  await press(button(lineOf(narrow.get('panel'), 'rule:take'), 'I know this'));
  check('learning works without storage and without a dock', narrow.run('K.known.size') === 1 && withClass(lineOf(narrow.get('panel'), 'rule:take'), 'known').length === 1);
  await press(narrow.get('tab-evidence'));
  check('a tab press is kept in the address', narrow.location.hash === '#evidence' && narrow.text('panel').includes('Could it have failed?'));
  check('a link to a tab opens that tab', load(fixture.snapshot, { hash: '#try' }).run('S.tab') === 'try');
  const remembered = load(fixture.snapshot); await fire(remembered.get('model'), 'change', { target: { value: String(named('ledger.zal')) } });
  check('the chosen statement is remembered by its file name only', remembered.kept['zal-review-desk-model'] === 'ledger.zal' && Object.keys(remembered.kept).every((key) => key.startsWith('zal-review-desk-')));
  await press(remembered.get('theme'));
  check('the theme choice is the reader own and is kept', remembered.document.documentElement.attributes['data-theme'] === 'dark' && remembered.kept['zal-review-desk-theme'] === 'dark');

  // ---- live, table in the page: candidates go to ZAL through the page's own token
  const candidateOf = (source) => source.includes('take now') ? fixture.candidates.refused : source.includes('[true] -> accept unlocked, used += 1') ? fixture.candidates.overflows : source.includes('rule lock_it') ? fixture.candidates.same : fixture.candidates.freer;
  let changed = false;
  const live = load(fixture.live, { answer: (call) => call.url === '/api/changed' ? { body: { changed } } : call.url === '/api/candidate' ? { body: candidateOf(call.body.source) } : { status: 400, body: { error: 'Unknown review desk action' } } });
  check('live page says which file it reads and starts one poll', live.text('top').includes('Live on this machine, reading limit.zal') && live.timers.length === 1 && live.text('foot').includes('live on 127.0.0.1'));
  await press(live.get('tab-changes'));
  const area = () => tagged(live.get('panel'), 'textarea')[0], send = async (edit) => { const box = area(); box.value = edit(box.value); await fire(box, 'input'); await press(button(live.get('panel'), 'Check this candidate')); await settle(); };
  check('candidate box starts from the current statement', area().value === limit.symbolic);
  await send((text) => text);
  const sent = live.calls[live.calls.length - 1];
  check('a candidate is sent with the page token, the shown revision and nothing else', sent.url === '/api/candidate' && sent.method === 'POST' && sent.headers['X-ZAL-Review-Desk'] === 'page-token' && JSON.stringify(Object.keys(sent.body).sort()) === '["revision","source"]' && sent.body.revision === limit.revision);
  check('an unchanged candidate is said to decide every input as before', live.text('panel').includes('Every check passes on your candidate, and it decides every declared input as before.'));
  await send((text) => text.replace('[used < 3]', '[true]'));
  check('a candidate that breaks the structure names ZAL check and tells its story', live.text('panel').includes("Your candidate breaks the machine's structure (update_ranges) before any claim is consulted.") && fixture.candidates.overflows.story.english.every((line) => live.text('panel').includes(line)));
  check('the draft is kept across the redraw', area().value.includes('[true] -> accept unlocked, used += 1'));
  await send((text) => text.replace('rule take:', 'rule take now:'));
  check('a refused candidate shows ZAL located diagnostic and suggestion', live.text('panel').includes('ZAL does not accept your candidate.') && live.text('panel').includes(`line ${fixture.candidates.refused.source_diagnostic.location.line}`) && live.text('panel').includes(fixture.candidates.refused.source_diagnostic.reason) && live.text('panel').includes('ZAL suggests: ' + fixture.candidates.refused.source_diagnostic.suggested_fix));
  await press(button(live.get('panel'), 'Reset to the current statement'));
  await send((text) => text.replace('rule lock_it: unlocked + lock [true] -> accept locked;\n', ''));
  check('a passing candidate is judged by the inputs it decides differently', live.text('panel').includes('Every check passes on your candidate.') && live.text('panel').includes('4 inputs of 32 are decided differently.') && live.text('panel').includes('Open this input in the current machine'));
  check('a live page offers no way to accept, agree or export', tagged(live.document.body, 'button').filter((node) => /\b(btn|link)\b/.test(node.attributes.class || '')).every((node) => !/accept|agree|export|approve/i.test(node.textContent)) && live.text('panel').includes('Nothing is saved and nothing is agreed.') && live.text('panel').includes('This page shows consequences and records no agreement.'));
  await live.timers[0](); await settle();
  check('an unchanged file raises nothing', withClass(live.get('verdict'), 'banner').length === 0 && live.calls[live.calls.length - 1].headers['X-ZAL-Review-Desk'] === 'page-token');
  changed = true; await live.timers[0](); await settle();
  check('a changed file is announced and the old revision is named as old', textOf(withClass(live.get('verdict'), 'banner')[0]).startsWith('The file changed on disk. This page still shows the earlier revision.') && live.location.reloads === 0);
  await press(tagged(live.get('verdict'), 'button').find((node) => node.textContent === 'Reload'));
  check('Reload asks the browser for the page again', live.location.reloads === 1);

  for (const kind of ['different_profile', 'different_domain', 'unsupported']) {
    const told = fixture.candidates[kind];
    const different = load(fixture.live, { hash: '#changes', answer: () => ({ body: told }) });
    await different.run('checkCandidate()');
    check(kind + ': no equivalence or full-pass claim without that evidence', !different.text('panel').includes('Every check passes on your candidate, and it decides every declared input as before.') &&
      (kind === 'unsupported' ? different.text('panel').includes('unsupported obligations; this is not a full pass.') && told.advisories.every((a) => different.text('panel').includes(a.message))
        : different.text('panel').includes('equivalence to the current statement was not established.')));
  }

  // A result belongs to the exact submitted source, never the text typed or reset while it was pending.
  const waiting = [];
  const racing = load(fixture.live, { hash: '#changes', answer: (call) => new Promise((resolve) => waiting.push({ call, resolve })) });
  const candidateBox = () => tagged(racing.get('panel'), 'textarea')[0];
  const original = candidateBox().value;
  const firstRequest = racing.run('checkCandidate()');
  candidateBox().value = original.replace('[used < 3]', '[true]'); await fire(candidateBox(), 'input');
  waiting[0].resolve({ body: fixture.candidates.same }); await firstRequest;
  check('a verdict for an earlier draft is discarded after typing', !racing.run('S.candidate') && !racing.run('S.busy') && !racing.text('panel').includes('Every check passes on your candidate'));
  const secondRequest = racing.run('checkCandidate()');
  await press(button(racing.get('panel'), 'Reset to the current statement'));
  const thirdRequest = racing.run('checkCandidate()');
  waiting[1].resolve({ body: fixture.candidates.overflows }); await secondRequest;
  check('a reset invalidates the old answer without clearing a newer pending request', !racing.run('S.candidate') && racing.run('S.busy'));
  waiting[2].resolve({ body: fixture.candidates.same }); await thirdRequest;
  check('the current candidate result is still shown after superseding a request', racing.text('panel').includes('Every check passes on your candidate, and it decides every declared input as before.'));
  const sameBox = candidateBox(); sameBox.value = original.replace('[used < 3]', '[true]'); await fire(sameBox, 'input');
  check('editing a checked candidate removes its old verdict without replacing the editor', !racing.run('S.candidate') && !racing.text('panel').includes('Every check passes on your candidate') && candidateBox() === sameBox);
  const fourthRequest = racing.run('checkCandidate()');
  await press(button(racing.get('panel'), 'Reset to the current statement'));
  waiting[3].resolve({ status: 400, body: { error: 'old request failed' } }); await fourthRequest;
  check('a reset also discards a late error', !racing.run('S.candidate') && !racing.run('S.busy') && !racing.text('panel').includes('old request failed'));

  // ---- live, no table in the page: every decision is asked of ZAL
  const pending = [];
  const asking = load(fixture.live_asked, { hash: '#try', answer: (call) => call.url === '/api/changed' ? { body: { changed: false } } : new Promise((resolve) => pending.push({ call, resolve })) });
  const answerFor = (call) => fixture.answers[asking.run(`keyOf(B(), ${JSON.stringify(call.body.input)})`)];
  check('a statement without a table asks ZAL for the first input and says it is asking', pending.length === 1 && pending[0].call.url === '/api/explain' && pending[0].call.body.revision === fixture.asked.revision && asking.text('panel').includes('Asking ZAL'));
  await press(withClass(asking.get('panel'), 'opt').find((node) => node.textContent === 'withdraw'));
  check('a second input is asked while the first is still out', pending.length === 2 && pending[1].call.body.input.event === 'withdraw');
  pending[1].resolve({ body: answerFor(pending[1].call) }); await settle();
  const second = answerFor(pending[1].call);
  check("ZAL's answer for the current input is shown with its sentence and nearest opposite decision", said(asking) === second.sentence && asking.text('panel').includes('What would make it rejected?'));
  pending[0].resolve({ body: answerFor(pending[0].call) }); await settle();
  check('a late answer for an earlier input does not replace the current one', said(asking) === second.sentence);
  await press(button(asking.get('panel'), 'Take this step'));
  check('a step in a live page asks ZAL about the next input', pending.length === 3 && pending[2].call.body.input.variables.used === 1);
  pending[2].resolve({ status: 400, body: { error: 'This page shows an older revision; reload' } }); await settle();
  check('a refusal from the server is shown as ZAL not answering', asking.text('panel').includes('ZAL did not answer for this input: This page shows an older revision; reload'));
  await press(asking.get('tab-evidence'));
  check('a page that holds no table does not say a table was compared', asking.text('panel').includes('Where this came from') && !asking.text('panel').includes('The decision table'));
  check("a table that disagrees with ZAL's count is said to disagree", (() => { const odd = load(fixture.snapshot, { hash: '#evidence' }), agreed = odd.text('panel').includes('The decision table has exactly the number of input tuples ZAL reports.');
    odd.run('DESK.bundles[S.i].cross_checks.tuples = false; render()'); return agreed && odd.text('panel').includes("The decision table does not have the number of input tuples ZAL reports; trust ZAL's count."); })());

  // ---- live, a file ZAL refuses or that raises an error
  let moved = false;
  const refused = load(fixture.live_refused, { answer: () => ({ body: { changed: moved } }) });
  check('a refused file shows ZAL located diagnostic and suggestion, not a dead page', ['ZAL does not accept limit.zal.', `line ${fixture.located.location.line}`, fixture.located.source_line, fixture.located.reason, 'ZAL suggests: ' + fixture.located.suggested_fix, 'This page reloads by itself.'].every((text) => refused.document.body.textContent.includes(text)));
  await refused.timers[0](); await settle();
  check('a refused file is not reloaded until it changes', refused.location.reloads === 0);
  moved = true; await refused.timers[0](); await settle();
  check('a corrected file reloads the page by itself', refused.location.reloads === 1);
  check('an error is not presented as a refusal by ZAL', (() => { const failed = load(fixture.live_error).document.body.textContent; return failed.includes('raised an error. This is not a refusal by ZAL, and the source may be fine.') && failed.includes('TypeError: planted') && !failed.includes('ZAL does not accept'); })());

  finished = true;
  console.log(`${checks.length} review desk checks passed (DOM double; no browser, network or provider)`);
})().catch((error) => { console.error(error); process.exit(1); });
