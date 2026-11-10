/* Optional real-browser check of the review desk: requires an already installed Playwright/Chromium.
   Start `python3 integrations/zal/review_desk.py serve SOURCE.zal`, then pass its loopback URL and an
   evidence directory. It reads and presses; the desk has nothing to accept, export or send to a provider. */
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
    const shown = await page.evaluate(() => ({ status: DESK.bundles[0].check.status, live: LIVE, table: DESK.bundles[0].decisions.status,
      edits: (DESK.bundles[0].edits || { rows: [] }).rows.length, symbolic: DESK.bundles[0].symbolic }));
    check('actual browser HTTP path shows ZAL status first', shown.live && (await page.locator('#verdict .status').first().textContent()) === shown.status);
    check('wide layout keeps the statement beside the instruments', await page.evaluate(() => {
      const left = document.querySelector('#statement').getBoundingClientRect(), right = document.querySelector('#panel').getBoundingClientRect();
      return left.width > 300 && right.left >= left.right - 1 && document.documentElement.scrollHeight <= innerHeight; }));
    await page.screenshot({ path: path.join(output, 'wide.png') });

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
    for (const clause of await page.locator('#statement button.clause').all()) await clause.click();
    check('every clause of the statement opens', await page.locator('#statement button.clause').count() > 0);

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
    await page.screenshot({ path: path.join(output, 'narrow.png'), fullPage: true });
    check('nothing outside the loopback desk was requested', foreign.length === 0);
    check('no unexpected browser or CSP errors', errors.length === 0);
    const report = { schema: 'zal/review-desk-browser-check/1', network_path: 'native browser to loopback HTTP', checks, errors,
      statement_status: shown.status, decision_table: shown.table, planted_edits: shown.edits, measurements,
      limitations: ['Not a complete WCAG audit', 'One source file and one browser engine per run',
        'Presses are browser automation, not a person reviewing meaning',
        'The desk records no agreement, so none is exercised'] };
    await fs.writeFile(path.join(output, 'review-desk-browser-report.json'), JSON.stringify(report, null, 2) + '\n');
    console.log(JSON.stringify(report, null, 2));
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
