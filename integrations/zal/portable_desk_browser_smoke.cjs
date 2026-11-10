/* Optional real-browser check of the portable desk: requires an already installed Playwright/Chromium and a
   build made by portable_desk_build.py with the pinned runtime. It serves the web folder on loopback, runs
   ZAL as WebAssembly in the page, compares every carried example with the result the build recorded natively,
   writes and checks a statement, starts again with the server stopped, and loads the extension folder.
       node integrations/zal/portable_desk_browser_smoke.cjs BUILD_DIR EVIDENCE_DIR
   It reads, types and presses; the desk has nothing to accept, export or send. */
'use strict';
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');
const built = path.resolve(process.argv[2] || ''), output = process.argv[3] || '/tmp/zal-portable-desk-browser';
if (!fs.existsSync(path.join(built, 'portable-desk-build.json'))) throw Error('Give the folder portable_desk_build.py wrote');
const TYPES = { html: 'text/html', js: 'text/javascript', mjs: 'text/javascript', css: 'text/css', json: 'application/json', webmanifest: 'application/manifest+json',
  wasm: 'application/wasm', zip: 'application/zip', png: 'image/png', txt: 'text/plain' };

function serve(folder, policy) {                                   // loopback only, files of the folder only; an optional policy on every reply
  const server = http.createServer((request, reply) => {
    const name = decodeURIComponent(new URL(request.url, 'http://x').pathname).replace(/\/$/, '/index.html'), file = path.join(folder, name);
    if (request.method !== 'GET' || !file.startsWith(folder + path.sep) || !fs.existsSync(file) || !fs.statSync(file).isFile()) { reply.writeHead(404).end(); return; }
    reply.writeHead(200, { 'Content-Type': TYPES[file.split('.').pop()] || 'application/octet-stream', 'Cache-Control': 'no-store', ...(policy ? { 'Content-Security-Policy': policy } : {}) }).end(fs.readFileSync(file));
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

(async () => {
  fs.mkdirSync(output, { recursive: true });
  const record = JSON.parse(fs.readFileSync(path.join(built, 'portable-desk-build.json'), 'utf8'));
  const server = await serve(path.join(built, 'web')), url = `http://127.0.0.1:${server.address().port}/`;
  const browser = await chromium.launch({ headless: true, ...(process.env.CHROMIUM ? { executablePath: process.env.CHROMIUM } : {}) });
  const errors = [], checks = [], foreign = [], measurements = {};
  const check = (name, condition) => { assert(condition, name); checks.push(name); };
  const watch = (page, own) => {
    page.on('pageerror', (error) => errors.push(error.message));
    page.on('console', (value) => { if (value.type() === 'error') errors.push(value.text()); });
    page.on('request', (request) => { if (!request.url().startsWith(own)) foreign.push(request.url()); });
  };
  const settled = (page) => page.waitForFunction(() => DESK.bundles[0] && !DESK.bundles[0].pending && !document.querySelector('#verdict .banner'), null, { timeout: 120000 });
  const tab = async (page, id) => { await page.locator('#tab-' + id).click(); await page.waitForFunction((wanted) => S.tab === wanted, id); };
  const fits = (page) => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth);
  try {
    // ---- the web folder, wide
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } }), page = await context.newPage();
    watch(page, url);
    const started = Date.now();
    await page.goto(url);
    check('the page states a policy that admits only its own files and WebAssembly', await page.evaluate(() => {
      const policy = document.querySelector('meta[http-equiv="Content-Security-Policy"]').content;
      return policy.includes("default-src 'none'") && policy.includes("script-src 'self' 'wasm-unsafe-eval'") && !/https?:/.test(policy) && !document.querySelector('script:not([src])'); }));
    await page.waitForFunction(() => DESK.bundles.length > 0, null, { timeout: 120000 });
    measurements.first_result_ms = Date.now() - started;         /* the engine starting and ZAL reading the first statement */
    await settled(page);
    check('ZAL runs in the page as WebAssembly and shows its own status first', await page.evaluate(() => DESK.bundles[0].python.length > 0 && document.querySelector('#verdict .status').textContent === DESK.bundles[0].check.status) &&
      (await page.locator('#foot').textContent()).includes('checker ' + record.checker.slice(0, 12)));
    measurements.examples = {};
    for (const example of record.examples) {                       // each carried example, as this browser computes it
      const asked = Date.now();
      await page.locator('#shelf').selectOption('example:' + example.file);
      await page.waitForFunction((file) => DESK.bundles[0] && DESK.bundles[0].file === file && !DESK.bundles[0].pending, example.file, { timeout: 120000 });
      measurements.examples[example.file] = Date.now() - asked;
      await tab(page, 'evidence');
      const said = await page.locator('#panel').textContent(), status = await page.locator('#verdict .status').first().textContent();
      check(`this browser's result for ${example.file} is the one the build recorded natively`, status === example.status && said.includes("this device's result for it is the one recorded when the desk was built") &&
        !(await page.locator('#verdict').textContent()).includes('Do not rely'));
    }
    await page.screenshot({ path: path.join(output, 'wide.png') });

    // ---- writing
    const source = record.examples.find((example) => example.file === 'order2.zal') || record.examples[0];
    const text = await page.evaluate((file) => DESK.portable.examples.find((example) => example.file === file).source, source.file);
    await page.locator('#new').click();
    check('New opens a blank statement under Write with the keyboard in the text', await page.evaluate(() => document.activeElement.id === 'source' && document.querySelector('#source').value === '' && document.querySelector('#check').disabled));
    await page.locator('#source').fill(text.replace(/^(rule \w+):/m, '$1 oops:'));
    await page.locator('#check').click();
    await page.waitForFunction(() => document.querySelector('#write-said').textContent.includes('ZAL does not accept this text.'), null, { timeout: 60000 });
    check("a text ZAL refuses shows ZAL's located diagnostic", /line \d+/.test(await page.locator('#write-said').textContent()));
    await page.getByRole('button', { name: /^Go to line/ }).click();
    check('Go to line selects the line ZAL names', await page.evaluate(() => { const area = document.querySelector('#source'); return document.activeElement === area && area.value.slice(area.selectionStart, area.selectionEnd).includes(' oops:'); }));
    await page.locator('#source').fill(text);
    await page.locator('#check').click();
    await page.waitForFunction(() => DESK.bundles[0] && document.querySelector('#write-said').textContent.includes('ZAL read this text.'), null, { timeout: 60000 });
    check('the corrected text is read by ZAL and shown on the desk', (await page.locator('#verdict .status').first().textContent()) === source.status);
    await page.locator('#source').focus(); await page.keyboard.press('Control+End'); await page.keyboard.type('\n# a note');
    await page.waitForFunction(() => !DESK.bundles[0].pending, null, { timeout: 60000 });
    check('typing is never interrupted by a redraw: the keyboard and the caret stay in the text', await page.evaluate(() => { const area = document.querySelector('#source'); return document.activeElement === area && area.selectionStart === area.value.length && area.value.endsWith('# a note'); }));
    check('the verdict says the text has changed since ZAL read it', (await page.locator('#verdict').textContent()).includes('You have changed the text since ZAL read it'));
    await tab(page, 'try');
    await page.waitForSelector('#panel .result .say', { timeout: 60000 });
    check('an input is explained by ZAL in the page', (await page.locator('#panel .result .say').textContent()).startsWith('From '));
    await tab(page, 'changes');
    await page.locator('#compare').click();
    await page.waitForSelector('#cand-result', { timeout: 60000 });
    check('an edit is compared by ZAL before it is checked, and replaces nothing', (await page.locator('#cand-result').textContent()).includes('What your candidate would change'));
    await page.evaluate(() => window.dispatchEvent(new Event('pagehide')));
    await page.reload();
    await page.waitForSelector('#verdict .status', { timeout: 120000 });
    check("after a reload the reader's text is still on this device", await page.evaluate(() => document.querySelector('#source').value.endsWith('# a note') && document.querySelector('#shelf').value.startsWith('mine:')));
    await page.emulateMedia({ colorScheme: 'dark' });
    check('the dark scheme changes the page ground', await page.evaluate(() => getComputedStyle(document.body).backgroundColor) !== 'rgb(236, 239, 243)');
    await page.emulateMedia({ colorScheme: 'light' });

    // ---- starting again with no server
    await page.evaluate(() => navigator.serviceWorker.ready);
    await page.reload();                                           // this load is the first one answered from the device's own copy
    await page.waitForSelector('#verdict .status', { timeout: 120000 });
    await new Promise((resolve) => server.close(resolve));
    server.closeAllConnections();
    await page.reload();
    await page.waitForSelector('#verdict .status', { timeout: 120000 });
    await page.locator('#shelf').selectOption('example:' + record.examples[0].file);
    await page.waitForFunction((file) => DESK.bundles[0] && DESK.bundles[0].file === file && !DESK.bundles[0].pending, record.examples[0].file, { timeout: 120000 });
    check('with the server stopped the desk starts from its own copy and ZAL still answers', (await page.locator('#verdict .status').first().textContent()) === record.examples[0].status);
    await context.close();

    // ---- a phone
    const again = await serve(path.join(built, 'web')), phoneUrl = `http://127.0.0.1:${again.address().port}/`;
    const phone = await browser.newContext({ viewport: { width: 390, height: 844 }, deviceScaleFactor: 2, hasTouch: true, isMobile: true }), small = await phone.newPage();
    watch(small, phoneUrl);
    await small.goto(phoneUrl);
    await small.waitForSelector('#verdict .status', { timeout: 120000 });
    await settled(small);
    for (const id of ['statement', 'evidence', 'try', 'changes', 'write']) {
      await tab(small, id);
      check(`a phone-sized page has no sideways overflow (${id})`, await fits(small));
    }
    check('on a phone the controls are large enough for a finger and the text does not make the page zoom', await small.evaluate(() =>
      [...document.querySelectorAll('#writer .tokens .opt, #check')].every((node) => node.getBoundingClientRect().height >= 40) && parseFloat(getComputedStyle(document.querySelector('#source')).fontSize) >= 16));
    await small.screenshot({ path: path.join(output, 'phone-write.png') });
    await tab(small, 'statement');
    await small.screenshot({ path: path.join(output, 'phone-statement.png') });
    await phone.close();
    await new Promise((resolve) => again.close(resolve));

    // ---- a host that forbids WebAssembly
    const strict = await serve(path.join(built, 'web'), "script-src 'self'; worker-src 'self'"), strictUrl = `http://127.0.0.1:${strict.address().port}/`;
    const barred = await browser.newContext(), denied = await barred.newPage();
    await denied.goto(strictUrl);
    await denied.waitForFunction(() => document.querySelector('#write-said').textContent.includes('could not start'), null, { timeout: 60000 });
    check('where WebAssembly is forbidden the desk says so at once and checks nothing', (await denied.locator('#write-said').textContent()).includes('not allowed to run WebAssembly') && await denied.evaluate(() => document.querySelector('#check').disabled && DESK.bundles.length === 0));
    await barred.close();
    await new Promise((resolve) => strict.close(resolve));

    // ---- the extension folder, loaded as an extension
    const folder = path.join(built, 'extension');
    let loaded = null;
    try {
      loaded = await chromium.launchPersistentContext(fs.mkdtempSync(path.join(os.tmpdir(), 'zal-portable-desk-')), { channel: 'chromium', headless: true,
        args: [`--disable-extensions-except=${folder}`, `--load-extension=${folder}`] });
      const worker = loaded.serviceWorkers()[0] || await loaded.waitForEvent('serviceworker', { timeout: 30000 });
      const own = `chrome-extension://${worker.url().split('/')[2]}/`, inside = await loaded.newPage();
      watch(inside, own);
      check('the extension loads and its button has a listener', await worker.evaluate(() => chrome.action.onClicked.hasListeners() && !chrome.runtime.getManifest().permissions));
      await inside.goto(own + 'index.html');
      await inside.waitForSelector('#verdict .status', { timeout: 120000 });
      await settled(inside);
      await tab(inside, 'evidence');
      check("inside the extension ZAL runs under the manifest's policy and gives the build's result", (await inside.locator('#panel').textContent()).includes("this device's result for it is the one recorded when the desk was built"));
      await inside.screenshot({ path: path.join(output, 'extension.png') });
    } finally { if (loaded) await loaded.close(); }

    check('nothing outside the desk\'s own files was requested', foreign.length === 0);
    check('no unexpected browser or policy errors', errors.length === 0);
    const report = { schema: 'zal/portable-desk-browser-check/1', checks, errors, checker: record.checker, runtime: record.runtime.package + ' ' + record.runtime.version, measurements,
      limitations: ['One browser engine (Chromium); Firefox, Safari and Brave were not run', 'A phone is emulated by viewport and touch, not by a device',
        "The extension's toolbar button is not pressed; its listener is only observed", 'Not a complete WCAG audit', 'Typing and presses are browser automation, not a person writing a statement'] };
    fs.writeFileSync(path.join(output, 'portable-desk-browser-report.json'), JSON.stringify(report, null, 2) + '\n');
    console.log(JSON.stringify(report, null, 2));
  } finally { await browser.close(); server.close(); }
})().catch((error) => { console.error(error); process.exitCode = 1; });
