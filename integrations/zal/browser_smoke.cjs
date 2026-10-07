/* Optional real-browser check: requires an already installed Playwright/Chromium.
   Start a fresh companion, then pass its loopback URL and an evidence directory.
   No network bridge, injected fixture page or model call is used by default. */
'use strict';
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const url = process.argv[2] || 'http://127.0.0.1:8765/';
const output = process.argv[3] || '/tmp/zal-browser';
if (!/^http:\/\/127\.0\.0\.1:\d+\/$/.test(url)) throw Error('Use the fresh loopback companion URL');

(async () => {
  await fs.mkdir(output, { recursive: true });
  const browser = await chromium.launch({ headless: true,
    ...(process.env.CHROMIUM ? { executablePath: process.env.CHROMIUM } : {}) });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const errors = [], checks = [], expectedRefusals = [], pendingRefusals = new Map(), providerRequests = [];
  const observe = tab => {
    tab.on('request', request => { if (/\/api\/(connect|discuss)$/.test(request.url()) && !request.postDataJSON()?.demo) providerRequests.push(request.url()); });
    tab.on('pageerror', error => errors.push(error.message));
    tab.on('console', value => {
      if (value.type() !== 'error') return;
      const endpoint = value.location().url;
      if (pendingRefusals.has(endpoint) && value.text().includes('400')) {
        expectedRefusals.push(pendingRefusals.get(endpoint));
        pendingRefusals.delete(endpoint);
      } else errors.push(value.text());
    });
  };
  observe(page);
  const check = (name, condition) => { assert(condition, name); checks.push(name); };
  try {
    await page.goto(url);
    await page.waitForFunction(() => document.querySelector('#tupleCount').textContent === '24');
    check('actual browser HTTP path', (await page.locator('#englishRule').textContent()).includes('authorized'));
    const initial = await page.evaluate(() => view.model.revision);
    for (const id of ['event:approve', 'context:authorized']) {
      const button = page.locator('[data-object="' + id + '"]');
      await button.focus(); await page.keyboard.press('Enter');
      await page.waitForFunction(object => view.selected === object, id);
      check(id + ' is keyboard-selectable with synchronized text',
        await button.getAttribute('aria-current') === 'true' &&
        (await page.locator('#englishRule').textContent()).includes(id.split(':')[1]) &&
        (await page.locator('#symbolicRule').textContent()).includes(id.split(':')[1]));
    }
    await page.locator('#why').click();
    await page.waitForFunction(() => document.querySelector('#explanation').textContent.includes('typed AST evaluation'));
    check('primary Explain is deterministic with explicit assumptions',
      (await page.locator('#explanation').textContent()).includes('Default used: true') &&
      (await page.locator('#explanation').textContent()).includes('authorized') && providerRequests.length === 0);
    check('check advisory does not imply an unstated application invariant',
      (await page.locator('#checkAdvisories').textContent()).includes('No application invariants declared'));
    const helpTarget = page.locator('[data-object="rule:approve_order"]');
    await helpTarget.focus();
    await page.waitForFunction(() => document.querySelector('#helpPreview').textContent.includes('Meaning of rule:approve_order'));
    check('focus exposes endpoint-derived canonical object help', await page.locator('#legend').isVisible() === false &&
      (await page.locator('#helpPreview').textContent()).includes('not arbitrary editor text or source spans'));
    await page.keyboard.press('?');
    await page.waitForFunction(() => document.querySelector('#helpTitle').textContent === 'Meaning of rule:approve_order');
    check('keyboard context help retains typed identity', (await page.locator('#helpRevision').textContent()).includes('Topic: rule:approve_order'));
    await page.keyboard.press('Escape');
    check('context-help close restores its invoking focus', await helpTarget.evaluate(element => element === document.activeElement));
    await page.locator('#question').fill('Does this input work?'); await page.keyboard.press('?');
    check('typing question mark does not hijack editor input', !await page.locator('#legend').isVisible() &&
      (await page.locator('#question').inputValue()).endsWith('??'));
    await page.locator('#selectedHelp').click();
    await page.waitForFunction(() => document.querySelector('#helpRevision').textContent.includes('Topic: context:authorized'));
    check('visible help control works without hover', (await page.locator('#legendText').textContent()).includes('fresh Boolean observations'));
    await page.locator('#helpTopic').fill('and'); await page.locator('#helpForm button[type="submit"]').click();
    await page.waitForFunction(() => document.querySelector('#helpRevision').textContent.includes('Topic: &'));
    check('operator aliases resolve through canonical grammar metadata', (await page.locator('#legendText').textContent()).includes('every Boolean operand'));
    await page.locator('#closeLegend').click();
    // Hold a real help response while another object is selected at the same revision.
    let releaseHelp, reachedHelp, heldHelp = false;
    const pendingHelp = new Promise(resolve => { reachedHelp = resolve; });
    const helpGate = new Promise(resolve => { releaseHelp = resolve; });
    await page.route('**/api/help', async route => {
      if (!heldHelp && route.request().postDataJSON().topic === 'rule:approve_order') {
        heldHelp = true; reachedHelp(); await helpGate;
      }
      await route.continue();
    });
    await page.evaluate(() => { globalThis.pendingHelpUI = openHelp('rule:approve_order', document.querySelector('#selectedHelp'));  });
    await pendingHelp;
    await page.evaluate(() => choose('event:approve'));
    await page.waitForFunction(() => view.selected === 'event:approve');
    const oldHelpResponse = page.waitForResponse(response => response.url().endsWith('/api/help') && response.request().postDataJSON().topic === 'rule:approve_order');
    releaseHelp(); await oldHelpResponse; await page.evaluate(() => pendingHelpUI);
    await page.waitForFunction(() => document.querySelector('#legendText').textContent.includes('Selection or revision changed'));
    check('delayed help cannot replace changed selection', !(await page.locator('#helpTitle').textContent()).includes('approve_order'));
    await page.unroute('**/api/help'); await page.locator('#closeLegend').click();
    check('help and explanation do not start Codex', providerRequests.length === 0);
    const peer = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
    observe(peer);
    await peer.goto(url); await peer.waitForFunction(() => !!view);
    await peer.locator('[data-object="rule:approve_order"]').click();
    await peer.waitForFunction(() => view.selected === 'rule:approve_order');
    await peer.locator('#ruleEditor summary').click();
    await peer.locator('#editGuard').fill('!authorized');
    await peer.locator('#editButton').click();
    const dirtyDraft = (await peer.locator('#sourceEditor').inputValue()).replaceAll('not_enabled', 'local_draft');
    await peer.locator('#sourceEditor').fill(dirtyDraft);
    const graphState = page.locator('#graph [data-focus="state:pending"]');
    await graphState.focus(); await page.keyboard.press('Enter');
    await page.waitForFunction(() => view.selected === 'state:pending');
    check('keyboard graph selection preserves focus', await page.evaluate(() => document.activeElement.dataset.focus === 'state:pending'));
    await peer.waitForFunction(() => view.selected === 'state:pending');
    check('other-tab selection preserves inline draft object, base and guard', await peer.evaluate(base =>
      ruleDirty && ruleDraftId === 'approve_order' && ruleDraftBase === base &&
      document.querySelector('#editGuard').value === '!authorized' && !document.querySelector('#ruleEditor').hidden, initial));
    await page.locator('[data-object="rule:approve_order"]').click();
    await page.waitForFunction(() => view.selected === 'rule:approve_order');
    await page.locator('#question').fill('Should cancellation after approval succeed?');
    await page.locator('#replay').click();
    await page.waitForFunction(() => !!view.proposal);
    check('replay creates candidate without agreement', await page.evaluate(() => view.agreement === 'seed-example' && view.model.revision === view.proposal.base_revision));
    check('semantic witness is explicit', (await page.locator('#diffWitness').textContent()).includes('approved'));
    const candidate = await page.evaluate(() => ({ base_revision: view.proposal.base_revision, candidate_revision: view.proposal.revision }));
    await page.screenshot({ path: path.join(output, 'desktop-proposal.png'), fullPage: true });
    const measurements = await page.locator('nav,main,aside').evaluateAll(elements => elements.map(e => {
      const r = e.getBoundingClientRect(); return { tag: e.tagName, x: r.x, y: r.y, width: r.width, height: r.height };
    }));
    await page.locator('#accept').click();
    await page.waitForFunction(() => view.agreement === 'human-accepted' && !view.proposal);
    check('human acceptance changes exact revision only', await page.evaluate(revision => view.model.revision !== revision && view.realization === 'not-run', initial));
    await peer.waitForFunction(base => view.model.revision !== base, initial);
    check('other-tab acceptance preserves dirty editor text and exact old base',
      await peer.evaluate(({ base, draft }) => editorBase === base && editorDirty &&
        document.querySelector('#sourceEditor').value === draft, { base: initial, draft: dirtyDraft }));
    check('stale draft is labeled and review is disabled',
      (await peer.locator('#editRevision').textContent()).includes('Stale draft') &&
      await peer.locator('#checkEdit').isDisabled());
    await peer.screenshot({ path: path.join(output, 'stale-draft.png'), fullPage: true });
    await peer.locator('#closeEditor').click(); await peer.locator('#editButton').click();
    check('closing and reopening does not discard a stale draft',
      await peer.locator('#sourceEditor').inputValue() === dirtyDraft &&
      await peer.evaluate(base => editorBase === base && editorDirty, initial));
    await peer.locator('#editMode').selectOption('symbolic');
    check('projection switch cannot discard a dirty stale draft',
      await peer.locator('#editMode').inputValue() === 'english' &&
      await peer.locator('#sourceEditor').inputValue() === dirtyDraft);
    pendingRefusals.set(url + 'api/propose', 'HTTP400 stale draft proposal');
    const stale = await peer.evaluate(async () => {
      const response = await fetch('/api/propose', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-ZAL-Client': token },
        body: JSON.stringify({ revision: editorBase, syntax: document.querySelector('#editMode').value, surface: document.querySelector('#sourceEditor').value }) });
      return { status: response.status, data: await response.json() };
    });
    check('server independently refuses stale draft without a candidate', stale.status === 400 &&
      stale.data.error.toLowerCase().includes('stale') && await peer.evaluate(() => !view.proposal));
    await peer.locator('#discardEdit').click();
    check('explicit discard adopts current text and exact revision', await peer.evaluate(() =>
      !editorDirty && editorBase === view.model.revision && document.querySelector('#sourceEditor').value === view.model.english) &&
      await peer.locator('#checkEdit').isEnabled());
    await peer.locator('#closeEditor').click();
    check('other-tab acceptance preserves and labels stale inline fields', await peer.evaluate(base =>
      ruleDirty && ruleDraftId === 'approve_order' && ruleDraftBase === base &&
      document.querySelector('#editGuard').value === '!authorized', initial) &&
      (await peer.locator('#ruleDraftStatus').textContent()).includes('Stale transition draft') &&
      await peer.locator('#proposeRule').isDisabled());
    await peer.screenshot({ path: path.join(output, 'stale-transition.png'), fullPage: true });
    await peer.locator('#discardRule').click();
    check('explicit inline discard reloads selected rule at current revision', await peer.evaluate(() =>
      !ruleDirty && ruleDraftBase === view.model.revision && ruleDraftId === 'approve_order' &&
      document.querySelector('#editGuard').value === 'authorized') && await peer.locator('#proposeRule').isEnabled());
    await peer.close();
    pendingRefusals.set(url + 'api/accept', 'HTTP400 duplicate acceptance');
    const duplicate = await page.evaluate(async data => {
      const response = await fetch('/api/accept', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-ZAL-Client': token }, body: JSON.stringify(data) });
      return response.status;
    }, candidate);
    check('duplicate acceptance refused', duplicate === 400);
    for (const [event, authorized] of [['submit', false], ['approve', false], ['approve', true], ['cancel', false]]) {
      await page.locator('#traceEvent').selectOption(event);
      await page.locator('#fact-authorized').setChecked(authorized);
      const count = await page.evaluate(() => trace.length);
      await page.locator('#step').click();
      await page.waitForFunction(n => trace.length === n + 1, count);
    }
    check('accepted revision trace includes rejection frame', await page.evaluate(() => trace[1].outcome.class === 'Reject' && trace[1].pre === trace[1].outcome.state && trace[3].outcome.state === 'cancelled'));
    await page.locator('#scrubber').fill('2');
    await page.locator('#whyRecorded').click();
    await page.waitForFunction(() => document.querySelector('#explanation').textContent.startsWith('Recorded step 2') && document.querySelector('#explanation').textContent.includes('typed AST evaluation'));
    check('recorded Why uses exact attempted prestate and observations', (await page.locator('#explanation').textContent()).includes('pending + approve') && (await page.locator('#explanation').textContent()).includes('Frame: control unchanged'));
    check('trace scrub preserves model meaning', await page.evaluate(() => scrub === 2 && view.agreement === 'human-accepted'));
    await page.locator('#question').focus(); await page.locator('#question').fill('A draft survives polling.');
    await page.waitForTimeout(1100);
    check('polling does not steal focus or draft', await page.evaluate(() => document.activeElement.id === 'question' && document.querySelector('#question').value === 'A draft survives polling.' && scrub === 2));
    const exported = await page.evaluate(async () => { const r = await fetch('/api/export', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-ZAL-Client': token }, body: '{}' }); return { status: r.status, data: await r.json() }; });
    check('agreed finite profile exports factory declarations', exported.status === 200 && exported.data.files['v2/policy.json'] && exported.data.authority === 'none');
    await page.locator('#editButton').click();
    const older = (await page.locator('#sourceEditor').inputValue()).replaceAll('not_enabled', 'older_reason');
    await page.locator('#sourceEditor').fill(older);
    let release, reached;
    const pending = new Promise(resolve => { reached = resolve; });
    const gate = new Promise(resolve => { release = resolve; });
    await page.route('**/api/propose', async route => { reached(); await gate; await route.continue(); });
    await page.locator('#checkEdit').click(); await pending;
    const newer = older.replaceAll('older_reason', 'newer_reason');
    await page.locator('#sourceEditor').fill(newer); release();
    await page.waitForFunction(() => document.querySelector('#editFeedback').textContent.includes('newer text remains unreviewed'));
    check('delayed check cannot overwrite newer editor input', await page.locator('#sourceEditor').inputValue() === newer);
    await page.unroute('**/api/propose');
    await page.locator('#closeEditor').click(); await page.locator('#reject').click();
    await page.waitForFunction(() => !view.proposal);
    await page.locator('#editButton').click(); await page.locator('#discardEdit').click();
    await page.locator('#editMode').selectOption('symbolic');
    const overlapping = await page.locator('#sourceEditor').inputValue() + '\nrule overlap: pending + cancel [true] -> accept cancelled;\n';
    await page.locator('#sourceEditor').fill(overlapping); await page.locator('#checkEdit').click();
    await page.waitForFunction(() => view.proposal?.evidence.status === 'counterexample');
    check('invalid overlap cannot be accepted', await page.locator('#accept').isDisabled());
    await page.screenshot({ path: path.join(output, 'counterexample.png'), fullPage: true });
    await page.locator('#reject').click(); await page.waitForFunction(() => !view.proposal);
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.locator('#motion').setChecked(true);
    check('reduced motion overrides optional transition', await page.locator('#graph .state').first().evaluate(e => getComputedStyle(e).transitionDuration === '0s'));
    await page.setViewportSize({ width: 390, height: 844 });
    check('narrow viewport has no horizontal page overflow', await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.screenshot({ path: path.join(output, 'mobile.png'), fullPage: true });
    await page.locator('#legendButton').click();
    await page.waitForFunction(() => document.querySelector('#helpRevision').textContent.includes('Topic: overview'));
    await page.locator('#helpTopics').selectOption('grammar:default');
    await page.waitForFunction(() => document.querySelector('#legendText').textContent.includes('Otherwise reject'));
    check('help dialog has metadata-derived canonical English', (await page.locator('#legendText').textContent()).includes('Otherwise reject'));
    check('mobile help fits viewport', await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.keyboard.press('Escape');
    check('dialog returns focus to trigger', await page.evaluate(() => document.activeElement.id === 'legendButton'));
    check('all deliberate HTTP refusals were classified exactly once', pendingRefusals.size === 0 && expectedRefusals.length === 2);
    check('no live provider action during browser checks', providerRequests.length === 0);
    check('no unexpected browser or CSP errors', errors.length === 0);
    const report = { schema: 'zal/browser-check/1', network_path: 'native browser to loopback HTTP', checks,
      errors, expected_refusals: expectedRefusals, viewport: { width: 1440, height: 1000 }, measurements, live_model: 'not-run',
      limitations: ['Not a complete WCAG audit', 'No production adoption or usage-equivalence check'] };
    await fs.writeFile(path.join(output, 'browser-report.json'), JSON.stringify(report, null, 2) + '\n');
    console.log(JSON.stringify(report, null, 2));
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
