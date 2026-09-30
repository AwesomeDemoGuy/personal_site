// Measure the frozen site built with the same release profile as the migration.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { gzipSync } from 'node:zlib';
const { chromium } = await import(process.env.PRETEXT_PLAYWRIGHT_MODULE || 'playwright');
const base = process.env.PRETEXT_TEST_URL || 'http://127.0.0.1:31340';
const site = process.env.PRETEXT_BASELINE_SITE;
if (!site) throw Error('Set PRETEXT_BASELINE_SITE to the frozen release site directory');
const browser = await chromium.launch({ headless: true, ...(process.env.PRETEXT_CHROMIUM_PATH ? { executablePath: process.env.PRETEXT_CHROMIUM_PATH } : {}) });
try {
  const page = await browser.newPage({ viewport: { width: 1100, height: 1000 } });
  const requests = new Set(); page.on('response', response => { const url = new URL(response.url()); if (/\.(js|wasm)$/.test(url.pathname)) requests.add(url.pathname); });
  await page.addInitScript(() => {
    const request = requestAnimationFrame.bind(window); window.pretextFrames = [];
    window.requestAnimationFrame = callback => request(time => {
      const start = performance.now(); callback(time);
      queueMicrotask(() => { document.body?.getBoundingClientRect(); window.pretextFrames.push(performance.now() - start); });
    });
    const measure = CanvasRenderingContext2D.prototype.measureText;
    window.pretextMeasurementCalls = 0;
    CanvasRenderingContext2D.prototype.measureText = function (...args) { window.pretextMeasurementCalls++; return measure.apply(this, args); };
  });
  await page.goto(base + '/about'); await page.waitForSelector('.intro .flow-line');
  await page.evaluate(() => document.fonts.ready);
  const settle = async () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(() => requestAnimationFrame(resolve)))));
  await settle();
  const artifacts = new URL('../target/pretext-validation/', import.meta.url); mkdirSync(artifacts, { recursive: true });
  await page.screenshot({ path: new URL('baseline-about.png', artifacts).pathname, fullPage: true });
  await page.evaluate(() => { window.pretextFrames = []; window.pretextMeasurementCalls = 0; });
  const box = await page.locator('.profile-photo').boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2); await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2 + 400, { steps: 30 }); await page.mouse.up(); await settle();
  await page.screenshot({ path: new URL('baseline-dragged.png', artifacts).pathname, fullPage: true });
  const measurements = await page.evaluate(() => {
    const frames = window.pretextFrames.slice().sort((a, b) => a - b);
    return { warmDragMeasurementCalls: window.pretextMeasurementCalls, dragFrames: { samples: frames.length, medianMs: frames[Math.floor(frames.length / 2)], p95Ms: frames[Math.floor(frames.length * .95)], scope: 'RAF callback plus microtasks and forced DOM layout; excludes paint' } };
  });
  const assets = [...requests].map(path => { const bytes = readFileSync(site + path); return { path, bytes: bytes.length, gzipBytes: gzipSync(bytes).length }; });
  const report = { ...measurements, assets, totalGzipBytes: assets.reduce((sum, a) => sum + a.gzipBytes, 0) };
  writeFileSync(new URL('baseline-report.json', artifacts), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify(report));
} finally { await browser.close(); }
