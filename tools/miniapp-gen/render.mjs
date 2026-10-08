// Headless render of a built mini-app (system Chrome via puppeteer-core): iPhone-sized
// viewport, waits for the app to settle, screenshots, and returns console errors.
import puppeteer from 'puppeteer-core';
import { resolve } from 'node:path';

export async function render(htmlPath, pngPath, { width = 393, height = 852, waitMs = 3500, actions = [], query = '' } = {}) {
  const browser = await puppeteer.launch({
    executablePath: '/usr/bin/google-chrome', headless: true,
    args: ['--no-sandbox', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--hide-scrollbars'],
  });
  const errors = [];
  try {
    const page = await browser.newPage();
    await page.setViewport({ width, height, deviceScaleFactor: 2, isMobile: true, hasTouch: true });
    page.on('pageerror', (e) => errors.push(String(e.message ?? e)));
    page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
    await page.goto(`file://${resolve(htmlPath)}${query}`, { waitUntil: 'load' });
    await new Promise((r) => setTimeout(r, waitMs));
    for (const a of actions) {
      if (a.click) await page.click(a.click).catch((e) => errors.push(`click ${a.click}: ${e.message}`));
      if (a.wait) await new Promise((r) => setTimeout(r, a.wait));
      if (a.shot) await page.screenshot({ path: a.shot });
    }
    await page.screenshot({ path: pngPath });
    const text = await page.evaluate(() => document.body.innerText.slice(0, 2000));
    return { errors, text };
  } finally { await browser.close(); }
}

if (process.argv[1]?.endsWith('render.mjs')) {
  const [html, png, q] = process.argv.slice(2);
  const r = await render(html, png, { query: q ?? '', waitMs: 6000 });
  console.log(JSON.stringify(r.errors));
}
