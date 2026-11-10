/* Optional real-browser check of the review desk: requires an already installed Playwright/Chromium.
   Start `python3 integrations/zal/review_desk.py serve SOURCE.zal`, then pass its loopback URL and an
   evidence directory. It reads, points and presses; the desk has nothing to accept, export or send to a provider. */
'use strict';
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const url = process.argv[2] || 'http://127.0.0.1:8770/';
const output = process.argv[3] || '/tmp/zal-review-desk-browser';
if (!/^http:\/\/127\.0\.0\.1:\d+\/$/.test(url)) throw Error('Use the loopback review desk URL');

(async () => {
  await fs.mkdir(output, { recursive: true });
  const browser = await chromium.launch({ headless: true,
    ...(process.env.CHROMIUM ? { executablePath: process.env.CHROMIUM } : {}) });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const errors = [], checks = [], foreign = [], measurements = {};
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', value => { if (value.type() === 'error') errors.push(value.text()); });
  page.on('request', request => { if (!request.url().startsWith(url)) foreign.push(request.url()); });
  const check = (name, condition) => { assert(condition, name); checks.push(name); };
  const fits = () => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth);
  const tab = async id => { await page.locator('#tab-' + id).click(); await page.waitForFunction(wanted => S.tab === wanted, id); };
  try {
    const served = (await page.goto(url)).headers();
    check('the page is served uncached with its no-outside-content policy', served['cache-control'] === 'no-store' &&
      ["default-src 'none'", "connect-src 'self'", "frame-ancestors 'none'"].every(part => (served['content-security-policy'] || '').includes(part)));
    await page.waitForSelector('#verdict .status');
    await page.evaluate(() => { localStorage.clear(); }); await page.reload(); await page.waitForSelector('#verdict .status');
    const shown = await page.evaluate(() => ({ status: DESK.bundles[0].check.status, live: LIVE, table: DESK.bundles[0].decisions.status,
      edits: (DESK.bundles[0].edits || { rows: [] }).rows.length, symbolic: DESK.bundles[0].symbolic }));
    check('actual browser HTTP path shows ZAL status first', shown.live && (await page.locator('#verdict .status').first().textContent()) === shown.status);
    check('wide layout keeps the statement beside the instruments', await page.evaluate(() => {
      const left = document.querySelector('#statement').getBoundingClientRect(), right = document.querySelector('#panel').getBoundingClientRect();
      return left.width > 300 && right.left >= left.right - 1 && document.documentElement.scrollHeight <= innerHeight; }));

    const rule = page.locator('#statement .clause[data-clause^="rule:"]').first(), ruleKey = await rule.getAttribute('data-clause');
    check('a first visit reads English and formal symbols', (await page.locator('#statement .seg button[aria-pressed="true"]').allTextContents()).join() === 'English,Symbols' &&
      await page.locator('#statement .fm .g.o').count() > 0 && (await rule.locator('.fm').textContent()).includes('⇒'));
    check('the reading key is docked under the statement and covers none of it', await page.evaluate(() => {
      const doc = document.querySelector('#doc').getBoundingClientRect(), key = document.querySelector('#key').getBoundingClientRect();
      return key.top >= doc.bottom - 1 && key.height > 60 && key.bottom <= innerHeight + 1; }));
    await rule.locator('.cl-head').hover();
    check('pointing at a line turns the key to that line', await page.evaluate(key => K.at === key && document.querySelector('#key .for').textContent.startsWith('rule ') &&
      document.querySelectorAll('#key .krow').length > 0 && document.querySelector(`#statement .clause[data-clause="${key}"]`).classList.contains('at'), ruleKey));
    check('what the rule leaves unchanged shows while its line is read', await page.evaluate(key => {
      const more = document.querySelector(`#statement .clause[data-clause="${key}"] .fm-more`); return !more || getComputedStyle(more).display !== 'none'; }, ruleKey));
    await rule.locator('.g[data-k="implies"]').first().hover();
    check('the symbol under the pointer is lit in the line and in the key, and its row is in view', await page.evaluate(() => {
      const lit = document.querySelector('#key .hot[data-k="implies"]'), body = document.querySelector('#key .key-b');
      if (!lit || !document.querySelector('#statement .g.hot[data-k="implies"]')) return false;
      const row = lit.getBoundingClientRect(), box = body.getBoundingClientRect(); return row.top >= box.top - 1 && row.bottom <= box.bottom + 1; }));
    await page.waitForTimeout(250);                                 // the mark fades in
    await page.screenshot({ path: path.join(output, 'wide.png') });
    await page.locator('#key .krow[data-k="implies"] button').click();
    check('a learned symbol shrinks to a word and the browser keeps it', await page.evaluate(() =>
      JSON.parse(localStorage.getItem('zal-review-desk-learned')).includes('implies') && !!document.querySelector('#key .kchip.known[data-k="implies"]')));
    await page.reload(); await page.waitForSelector('#verdict .status');
    check('a learned symbol is still learned after a reload', await page.evaluate(() => K.known.has('implies')));
    await page.evaluate(() => { localStorage.removeItem('zal-review-desk-learned'); }); await page.reload(); await page.waitForSelector('#verdict .status');
    await page.locator('#statement .clause[data-clause^="rule:"] .cl-head').first().focus();
    check('the keyboard moves the key as the pointer does', await page.evaluate(key => K.at === key, ruleKey));

    await tab('evidence');
    check('evidence lists every planted edit', await page.locator('#panel tr.row').count() === shown.edits);
    await tab('try');
    await page.waitForSelector('#panel .result .say');
    const options = await page.locator('#panel .opt').count();
    for (let at = 0; at < options; at += 1) {
      await page.locator('#panel .opt').nth(at).click();
      await page.waitForSelector('#panel .result .say');
    }
    check('every option can be pressed and ZAL answers each input', options > 0 && (await page.locator('#panel .result .say').textContent()).startsWith('From '));
    check('a formula on the Try tab shows the truth of its parts, in symbols', await page.locator('#panel .truth .fm').count() === 0 ||
      await page.locator('#panel .truth .tv[data-v]').count() > 0);
    const first = page.locator('#panel .opt').first(), label = await first.textContent();
    await first.focus(); await page.keyboard.press('Space');
    check('keyboard focus stays on the pressed option after the redraw', await page.evaluate(wanted =>
      document.activeElement.classList.contains('opt') && document.activeElement.textContent === wanted && document.activeElement.getAttribute('aria-pressed') === 'true', label));
    await page.keyboard.press('Tab');
    check('focus is visible on the next control', await page.evaluate(() => getComputedStyle(document.activeElement).outlineStyle !== 'none'));
    for (let step = 0; step < 3 && await page.getByRole('button', { name: 'Take this step' }).count(); step += 1) {
      await page.getByRole('button', { name: 'Take this step' }).click();
      await page.waitForSelector('#panel .result .say');
    }
    measurements.steps_taken = await page.locator('#panel .trace li').count();
    for (const line of await page.locator('#statement .cl-head').all()) await line.click();
    check('every line of the statement opens', await page.locator('#statement .cl-head').count() > 0 &&
      await page.locator('#statement .cl-head[aria-expanded="true"]').count() === 1 && await page.evaluate(() => !document.querySelector('button button')));

    await tab('changes');
    await page.getByRole('button', { name: 'Check this candidate' }).click();
    await page.waitForSelector('#cand-result');
    check('an unchanged candidate goes to ZAL and changes no decision', (await page.locator('#cand-result').textContent()).includes('it decides every declared input as before'));
    await page.locator('#candidate').fill(shown.symbolic.replace(/^rule (\w+):/m, 'rule $1 oops:'));
    await page.getByRole('button', { name: 'Check this candidate' }).click();
    await page.waitForFunction(() => document.querySelector('#cand-result').textContent.includes('ZAL does not accept your candidate.'));
    check('a candidate ZAL refuses shows its located diagnostic', /line \d+/.test(await page.locator('#cand-result').textContent()));
    check('a candidate replaces nothing', await page.evaluate(wanted => DESK.bundles[0].symbolic === wanted, shown.symbolic));
    await page.screenshot({ path: path.join(output, 'candidate.png'), fullPage: true });

    await page.emulateMedia({ colorScheme: 'dark' });
    const light = 'rgb(236, 239, 243)';
    check('dark scheme changes the page ground', await page.evaluate(() => getComputedStyle(document.body).backgroundColor) !== light);
    await page.screenshot({ path: path.join(output, 'dark.png') });
    await page.emulateMedia({ colorScheme: 'light' });

    await page.setViewportSize({ width: 390, height: 844 });
    for (const id of ['statement', 'evidence', 'try', 'changes']) {
      await tab(id);
      check(`narrow viewport has no horizontal page overflow (${id})`, await fits());
    }
    await tab('statement');
    await page.evaluate(() => { S.sel = null; render(); });
    const narrowRule = page.locator('#panel .clause[data-clause^="rule:"]').first();
    await narrowRule.locator('.cl-head').click();
    check('on a narrow screen an opened line carries its own reading key, and the page still fits', await narrowRule.locator('.key.inline .krow').count() > 0 &&
      await page.locator('#key').count() === 0 && await fits());
    await page.screenshot({ path: path.join(output, 'narrow.png'), fullPage: true });
    check('nothing outside the loopback desk was requested', foreign.length === 0);
    check('no unexpected browser or CSP errors', errors.length === 0);
    const report = { schema: 'zal/review-desk-browser-check/1', network_path: 'native browser to loopback HTTP', checks, errors,
      statement_status: shown.status, decision_table: shown.table, planted_edits: shown.edits, measurements,
      limitations: ['Not a complete WCAG audit', 'One source file and one browser engine per run',
        'Pointing and presses are browser automation, not a person reading the formulas',
        'The desk records no agreement, so none is exercised'] };
    await fs.writeFile(path.join(output, 'review-desk-browser-report.json'), JSON.stringify(report, null, 2) + '\n');
    console.log(JSON.stringify(report, null, 2));
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
