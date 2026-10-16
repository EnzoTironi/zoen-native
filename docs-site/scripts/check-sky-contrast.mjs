#!/usr/bin/env node
// WCAG 2.2 AA contrast check for the living sky (real time-lapse footage), across the times of day,
// in light and dark mode, on several real video frames per phase, at a normal and a 2160px-tall window.
//
//   npm run build && npm run check:contrast            # 4 phases: dawn, day, sunset, night
//   npm run check:contrast -- --hours=all              # every hour, 00:00…23:00
//   CHROME=/path/to/chrome npm run check:contrast       # defaults to /usr/bin/google-chrome
//
// For every page × theme × hour × viewport it fakes the visitor's clock, then for each of FRAMES
// (seconds into the 8 s loop) pauses the footage on that frame and:
//   1. pixel check: for each visible text run, hides all text, screenshots the real composited
//      background (sky, clouds, stars, frosted surfaces) and computes the contrast of the text
//      colour against every background pixel under that text. Body text needs 4.5:1, large text
//      (≥ 24px, or ≥ 18.66px bold) 3:1, and UI icons 3:1. The worst pixel decides.
//      Diagram text (Mermaid) is measured against the diagram's own fills: only its glyphs are hidden.
//   2. axe-core's color-contrast rule as a second opinion. Nodes axe cannot decide ("incomplete",
//      e.g. text over images or gradients) are not ignored: each must be covered by a pixel
//      measurement (scrolled into view if needed) that passes, or the check fails.
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
const PHASES = { dawn: 6.5, day: 12, sunset: 18, night: 1 };
const FRAMES = (args.frames ?? '0.5,3,6').split(',').map(Number);
// Viewport sets: "WxH" applies to every page; "WxH@/path/" only to that page.
const VIEWPORTS = (args.viewports ?? '1440x900,1440x2160@/technical/architecture/').split(',').map((v) => {
  const [size, only] = v.split('@'); const [w, h] = size.split('x').map(Number); return { w, h, only };
});
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

const browser = await puppeteer.launch({ executablePath: process.env.CHROME ?? '/usr/bin/google-chrome', args: ['--no-sandbox', '--hide-scrollbars', '--font-render-hinting=none', '--autoplay-policy=no-user-gesture-required'] });
const failures = [];
const report = [];

async function open(path, theme, hour, media = [], vp = { w: 1440, h: 900 }) {
  const page = await browser.newPage();
  await page.setViewport({ width: vp.w, height: vp.h });
  if (media.length) { const cdp = await page.createCDPSession(); await cdp.send('Emulation.setEmulatedMedia', { features: media }); }
  await page.evaluateOnNewDocument((theme, hour) => {
    localStorage.setItem('starlight-theme', theme);
    // Fake the visitor's clock: today at the given local hour.
    const RealDate = Date;
    const fixed = new RealDate(); fixed.setHours(Math.floor(hour), Math.round((hour % 1) * 60), 0, 0);
    const t = fixed.getTime();
    globalThis.Date = class extends RealDate {
      constructor(...a) { super(...(a.length ? a : [t])); }
      static now() { return t; }
    };
  }, theme, hour);
  await page.goto(base + path, { waitUntil: 'networkidle0' });
  await page.evaluate(() => document.fonts.ready);
  await new Promise((r) => setTimeout(r, 1200)); // mermaid renders client-side
  page._vh = vp.h;
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
    if (!el || el.closest('.sr-only, .sl-skip-link, script, style, noscript, .zoen-sky, dialog:not([open])') || !visible(el)) continue;
    // aria-hidden text is still painted (the "Search" label, aside titles), so it is measured too.
    const range = document.createRange(); range.selectNodeContents(n);
    for (const r of range.getClientRects()) {
      if (r.width < 2 || r.height < 4 || r.bottom <= 0 || r.top >= vh || r.right <= 0 || r.left >= vw) continue;
      // Only text that is actually painted on top at this point (not covered, clipped or off-screen).
      const hit = document.elementFromPoint(Math.min(vw - 1, Math.max(0, (r.left + r.right) / 2)), Math.min(vh - 1, Math.max(0, (r.top + r.bottom) / 2)));
      // Accept the text's own element, or an overlay from the same control (link cards stretch their
      // <a>::before over the whole card, so the hit is a sibling inside the same card).
      const near = (a, b) => { for (let k = 0, x = el; k < 4 && x; k++, x = x.parentElement) if (x.contains(hit)) return true; return false; };
      if (!hit || !(hit === el || el.contains(hit) || hit.contains(el) || near())) continue;
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
      const ink = el instanceof SVGElement && s.fill && s.fill.startsWith('rgb') ? s.fill : s.color; // SVG text paints with fill
      const axeIdx = [...new Set([...document.querySelectorAll('[data-axe-incomplete]')].filter((a) => a === el || a.contains(el)).map((a) => a.dataset.axeIncomplete))];
      out.push({ kind: 'text', axeIdx, diagram: !!el.closest('.mermaid'), text: n.textContent.trim().slice(0, 40), color: ink, large: size >= 24 || (bold && size >= 18.66), sel: el.tagName.toLowerCase() + (el.className && typeof el.className === 'string' ? '.' + el.className.split(' ')[0] : ''),
        rect: [Math.max(0, l), Math.max(0, t), Math.min(vw, rr), Math.min(vh, b)] });
    }
  }
  for (const svg of document.querySelectorAll('header svg, .sidebar-pane svg, .hero svg, .sl-link-button svg')) {
    if (svg.closest('[aria-hidden="true"]:not(svg)') && !svg.closest('a, button')) continue;
    if (svg.closest('.mermaid')) continue;
    const r = svg.getBoundingClientRect();
    if (r.width < 4 || r.bottom <= 0 || r.top >= vh || !visible(svg)) continue;
    out.push({ kind: 'icon', axeIdx: [], text: (svg.closest('a, button, label')?.getAttribute('aria-label') || svg.closest('a, button, label')?.textContent || 'icon').trim().slice(0, 40), color: getComputedStyle(svg).color, large: true, sel: 'svg', rect: [r.left, r.top, r.right, r.bottom] });
  }
  // Form controls paint their value without a text node (the theme picker <select>).
  for (const sel of document.querySelectorAll('select')) {
    const r = sel.getBoundingClientRect();
    if (r.width < 4 || r.bottom <= 0 || r.top >= vh || !visible(sel)) continue;
    const axeIdx = sel.dataset.axeIncomplete ? [sel.dataset.axeIncomplete] : [];
    const s = getComputedStyle(sel);
    out.push({ kind: 'text', axeIdx, diagram: false, text: sel.selectedOptions[0]?.textContent.trim() ?? 'select', color: s.color, large: false, sel: 'select', rect: [r.left, r.top + 2, r.right, r.bottom - 2] });
  }
  // Controls painted over text (Expressive Code's copy button): their pixels belong to the control.
  const overlays = [...document.querySelectorAll('.expressive-code .copy')].map((o) => { const q = o.getBoundingClientRect(); return [q.left, q.top, q.right, q.bottom]; });
  return { out, overlays };
};
const parse = (c) => { const m = c.match(/rgba?\(([^)]+)\)/); if (!m) return null; const p = m[1].split(/[ ,/]+/).filter(Boolean).map(Number); return [p[0], p[1], p[2], p[3] ?? 1]; };

const HIDE = '*, *::before, *::after { color: transparent !important; -webkit-text-fill-color: transparent !important; text-shadow: none !important; } svg:not(.mermaid svg) { visibility: hidden !important; } html:root:root:root:root:root:root body .sl-markdown-content .mermaid svg :is(text, tspan) { fill: transparent !important; stroke: transparent !important; } img { visibility: visible; }';

async function pixelCheck(page, label, measured, frame) {
  const height = await page.evaluate(() => document.documentElement.scrollHeight);
  const vh = page._vh;
  let worst = { r: Infinity };
  let checked = 0;
  const stops = [];
  for (let y = 0; y < Math.min(height, vh * 10); y += vh - 100) stops.push({ y });
  // Then every axe "incomplete" node not yet measured, scrolled into view one by one.
  const pending = async () => (await page.evaluate(() => [...document.querySelectorAll('[data-axe-incomplete]')].map((e) => e.dataset.axeIncomplete))).filter((i) => !measured.has(i));
  for (let k = 0; k < stops.length + 400; k++) {
    let y;
    if (k < stops.length) { y = stops[k].y; await page.evaluate((y) => scrollTo({ top: y, behavior: 'instant' }), y); }
    else {
      const left = (await pending()).filter((i) => !page._tried?.has(i));
      if (!left.length) break;
      (page._tried ??= new Set()).add(left[0]);
      y = await page.evaluate((i) => { const e = document.querySelector(`[data-axe-incomplete="${i}"]`); e.scrollIntoView({ block: 'center', behavior: 'instant' }); return scrollY; }, left[0]);
    }
    await new Promise((r) => setTimeout(r, 120));
    await page.evaluate((t) => ZoenSky.freeze(t), frame);
    const { out: items, overlays } = await page.evaluate(collect);
    const covered = (x, y) => overlays.some(([a, b, c, d]) => x >= a && x < c && y >= b && y < d);
    await page.addStyleTag({ content: HIDE }).then((h) => (page._hide = h));
    await new Promise((r) => setTimeout(r, 80));
    const png = await page.screenshot({ type: 'png' });
    await page.evaluate((el) => el.remove(), page._hide);
    const { data, info } = await sharp(png).raw().toBuffer({ resolveWithObject: true });
    for (const it of items) {
      const fg = parse(it.color);
      if (!fg || fg[3] === 0) continue;
      const [x0, y0, x1, y1] = it.rect.map(Math.round);
      let min = Infinity;
      const all = it.diagram ? [] : null;
      for (let py = y0; py < y1; py += 1) for (let px = x0; px < x1; px += 1) {
        if (px < 0 || py < 0 || px >= info.width || py >= info.height || covered(px, py)) continue;
        const i = (py * info.width + px) * info.channels;
        const bg = [data[i], data[i + 1], data[i + 2]];
        const r = ratio(over(fg, bg), bg);
        if (all) all.push(r); else if (r < min) min = r;
      }
      // Diagram labels are crossed by the diagram's own lines and arrows; judge them against the
      // fill they sit on (10th percentile of the pixels under the text), not a 1px line.
      if (all && all.length) { all.sort((a, b) => a - b); min = all[Math.floor(all.length * 0.1)]; }
      if (!isFinite(min)) continue;
      checked++;
      const need = it.large ? 3 : 4.5;
      it.axeIdx.forEach((i) => measured.add(i));
      if (min < worst.r) worst = { r: min, ...it };
      if (min < need) failures.push(`${label} frame ${frame}s: ${it.kind} "${it.text}" (${it.sel}) ${min.toFixed(2)}:1 < ${need}:1 at scroll ${y}`);
    }
  }
  return { checked, worst };
}

// Runs axe and tags every node it could not decide with data-axe-incomplete, so the pixel check
// must measure it. Returns the incomplete list (id, target, axe's reason).
async function axeCheck(page) {
  await page.evaluate((src) => { if (!window.axe) (0, eval)(src); }, axeSource);
  await page.evaluate(() => scrollTo({ top: 0, behavior: 'instant' }));
  return page.evaluate(async () => {
    const r = await axe.run(document, { runOnly: ['color-contrast'] });
    const incomplete = [];
    r.incomplete.forEach((v) => v.nodes.forEach((n) => {
      const el = document.querySelector(n.target[0]); const id = String(incomplete.length);
      if (el) el.dataset.axeIncomplete = el.dataset.axeIncomplete ?? id;
      incomplete.push({ id: el ? el.dataset.axeIncomplete : null, target: n.target.join(' '), why: n.any[0]?.message ?? '' });
    }));
    return { violations: r.violations.flatMap((v) => v.nodes.map((n) => n.target.join(' ') + ' ' + (n.any[0]?.message ?? ''))), incomplete, passes: r.passes.reduce((a, v) => a + v.nodes.length, 0) };
  });
}

for (const theme of (args.themes ?? 'light,dark').split(',')) for (const [name, hour] of hours) for (const vp of VIEWPORTS) for (const path of pages) {
  if (vp.only && vp.only !== path) continue;
  const label = `${theme} ${name} ${vp.w}x${vp.h} ${path}`;
  const page = await open(path, theme, hour, [], vp);
  const sky = await page.evaluate(() => ({ phase: document.documentElement.dataset.skyPhase, videos: document.querySelectorAll('.zoen-sky video').length, hidden: document.querySelector('.zoen-sky')?.getAttribute('aria-hidden') }));
  if (sky.hidden !== 'true') failures.push(`${label}: sky layer is not aria-hidden`);
  if (!sky.videos) failures.push(`${label}: no sky video playing`);
  await page.evaluate((t) => ZoenSky.freeze(t), FRAMES[0]);
  const axe = await axeCheck(page);
  for (const v of axe.violations) failures.push(`${label}: axe ${v}`);
  // Undecided nodes that are not painted at all (zero-size or display:none at this width) cannot fail.
  const unpainted = new Set(await page.evaluate(() => [...document.querySelectorAll('[data-axe-incomplete]')].filter((e) => !e.getClientRects().length || !e.checkVisibility({ opacityProperty: true, visibilityProperty: true })).map((e) => e.dataset.axeIncomplete)));
  let worst = { r: Infinity }, runs = 0, unmeasured = [];
  // Coverage is tracked per frame: every undecided node must be measured on every frame.
  for (const frame of FRAMES) {
    const measured = new Set();
    page._tried = new Set();
    const px = await pixelCheck(page, label, measured, frame);
    runs += px.checked;
    if (px.worst.r < worst.r) worst = { ...px.worst, frame };
    const miss = axe.incomplete.filter((n) => !n.id || (!measured.has(n.id) && !unpainted.has(n.id)));
    for (const n of miss) failures.push(`${label} frame ${frame}s: axe could not decide ${n.target} (${n.why}) and the pixel check could not measure it`);
    if (miss.length > unmeasured.length) unmeasured = miss;
  }
  report.push({ theme, time: name, hour, viewport: `${vp.w}x${vp.h}`, path, phase: sky.phase, frames: FRAMES, textRuns: runs, worst: +worst.r.toFixed(2), worstAt: `${worst.kind} "${worst.text}" frame ${worst.frame}s (${worst.large ? 'large/UI, 3:1' : 'body, 4.5:1'})`, axeViolations: axe.violations.length, axePasses: axe.passes, axeIncomplete: axe.incomplete.length, axeIncompleteMeasured: axe.incomplete.length - unmeasured.length });
  console.log(`${label.padEnd(54)} ${sky.phase.padEnd(7)} runs ${String(runs).padStart(5)}  worst ${worst.r.toFixed(2)}:1 ${worst.kind} "${worst.text}" @${worst.frame}s  axe ${axe.violations.length} violations / ${axe.passes} passes / ${axe.incomplete.length - unmeasured.length} of ${axe.incomplete.length} undecided measured by pixels`);
  await page.close();
}

// Preference checks.
{
  const p = await open('/', 'light', 7.75, [{ name: 'prefers-reduced-motion', value: 'reduce' }]); // mid crossfade
  const t = await p.evaluate(() => ({ held: (() => {
      const snap = () => [...document.querySelectorAll('.zoen-sky-layer')].map((l) => l.style.opacity).join(); const o = snap();
      const t2 = Date.now() + 5 * 60000, D = Date; globalThis.Date = class extends D { constructor(...a) { super(...(a.length ? a : [t2])); } static now() { return t2; } };
      document.dispatchEvent(new Event('visibilitychange')); return snap() === o; })(), videos: document.querySelectorAll('.zoen-sky video').length, layers: document.querySelectorAll('.zoen-sky-layer').length, transition: getComputedStyle(document.querySelector('.zoen-sky-layer')).transitionDuration }));
  if (t.videos || !t.layers || t.transition !== '0s' || !t.held) failures.push(`prefers-reduced-motion: expected a still photo, got ${JSON.stringify(t)}`);
  console.log(`prefers-reduced-motion: reduce → ${t.videos} videos, ${t.layers} still photo layer(s), transition ${t.transition}, holds still over time: ${t.held}`);
  await p.close();
  // The table of contents must stay fixed while the page scrolls (no filtered ancestor).
  const f = await open('/technical/architecture/', 'dark', 12);
  const toc = await f.evaluate(async () => {
    const el = document.querySelector('.right-sidebar starlight-toc a'); const a = el.getBoundingClientRect().top;
    scrollTo({ top: 1500, behavior: 'instant' }); await new Promise((r) => setTimeout(r, 100));
    return { before: a, after: el.getBoundingClientRect().top };
  });
  if (Math.abs(toc.before - toc.after) > 1) failures.push(`table of contents moved while scrolling: ${JSON.stringify(toc)}`);
  console.log(`table of contents while scrolling: top ${toc.before.toFixed(0)} → ${toc.after.toFixed(0)} px`);
  await f.close();
  // Scrolled sidebars on a short window: their surface must scroll with them (no sky under text).
  for (const theme of ['light', 'dark']) {
    const g = await open('/technical/architecture/', theme, 18, [], { w: 1440, h: 480 });
    await g.evaluate(() => document.querySelectorAll('.sidebar-pane, .right-sidebar').forEach((e) => (e.scrollTop = e.scrollHeight)));
    const px = await pixelCheck(g, `${theme} sunset 1440x480 scrolled sidebars`, new Set(), 3);
    console.log(`${theme} 1440x480, sidebars scrolled to the end: runs ${px.checked}, worst ${px.worst.r.toFixed(2)}:1`);
    await g.close();
  }
  const q = await open('/', 'light', 12, [{ name: 'prefers-contrast', value: 'more' }]);
  const d = await q.evaluate(() => getComputedStyle(document.querySelector('.zoen-sky')).display);
  if (d !== 'none') failures.push('prefers-contrast: more → sky still visible');
  const qv = await q.evaluate(() => document.querySelectorAll('.zoen-sky video').length);
  if (qv) failures.push(`prefers-contrast: more → ${qv} sky video(s) still loaded`);
  await q.close();
  // Pause control (WCAG 2.2.2): pausing removes the video, keeps the photo, and persists.
  const z = await open('/', 'light', 7.75); // mid crossfade, so a minute later would normally differ
  const pz = await z.evaluate(async () => {
    const b = document.querySelector('.zoen-sky-toggle'); const before = document.querySelectorAll('.zoen-sky video').length;
    b.click(); const after = document.querySelectorAll('.zoen-sky video').length;
    // While paused, a later minute must not change the picture.
    const snap = () => [...document.querySelectorAll('.zoen-sky-layer')].map((l) => l.style.opacity).join();
    const o1 = snap();
    const t2 = Date.now() + 5 * 60000, D = Date; globalThis.Date = class extends D { constructor(...a) { super(...(a.length ? a : [t2])); } static now() { return t2; } };
    document.dispatchEvent(new Event('visibilitychange')); const frozen = snap() === o1;
    return { label: b.textContent, pressed: b.getAttribute('aria-pressed'), before, after, stored: localStorage.getItem('zoen-sky-paused'), frozen };
  });
  if (!(pz.before > 0 && pz.after === 0 && pz.pressed === 'true' && pz.stored === '1' && pz.frozen)) failures.push(`pause control: ${JSON.stringify(pz)}`);
  console.log(`pause control: ${pz.before} → ${pz.after} videos, "${pz.label}", aria-pressed ${pz.pressed}`);
  await z.close();
  const q2 = await open('/', 'light', 12, [{ name: 'prefers-contrast', value: 'more' }]);
  // The tint must sit above the footage.
  const zt = await q2.evaluate(() => getComputedStyle(document.querySelector('.zoen-sky'), '::after').zIndex);
  if (zt !== '3') failures.push(`sky tint z-index is ${zt}, expected 3 (above both clips)`);
  console.log(`prefers-contrast: more → sky display ${d}`);
  await q2.close();
}

await browser.close();
try { process.kill(-preview.pid); } catch {}
mkdirSync('contrast-report', { recursive: true });
writeFileSync('contrast-report/report.json', JSON.stringify({ hours: hours.map(([n, h]) => ({ n, h })), report, failures }, null, 2));
if (failures.length) { console.error(`\n${failures.length} contrast failure(s):\n` + failures.slice(0, 60).join('\n')); process.exit(1); }
console.log(`\nPASS: ${report.length} page/theme/time/viewport combinations × ${FRAMES.length} video frames meet WCAG 2.2 AA (text 4.5:1, large text and UI 3:1).`);
