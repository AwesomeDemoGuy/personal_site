// Run against a site built with --lib-features hydrate,pretext-validation.
// Playwright is a development-only dependency; it is not needed to build the site.
import assert from 'node:assert/strict';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { gunzipSync, gzipSync } from 'node:zlib';
const engines = await import(process.env.PRETEXT_PLAYWRIGHT_MODULE || 'playwright');
const browserName = process.env.PRETEXT_BROWSER || 'chromium';
const base = process.env.PRETEXT_TEST_URL || 'http://127.0.0.1:31339';
const artifacts = new URL('../target/pretext-validation/', import.meta.url);
mkdirSync(artifacts, { recursive: true });
const oracle = readFileSync(new URL('../tests/fixtures/pretext/oracle.mjs', import.meta.url), 'utf8');
const fixtureCases = JSON.parse(gunzipSync(readFileSync(new URL('../tests/fixtures/pretext/cases.json.gz', import.meta.url))));
const texts = [...new Set(fixtureCases.map(c => c.text))];
const gpgSource = readFileSync(new URL('../src/pages/gpg.rs', import.meta.url), 'utf8').match(/pub const PUBLIC_KEY: &str = r#"([\s\S]*?)"#;/)[1];
const keyLines = gpgSource.split('\n');
const flowingKey = `${keyLines[0]}\n\n${keyLines.slice(1, -2).join('')}\n${keyLines.at(-2)}\n\n${keyLines.at(-1)}`;
const browser = await engines[browserName].launch({ headless: true, ...(browserName === 'chromium' && process.env.PRETEXT_CHROMIUM_PATH ? { executablePath: process.env.PRETEXT_CHROMIUM_PATH } : {}) });
const errors = [];
const assetPaths = new Set();
const page = await browser.newPage({ viewport: { width: 1100, height: 1000 } });
page.on('response', response => {
  const url = new URL(response.url());
  if (url.origin === new URL(base).origin && /\.(js|wasm)$/.test(url.pathname)) assetPaths.add(url.pathname);
});
await page.addInitScript(() => {
  window.pretextMeasurementCalls = 0;
  const measure = CanvasRenderingContext2D.prototype.measureText;
  CanvasRenderingContext2D.prototype.measureText = function (...args) { window.pretextMeasurementCalls++; return measure.apply(this, args); };
  const request = window.requestAnimationFrame.bind(window);
  window.pretextFrames = [];
  window.requestAnimationFrame = callback => request(time => {
    const start = performance.now(); callback(time);
    queueMicrotask(() => { document.body?.getBoundingClientRect(); window.pretextFrames.push(performance.now() - start); });
  });
});
page.on('pageerror', error => errors.push(String(error)));
page.on('console', message => { if (message.type() === 'error' && !message.text().includes('Failed to load resource')) errors.push(message.text()); });
const settle = async () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(() => requestAnimationFrame(resolve)))));
try {
  await page.goto(`${base}/about`);
  await page.waitForSelector('.intro .flow-line', { timeout: 30000 });
  await settle();
  if (process.env.PRETEXT_SKIP_PARITY) {
    const hasValidation = await page.evaluate(async () => { const module = await import('/pkg/personal_site.js'); return 'pretext_validate' in module || 'pretext_benchmark' in module; });
    assert.equal(hasValidation, false, 'production excludes validation exports');
  }
  await page.screenshot({ path: new URL('about.png', artifacts).pathname, fullPage: true });
  const parity = process.env.PRETEXT_SKIP_PARITY ? { cases: 0, productionSmoke: true } : await page.evaluate(async ({ oracle, texts }) => {
    const url = URL.createObjectURL(new Blob([oracle], { type: 'text/javascript' }));
    const js = await import(url); URL.revokeObjectURL(url);
    const rust = await import('/pkg/personal_site.js');
    if (!rust.pretext_validate) throw Error('Build with the pretext-validation feature');
    const compare = (a, b, path = '') => {
      if (typeof a === 'number' && typeof b === 'number') { if (Math.abs(a - b) > 1e-7) throw Error(`${path}: ${a} != ${b}`); return; }
      if (a === null || b === null || typeof a !== 'object' || typeof b !== 'object') { if (a !== b) throw Error(`${path}: ${JSON.stringify(a)} != ${JSON.stringify(b)}`); return; }
      if (Array.isArray(a) !== Array.isArray(b)) throw Error(`${path}: incompatible containers`);
      const keys = Object.keys(a); if (keys.length !== Object.keys(b).length) throw Error(`${path}: different keys`);
      for (const key of keys) compare(a[key], b[key], `${path}.${key}`);
    };
    let count = 0; const timings = { javascript: [], rust: [] };
    for (const font of ['16px Arial', '18px serif', '14px monospace']) for (const text of texts) for (const whiteSpace of ['normal', 'pre-wrap']) for (const wordBreak of ['normal', 'keep-all']) for (const letterSpacing of [0, 1.5, -0.75]) {
      const options = { whiteSpace, wordBreak, letterSpacing }; const widths = [0, 1, 14, 64, 127.5, 320, 1e9];
      js.clearCache(); js.setLocale('en');
      let start = performance.now();
      const p = js.prepareWithSegments(text, font, options);
      const keys = ['segments', 'kinds', 'widths', 'lineEndFitAdvances', 'lineEndPaintAdvances', 'breakableFitAdvances', 'breakablePreferredBreaks', 'spacingGraphemeCounts', 'discretionaryHyphenWidth', 'tabStopAdvance'];
      const prepared = Object.fromEntries(keys.map(key => [key, p[key]])); prepared.segLevels = p.segLevels === null ? null : Array.from(p.segLevels);
      const layouts = widths.map(width => ({ width, batch: js.layoutWithLines(p, width, 24), stats: js.measureLineStats(p, width), layout: js.layout(js.prepare(text, font, options), width, 24) }));
      const expected = { prepared, layouts, naturalWidth: js.measureNaturalWidth(p) };
      timings.javascript.push(performance.now() - start);
      start = performance.now(); const actual = JSON.parse(rust.pretext_validate(text, font, JSON.stringify(options), JSON.stringify(widths))); timings.rust.push(performance.now() - start);
      try { compare(actual, expected); } catch (error) { throw Error(`case ${count} ${JSON.stringify({ text, font, ...options })}: ${error.message}`); }
      count++;
    }
    const percentile = values => { const sorted = [...values].sort((a, b) => a - b); return { median: sorted[Math.floor(sorted.length / 2)], p95: sorted[Math.floor(sorted.length * .95)] }; };
    return { cases: count, timings: { javascript: percentile(timings.javascript), rust: percentile(timings.rust) }, timingScope: 'diagnostic preparation plus seven layouts; Rust also serializes and tests cache invalidation' };
  }, { oracle, texts });
  console.log('Browser parity:', JSON.stringify(parity));
  const benchmark = process.env.PRETEXT_SKIP_PARITY ? null : await page.evaluate(async oracle => {
    const url = URL.createObjectURL(new Blob([oracle], { type: 'text/javascript' }));
    const js = await import(url); URL.revokeObjectURL(url);
    const rust = await import('/pkg/personal_site.js');
    const text = "Hi! I'm Sebastian Ashkar, a senior computer science undergrad at Arizona State University. I have a particular interest in cyber security. ".repeat(10);
    const font = '16px Arial', rounds = [];
    for (let round = 0; round < 5; round++) {
      js.setLocale('en'); let start = performance.now();
      for (let i = 0; i < 30; i++) { js.clearCache(); js.prepareWithSegments(text, font); }
      const preparationMs = (performance.now() - start) / 30;
      const p = js.prepareWithSegments(text, font); let checksum = 0; start = performance.now();
      for (let i = 0; i < 1000; i++) checksum += js.measureLineStats(p, 160 + i % 320).lineCount;
      const warmGeometryMs = (performance.now() - start) / 1000; start = performance.now();
      for (let i = 0; i < 1000; i++) checksum += js.layoutWithLines(p, 160 + i % 320, 24).lineCount;
      const warmLinesMs = (performance.now() - start) / 1000;
      const actual = JSON.parse(rust.pretext_benchmark(text, font, 30, 1000));
      if (actual.checksum !== checksum) throw Error('benchmark line counts differ');
      rounds.push({ javascript: { preparationMs, warmGeometryMs, warmLinesMs, checksum }, rust: actual });
    }
    return { textLength: text.length, rounds, scope: 'cleared-cache preparation; warm geometry; warm materialized lines. Timers exclude JSON and boundary calls.' };
  }, oracle);
  await page.evaluate(() => {
    window.pretextMeasurementCalls = 0;
    window.pretextFrames = [];
  });
  const photo = page.locator('.profile-photo');
  let box = await photo.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2 + 400, { steps: 30 });
  await page.mouse.up(); await settle();
  assert.equal(await page.evaluate(() => window.pretextMeasurementCalls), 0, 'warm dragging must not remeasure fonts');
  assert.notEqual(await photo.evaluate(el => el.style.transform), '');
  const collisions = await page.evaluate(() => {
    const p = document.querySelector('.profile-photo').getBoundingClientRect();
    const cx = p.x + p.width / 2, cy = p.y + p.height / 2, radius = Math.min(p.width, p.height) / 2;
    return Array.from(document.querySelectorAll('.flow-line')).filter(el => {
      const r = el.getBoundingClientRect(); const dx = Math.max(r.left - cx, cx - r.right, 0), dy = Math.max(r.top - cy, cy - r.bottom, 0);
      return r.width > 0 && dx * dx + dy * dy < radius * radius;
    }).map(el => el.textContent);
  });
  assert.deepEqual(collisions, [], 'text must avoid the photo');
  const dragFrames = await page.evaluate(() => {
    const frames = window.pretextFrames.slice().sort((a, b) => a - b);
    return { samples: frames.length, medianMs: frames[Math.floor(frames.length / 2)], p95Ms: frames[Math.floor(frames.length * .95)], scope: 'RAF callback plus microtasks and forced DOM layout; excludes paint' };
  });
  await page.screenshot({ path: new URL('dragged.png', artifacts).pathname, fullPage: true });
  await page.getByRole('link', { name: 'Blog', exact: true }).click(); await page.waitForSelector('.blog .flow-line'); await settle();
  assert.equal(await photo.evaluate(el => el.style.transform), '', 'navigation resets the photo');
  await page.screenshot({ path: new URL(`blog-${browserName}.png`, artifacts).pathname, fullPage: true });
  await page.getByRole('link', { name: 'Projects', exact: true }).click(); await page.waitForSelector('.projects .flow-line');
  await settle(); await page.screenshot({ path: new URL(`projects-${browserName}.png`, artifacts).pathname, fullPage: true });
  await page.goBack(); await page.waitForSelector('.blog .flow-line');
  await page.getByRole('link', { name: 'About', exact: true }).click(); await page.waitForSelector('.intro .flow-line');
  const icon = page.locator('.cert-card .cert-icon'); box = await icon.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2); await page.mouse.down();
  await page.mouse.move(box.x + 160, box.y - 120, { steps: 10 }); await page.mouse.up(); await settle();
  assert.equal(await page.locator('.cert-icon[data-floating="1"]').count(), 1);
  const floatingIcon = page.locator('.cert-icon[data-floating="1"]');
  const floatingBox = await floatingIcon.boundingBox();
  const cardBox = await page.locator('.cert-card').boundingBox();
  await page.mouse.move(floatingBox.x + floatingBox.width / 2, floatingBox.y + floatingBox.height / 2); await page.mouse.down();
  await page.mouse.move(cardBox.x + cardBox.width / 2, cardBox.y + cardBox.height / 2, { steps: 10 }); await page.mouse.up(); await settle();
  assert.equal(await page.locator('.cert-icon[data-floating="1"]').count(), 0, 'dropping the icon over its card docks it');
  const touchPoint = await icon.evaluate(el => {
    const r = el.getBoundingClientRect(), x = r.x + r.width / 2, y = r.y + r.height / 2;
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerType: 'touch', pointerId: 72, clientX: x, clientY: y, button: 0 }));
    window.dispatchEvent(new PointerEvent('pointermove', { pointerType: 'touch', pointerId: 72, clientX: x + 20, clientY: y }));
    return { x, y };
  });
  await settle();
  assert.equal(await page.locator('.cert-icon[data-floating="1"]').count(), 1, 'touch pointer starts a floating drag');
  await page.evaluate(({ x, y }) => window.dispatchEvent(new PointerEvent('pointercancel', { pointerType: 'touch', pointerId: 72, clientX: x + 20, clientY: y })), touchPoint);
  await settle();
  await page.evaluate(({ x, y }) => window.dispatchEvent(new PointerEvent('pointermove', { pointerType: 'touch', pointerId: 72, clientX: x + 200, clientY: y })), touchPoint);
  await settle();
  assert.equal(await page.locator('.cert-icon[data-floating="1"]').count(), 0, 'pointer cancellation ends the gesture');
  box = await icon.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2); await page.mouse.down();
  await page.mouse.move(box.x + 160, box.y - 120, { steps: 10 });
  // Navigation during an active gesture must remove the portal and listeners.
  await page.getByRole('link', { name: 'Blog', exact: true }).focus();
  await page.keyboard.press('Enter'); await settle();
  await page.mouse.up();
  assert.equal(await page.locator('.cert-icon[data-floating="1"]').count(), 0, 'floating icons are released on navigation');
  await page.goto(`${base}/gpg`); await page.waitForSelector('.gpg pre .flow-line'); await settle();
  const gpgText = await page.locator('.gpg pre').textContent();
  assert.equal(gpgText, flowingKey, 'all GPG presentation text and hard breaks are preserved');
  const selected = await page.locator('.gpg pre').evaluate(el => { const range = document.createRange(); range.selectNodeContents(el); return range.toString(); });
  assert.equal(selected, flowingKey, 'selection preserves the complete GPG presentation');
  const raw = await page.request.get(`${base}/gpg`, { headers: { Accept: 'application/pgp-keys' } });
  assert.equal(await raw.text(), gpgSource, 'content negotiation serves the original importable key');
  await page.setViewportSize({ width: 375, height: 812 }); await settle();
  await page.screenshot({ path: new URL('gpg-mobile.png', artifacts).pathname, fullPage: true });
  await page.goto(`${base}/about`); await page.waitForSelector('.intro .flow-line'); await settle();
  assert.equal(await page.locator('.about-email a').getAttribute('href'), '/gpg');
  const tooWide = await page.locator('.tech-tags').evaluate(el => Array.from(el.children).some(child => child.getBoundingClientRect().width > el.getBoundingClientRect().width + 1));
  assert.equal(tooWide, false, 'chips fit narrow containers');
  await page.screenshot({ path: new URL('about-mobile.png', artifacts).pathname, fullPage: true });
  const fontPath = process.env.PRETEXT_TEST_FONT;
  if (fontPath) {
    await page.evaluate(() => { window.pretextMeasurementCalls = 0; });
    await page.route('**/pretext-test.ttf', async route => {
      // A delayed real font response verifies FontFaceSet invalidation.
      await new Promise(resolve => setTimeout(resolve, 150));
      await route.fulfill({ contentType: 'font/ttf', body: readFileSync(fontPath) });
    });
    await page.evaluate(async () => {
      const font = new FontFace('PretextQA', 'url(/pretext-test.ttf)');
      document.fonts.add(font);
      document.querySelector('.intro').style.fontFamily = 'PretextQA, serif';
      await font.load(); await document.fonts.ready;
    });
    await settle();
    assert(await page.evaluate(() => window.pretextMeasurementCalls) > 0, 'font loading must invalidate cached widths');
    const loadedFontLines = await page.evaluate(async oracle => {
      const url = URL.createObjectURL(new Blob([oracle], { type: 'text/javascript' }));
      const js = await import(url); URL.revokeObjectURL(url);
      const el = document.querySelector('.intro .flow-text'), css = getComputedStyle(el);
      const source = el.textContent;
      const prepared = js.prepareWithSegments(source, css.font, { whiteSpace: 'pre-wrap', letterSpacing: parseFloat(css.letterSpacing) || 0 });
      return { actual: Array.from(el.children).map(line => line.textContent), expected: js.layoutWithLines(prepared, el.getBoundingClientRect().width, parseFloat(css.lineHeight)).lines.map(line => line.text) };
    }, oracle);
    assert.deepEqual(loadedFontLines.actual, loadedFontLines.expected, 'loaded font uses the new measured widths');
    const oldHeight = await page.locator('.tech-tags').evaluate(el => el.getBoundingClientRect().height);
    await page.locator('.tech-tags > span').first().evaluate(el => { el.style.fontSize = '32px'; el.style.padding = '20px'; });
    await settle();
    assert(await page.locator('.tech-tags').evaluate(el => el.getBoundingClientRect().height) > oldHeight, 'changed chip sizes refresh row height');
  }
  const staticContext = await browser.newContext({ javaScriptEnabled: false });
  const staticPage = await staticContext.newPage(); await staticPage.goto(`${base}/about`);
  assert((await staticPage.locator('.intro').textContent()).includes('Sebastian Ashkar'));
  await staticContext.close();
  assert.deepEqual(errors, [], 'browser must have no hydration errors or runtime panics');
  const wasm = readFileSync(new URL('../target/site/pkg/personal_site.wasm', import.meta.url));
  const assets = [...assetPaths].map(path => { const bytes = readFileSync(new URL(`../target/site${path}`, import.meta.url)); return { path, bytes: bytes.length, gzipBytes: gzipSync(bytes).length }; });
  const report = { ...parity, browserName, benchmark, dragFrames, browserErrors: errors, warmDragMeasurementCalls: 0, wasmBytes: wasm.length, wasmGzipBytes: gzipSync(wasm).length, assets, totalGzipBytes: assets.reduce((sum, a) => sum + a.gzipBytes, 0), scenarios: ['SSR without JavaScript', 'hydration', 'photo drag', 'obstacle clearance', 'navigation reset', 'back navigation', 'certificate docking', 'synthetic touch pointer and cancellation', 'floating certificate cleanup during active drag', 'complete GPG text and selection', 'GPG content negotiation', 'mobile resize', 'atomic chip widths', ...(fontPath ? ['delayed font loading and new widths', 'changed chip dimensions'] : [])] };
  const reportName = `${process.env.PRETEXT_SKIP_PARITY ? 'production-' : ''}report-${browserName}.json`;
  writeFileSync(new URL(reportName, artifacts), JSON.stringify(report, null, 2) + '\n');
  console.log('Site scenarios passed; report:', new URL(reportName, artifacts).pathname);
} catch (error) { console.error('Browser errors:', errors); throw error; }
finally { await browser.close(); }
