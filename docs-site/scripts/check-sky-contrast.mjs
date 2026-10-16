#!/usr/bin/env node
// WCAG 2.2 AA contrast check for the living sky, across the times of day, in light and dark mode.
//
//   npm run build && npm run check:contrast            # 4 phases: dawn, day, sunset, night
//   npm run check:contrast -- --hours=all              # every hour, 00:00…23:00
//   CHROME=/path/to/chrome npm run check:contrast       # defaults to /usr/bin/google-chrome
//
// For every page × theme × hour it fakes the visitor's clock, then:
//   1. pixel check: for each visible text run, hides all text, screenshots the real composited
//      background (sky, clouds, stars, frosted surfaces) and computes the contrast of the text
//      colour against every background pixel under that text. Body text needs 4.5:1, large text
//      (≥ 24px, or ≥ 18.66px bold) 3:1, and UI icons 3:1. The worst pixel decides.
//   2. axe-core's color-contrast rule as a second opinion.
// It also checks that prefers-reduced-motion removes sky transitions, prefers-contrast: more hides
// the sky, and the sky layer is aria-hidden. Exits non-zero on any failure.
import { spawn } from 'node:child_process';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import puppeteer from 'puppeteer-core';
import sharp from 'sharp';

const require = createRequire(import.meta.url);
const axeSource = readFileSync(require.resolve('axe-core/axe.min.js'), 'utf8');
const args = Object.fromEntries(process.argv.slice(2).map((a) => a.replace(/^--/, '').split('=')));
const PHASES = { dawn: 6, day: 12, sunset: 17, night: 0 };
const hours = args.hours === 'all'
  ? Array.from({ length: 24 }, (_, h) => [`${String(h).padStart(2, '0')}:00`, h])
  : Object.entries(PHASES);
const pages = (args.pages ?? '/,/technical/architecture/,/guides/run-locally/').split(',');
const port = Number(args.port ?? 4392);
const base = `http://127.0.0.1:${port}`;

const lin = (c) => { c /= 255; return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4; };
const lum = ([r, g, b]) => 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
const ratio = (a, b) => { const x = lum(a), y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); };
const over = ([r, g, b, a], [R, G, B]) => [r * a + R * (1 - a), g * a + G * (1 - a), b * a + B * (1 - a)];

const preview = spawn('npx', ['astro', 'preview', '--port', String(port), '--host', '127.0.0.1', '--force'], { stdio: 'ignore', detached: true });
for (let i = 0; i < 60; i++) {
  try { if ((await fetch(base)).ok) break; } catch {}
  await new Promise((r) => setTimeout(r, 250));
}

const browser = await puppeteer.launch({ executablePath: process.env.CHROME ?? '/usr/bin/google-chrome', args: ['--no-sandbox', '--hide-scrollbars', '--font-render-hinting=none'] });
const failures = [];
const report = [];

async function open(path, theme, hour, media = []) {
  const page = await browser.newPage();
  await page.setViewport({ width: 1440, height: 900 });
  const cdp = await page.createCDPSession();
  await cdp.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }, ...media] });
  await page.evaluateOnNewDocument((theme, hour) => {
    localStorage.setItem('starlight-theme', theme);
    // Fake the visitor's clock: today at the given local hour.
    const RealDate = Date;
    const fixed = new RealDate(); fixed.setHours(hour, 0, 0, 0);
    const t = fixed.getTime();
    globalThis.Date = class extends RealDate {
      constructor(...a) { super(...(a.length ? a : [t])); }
      static now() { return t; }
    };
  }, theme, hour);
  await page.goto(base + path, { waitUntil: 'networkidle0' });
  await page.evaluate(() => document.fonts.ready);
  await new Promise((r) => setTimeout(r, 1200)); // mermaid renders client-side
  return page;
}

// Text runs and UI icons currently in the viewport.
const collect = () => {
  const out = [];
  const vh = innerHeight, vw = innerWidth;
  const visible = (el) => { const s = getComputedStyle(el); return s.visibility !== 'hidden' && s.display !== 'none' && Number(s.opacity) > 0; };
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let n; (n = walker.nextNode());) {
    if (!n.textContent.trim()) continue;
    const el = n.parentElement;
    if (!el || el.closest('.sr-only, .sl-skip-link, [aria-hidden="true"], script, style, noscript, .zoen-sky, dialog:not([open])') || !visible(el)) continue;
    const range = document.createRange(); range.selectNodeContents(n);
    for (const r of range.getClientRects()) {
      if (r.width < 2 || r.height < 4 || r.bottom <= 0 || r.top >= vh || r.right <= 0 || r.left >= vw) continue;
      // Only text that is actually painted on top at this point (not covered, clipped or off-screen).
      const hit = document.elementFromPoint(Math.min(vw - 1, Math.max(0, (r.left + r.right) / 2)), Math.min(vh - 1, Math.max(0, (r.top + r.bottom) / 2)));
      if (!hit || !(hit === el || el.contains(hit) || hit.contains(el))) continue;
      // Clip to scrolling/overflow ancestors (e.g. long lines in code blocks) so hidden overflow isn't sampled.
      let [l, t, rr, b] = [r.left, r.top, r.right, r.bottom];
      for (let a = el.parentElement; a && a !== document.body; a = a.parentElement) {
        const cs = getComputedStyle(a);
        if (cs.overflowX !== 'visible' || cs.overflowY !== 'visible') {
          const c = a.getBoundingClientRect(); // clip to the padding box: borders/scrollbars are not behind text
          const cl = c.left + a.clientLeft, ct = c.top + a.clientTop;
          l = Math.max(l, cl); t = Math.max(t, ct); rr = Math.min(rr, cl + a.clientWidth); b = Math.min(b, ct + a.clientHeight);
        }
      }
      if (rr - l < 2 || b - t < 4) continue;
      const s = getComputedStyle(el);
      const size = parseFloat(s.fontSize), bold = Number(s.fontWeight) >= 700;
      out.push({ kind: 'text', text: n.textContent.trim().slice(0, 40), color: s.color, large: size >= 24 || (bold && size >= 18.66), sel: el.tagName.toLowerCase() + (el.className && typeof el.className === 'string' ? '.' + el.className.split(' ')[0] : ''),
        rect: [Math.max(0, l), Math.max(0, t), Math.min(vw, rr), Math.min(vh, b)] });
    }
  }
  for (const svg of document.querySelectorAll('header svg, .sidebar-pane svg, .hero svg, .sl-link-button svg')) {
    if (svg.closest('[aria-hidden="true"]:not(svg)') && !svg.closest('a, button')) continue;
    const r = svg.getBoundingClientRect();
    if (r.width < 4 || r.bottom <= 0 || r.top >= vh || !visible(svg)) continue;
    out.push({ kind: 'icon', text: (svg.closest('a, button, label')?.getAttribute('aria-label') || svg.closest('a, button, label')?.textContent || 'icon').trim().slice(0, 40), color: getComputedStyle(svg).color, large: true, sel: 'svg', rect: [r.left, r.top, r.right, r.bottom] });
  }
  // Controls painted over text (Expressive Code's copy button): their pixels belong to the control.
  const overlays = [...document.querySelectorAll('.expressive-code .copy')].map((o) => { const q = o.getBoundingClientRect(); return [q.left, q.top, q.right, q.bottom]; });
  return { out, overlays };
};
const parse = (c) => { const m = c.match(/rgba?\(([^)]+)\)/); if (!m) return null; const p = m[1].split(/[ ,/]+/).filter(Boolean).map(Number); return [p[0], p[1], p[2], p[3] ?? 1]; };

async function pixelCheck(page, label) {
  const height = await page.evaluate(() => document.documentElement.scrollHeight);
  let worst = { r: Infinity };
  let checked = 0;
  for (let y = 0; y < Math.min(height, 900 * 10); y += 800) {
    await page.evaluate((y) => scrollTo({ top: y, behavior: 'instant' }), y);
    await new Promise((r) => setTimeout(r, 120));
    const { out: items, overlays } = await page.evaluate(collect);
    const covered = (x, y) => overlays.some(([a, b, c, d]) => x >= a && x < c && y >= b && y < d);
    await page.addStyleTag({ content: '*, *::before, *::after { color: transparent !important; -webkit-text-fill-color: transparent !important; text-shadow: none !important; } svg:not(.zoen-sky svg) { visibility: hidden !important; } img { visibility: visible; }' }).then((h) => (page._hide = h));
    await new Promise((r) => setTimeout(r, 80));
    const png = await page.screenshot({ type: 'png' });
    await page.evaluate((el) => el.remove(), page._hide);
    const { data, info } = await sharp(png).raw().toBuffer({ resolveWithObject: true });
    for (const it of items) {
      const fg = parse(it.color);
      if (!fg || fg[3] === 0) continue;
      const [x0, y0, x1, y1] = it.rect.map(Math.round);
      let min = Infinity;
      for (let py = y0; py < y1; py += 1) for (let px = x0; px < x1; px += 1) {
        if (px < 0 || py < 0 || px >= info.width || py >= info.height || covered(px, py)) continue;
        const i = (py * info.width + px) * info.channels;
        const bg = [data[i], data[i + 1], data[i + 2]];
        const r = ratio(over(fg, bg), bg);
        if (r < min) min = r;
      }
      if (!isFinite(min)) continue;
      checked++;
      const need = it.large ? 3 : 4.5;
      if (min < worst.r) worst = { r: min, ...it };
      if (min < need) failures.push(`${label}: ${it.kind} "${it.text}" (${it.sel}) ${min.toFixed(2)}:1 < ${need}:1 at scroll ${y}`);
    }
  }
  return { checked, worst };
}

async function axeCheck(page) {
  await page.evaluate((src) => { if (!window.axe) (0, eval)(src); }, axeSource);
  await page.evaluate(() => scrollTo({ top: 0, behavior: 'instant' }));
  return page.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ['color-contrast'] });
    return { violations: r.violations.flatMap((v) => v.nodes.map((n) => n.target.join(' ') + ' ' + (n.any[0]?.message ?? ''))), incomplete: r.incomplete.reduce((a, v) => a + v.nodes.length, 0), passes: r.passes.reduce((a, v) => a + v.nodes.length, 0) };
  });
}

for (const theme of (args.themes ?? 'light,dark').split(',')) for (const [name, hour] of hours) for (const path of pages) {
  const label = `${theme} ${name} ${path}`;
  const page = await open(path, theme, hour);
  const sky = await page.evaluate(() => ({ phase: document.documentElement.dataset.skyPhase, color: getComputedStyle(document.documentElement).getPropertyValue('--zoen-sky').trim(), hidden: document.querySelector('.zoen-sky')?.getAttribute('aria-hidden') }));
  if (sky.hidden !== 'true') failures.push(`${label}: sky layer is not aria-hidden`);
  const px = await pixelCheck(page, label);
  const axe = await axeCheck(page);
  for (const v of axe.violations) failures.push(`${label}: axe ${v}`);
  if (args.verbose) axe.violations.forEach((v) => console.log('   axe:', v));
  report.push({ theme, time: name, hour, path, sky: sky.color, phase: sky.phase, textRuns: px.checked, worst: +px.worst.r.toFixed(2), worstAt: `${px.worst.kind} "${px.worst.text}" (${px.worst.large ? 'large/UI, 3:1' : 'body, 4.5:1'})`, axeViolations: axe.violations.length, axePasses: axe.passes, axeIncomplete: axe.incomplete });
  console.log(`${label.padEnd(44)} sky ${sky.color.padEnd(18)} runs ${String(px.checked).padStart(4)}  worst ${px.worst.r.toFixed(2)}:1 ${px.worst.kind} "${px.worst.text}"  axe ${axe.violations.length} violations / ${axe.passes} passes / ${axe.incomplete} incomplete`);
  await page.close();
}

// Preference checks.
{
  const p = await open('/', 'light', 12, [{ name: 'prefers-reduced-motion', value: 'reduce' }]);
  const t = await p.evaluate(() => getComputedStyle(document.documentElement).transitionDuration);
  if (!/^0s(, 0s)*$/.test(t)) failures.push(`prefers-reduced-motion: sky still transitions (${t})`);
  console.log(`prefers-reduced-motion: reduce → transition-duration ${t}`);
  await p.close();
  const q = await open('/', 'light', 12, [{ name: 'prefers-contrast', value: 'more' }]);
  const d = await q.evaluate(() => getComputedStyle(document.querySelector('.zoen-sky')).display);
  if (d !== 'none') failures.push('prefers-contrast: more → sky still visible');
  console.log(`prefers-contrast: more → sky display ${d}`);
  await q.close();
}

await browser.close();
try { process.kill(-preview.pid); } catch {}
mkdirSync('contrast-report', { recursive: true });
writeFileSync('contrast-report/report.json', JSON.stringify({ hours: hours.map(([n, h]) => ({ n, h })), report, failures }, null, 2));
if (failures.length) { console.error(`\n${failures.length} contrast failure(s):\n` + failures.slice(0, 60).join('\n')); process.exit(1); }
console.log(`\nPASS: ${report.length} page/theme/time combinations meet WCAG 2.2 AA (text 4.5:1, large text and UI 3:1).`);
