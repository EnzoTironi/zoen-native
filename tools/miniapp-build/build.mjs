#!/usr/bin/env node
// miniapp-build: one Zoen mini-app → one offline HTML file + a manifest with its sha256.
//
//   node tools/miniapp-build/build.mjs miniapps/hike [--out crates/roda-ffi/apps]
//
// Rules it enforces (a bundle that breaks one does not get written):
// - the manifest is strict (id, name, entry, capabilities with purposes, allowedDomains,
//   linkDomains for outbound links);
// - bare imports must be on the allowlist below (vendored, pinned in package-lock.json);
// - local imports stay inside the app folder or packages/ (the SDK and the UI kit);
// - assets are inlined as data: URLs; no <script src>, no remote stylesheet, no remote
//   URL in the app's own code except the declared allowedDomains;
// - the result fits the size budget (manifest.maxKB, default 400 KB).
// Runs at dev time only. The app never fetches code: the host loads this one file, checks
// its hash against the manifest and injects a CSP that only opens the allowed domains
// after the user agreed.
import { build } from 'esbuild';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, existsSync, mkdirSync, readdirSync, statSync } from 'node:fs';
import { resolve, join, relative, dirname, extname } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../..');
const ALLOW = ['react', 'react-dom', 'react-dom/client', 'react/jsx-runtime', 'react/jsx-dev-runtime', 'scheduler', 'zustand', 'zustand/middleware', 'date-fns', 'maplibre-gl', 'maplibre-gl/dist/maplibre-gl.css'];
const ALLOW_PREFIX = ['date-fns/', 'zustand/'];
const CAPS = new Set(['photos.pick', 'camera.capture', 'location', 'location.approximate', 'calendar.freebusy', 'calendar.events', 'calendar.add', 'contacts.pick', 'health.steps']);
const ALIAS = {
  '@zoen/miniapp-sdk': join(root, 'packages/zoen-miniapp-sdk/src/index.ts'),
  '@zoen/ui': join(root, 'packages/zoen-ui/src/index.tsx'),
  '@zoen/ui/tokens.css': join(root, 'packages/zoen-ui/src/tokens.css'),
};

const fail = (m) => { console.error(`✗ ${m}`); process.exit(1); };

export function validateManifest(m) {
  const errs = [];
  if (!/^[a-z][a-z0-9-]{1,31}$/.test(m.id ?? '')) errs.push('id: lowercase letters, digits, dashes (2–32)');
  if (typeof m.name !== 'string' || !m.name.trim() || m.name.length > 40) errs.push('name: 1–40 chars');
  if (typeof m.entry !== 'string' || !/^[\w./-]+\.(tsx?|jsx?)$/.test(m.entry)) errs.push('entry: a .ts/.tsx/.js/.jsx file in the app folder');
  const caps = m.capabilities ?? [];
  if (!Array.isArray(caps) || caps.length > 8) errs.push('capabilities: array (≤ 8)');
  for (const c of Array.isArray(caps) ? caps : []) {
    if (!CAPS.has(c?.id)) errs.push(`capabilities: unknown "${c?.id}"`);
    if (typeof c?.purpose !== 'string' || c.purpose.trim().length < 8 || c.purpose.length > 160) errs.push(`capabilities.${c?.id}: purpose 8–160 chars (the user reads it)`);
  }
  const doms = m.allowedDomains ?? [];
  if (!Array.isArray(doms) || doms.length > 3) errs.push('allowedDomains: array (≤ 3)');
  for (const d of Array.isArray(doms) ? doms : []) if (!/^(?=.{4,253}$)([a-z0-9-]+\.)+[a-z]{2,}$/.test(d)) errs.push(`allowedDomains: "${d}" must be a bare hostname (no scheme, path or wildcard)`);
  if (doms.length && (typeof m.networkPurpose !== 'string' || m.networkPurpose.length < 8)) errs.push('networkPurpose: required when allowedDomains is set');
  const links = m.linkDomains ?? [];
  if (!Array.isArray(links) || links.length > 6) errs.push('linkDomains: array (≤ 6)');
  for (const d of Array.isArray(links) ? links : []) if (!/^(?=.{4,253}$)([a-z0-9-]+\.)+[a-z]{2,}$/.test(d)) errs.push(`linkDomains: "${d}" must be a bare hostname`);
  if (m.maxKB != null && !(Number.isInteger(m.maxKB) && m.maxKB > 0 && m.maxKB <= 2048)) errs.push('maxKB: integer ≤ 2048');
  const extra = Object.keys(m).filter((k) => !['id', 'name', 'description', 'entry', 'capabilities', 'allowedDomains', 'networkPurpose', 'linkDomains', 'maxKB', 'title'].includes(k));
  if (extra.length) errs.push(`unknown keys: ${extra.join(', ')}`);
  return errs;
}

const guard = (appDir) => ({
  name: 'zoen-allowlist',
  setup(b) {
    b.onResolve({ filter: /^[^./]/ }, (args) => {
      if (ALIAS[args.path]) return { path: ALIAS[args.path] };
      if (args.path.startsWith('@zoen/')) return { errors: [{ text: `Unknown Zoen package ${args.path}` }] };
      // Imports made by vendored packages themselves are theirs to resolve.
      if (args.importer.includes('/node_modules/')) return undefined;
      if (ALLOW.includes(args.path) || ALLOW_PREFIX.some((p) => args.path.startsWith(p))) return undefined;
      return { errors: [{ text: `"${args.path}" is not on the mini-app allowlist (${ALLOW.slice(0, 8).join(', ')}, …)` }] };
    });
    b.onResolve({ filter: /^\.\.?\// }, (args) => {
      if (args.importer.includes('/node_modules/')) return undefined;
      const p = resolve(args.resolveDir, args.path);
      const okRoots = [appDir, join(root, 'packages')];
      if (!okRoots.some((r) => p.startsWith(r))) return { errors: [{ text: `Import escapes the app folder: ${args.path}` }] };
      return undefined;
    });
  },
});

function ownSources(dir) {
  const out = [];
  for (const f of readdirSync(dir)) {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) { if (!['node_modules', 'dist'].includes(f)) out.push(...ownSources(p)); }
    else if (/\.(tsx?|jsx?|css|html)$/.test(f)) out.push(p);
  }
  return out;
}

export async function buildApp(appDir, outDir) {
  appDir = resolve(appDir);
  const mPath = join(appDir, 'manifest.json');
  if (!existsSync(mPath)) fail(`${relative(root, appDir)}: no manifest.json`);
  const manifest = JSON.parse(readFileSync(mPath, 'utf8'));
  const errs = validateManifest(manifest);
  if (errs.length) fail(`manifest:\n  - ${errs.join('\n  - ')}`);

  // Remote URLs in the app's own code must be declared.
  for (const f of ownSources(appDir)) {
    const src = readFileSync(f, 'utf8');
    for (const m of src.matchAll(/\b(?:https?|wss?):\/\/([a-z0-9.-]+)/gi)) {
      const host = m[1].toLowerCase();
      // allowedDomains may be fetched (after consent); linkDomains are only ever opened
      // outside through ui/open-link, which the host confirms.
      if (![...(manifest.allowedDomains ?? []), ...(manifest.linkDomains ?? [])].includes(host) && !['www.w3.org'].includes(host)) fail(`${relative(root, f)}: remote URL to undeclared host "${host}"`);
    }
  }

  const t0 = performance.now();
  const r = await build({
    entryPoints: [join(appDir, manifest.entry)],
    bundle: true, write: false, minify: true, format: 'iife', target: ['safari18'],
    jsx: 'automatic', outdir: 'out', legalComments: 'none', metafile: true, logLevel: 'silent',
    define: { 'process.env.NODE_ENV': '"production"' },
    loader: { '.svg': 'dataurl', '.png': 'dataurl', '.jpg': 'dataurl', '.jpeg': 'dataurl', '.webp': 'dataurl', '.woff2': 'dataurl' },
    nodePaths: [join(here, 'node_modules')],
    plugins: [guard(appDir)],
  }).catch((e) => fail(e.errors?.map((x) => x.text).join('\n') ?? e.message));

  const js = r.outputFiles.find((f) => f.path.endsWith('.js'))?.text ?? '';
  const css = r.outputFiles.find((f) => f.path.endsWith('.css'))?.text ?? '';
  const title = (manifest.title ?? manifest.name).replace(/[<>&"]/g, '');
  // `</script` inside the bundle would end the tag early.
  const safeJs = js.replace(/<\/script/gi, '<\\/script');
  const html = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"><title>${title}</title><style>${css}</style></head><body><div id="root"></div><script>${safeJs}</script></body></html>`;

  if (/<script[^>]+src=/i.test(html.replace(safeJs, ''))) fail('external <script src> in output');
  const bytes = Buffer.byteLength(html);
  const maxKB = manifest.maxKB ?? 400;
  if (bytes > maxKB * 1024) fail(`bundle is ${(bytes / 1024).toFixed(0)} KB > budget ${maxKB} KB`);

  const sha256 = createHash('sha256').update(html).digest('hex');
  const deps = Object.fromEntries(Object.keys(r.metafile.inputs).map((p) => p.match(/node_modules\/((?:@[^/]+\/)?[^/]+)/)?.[1]).filter(Boolean).map((n) => [n, JSON.parse(readFileSync(join(here, 'node_modules', n, 'package.json'), 'utf8')).version]));
  const out = { id: manifest.id, name: manifest.name, description: manifest.description ?? '', capabilities: manifest.capabilities ?? [], allowedDomains: manifest.allowedDomains ?? [], networkPurpose: manifest.networkPurpose ?? '', linkDomains: manifest.linkDomains ?? [], sha256, bytes, deps };
  mkdirSync(outDir, { recursive: true });
  writeFileSync(join(outDir, `${manifest.id}.html`), html);
  writeFileSync(join(outDir, `${manifest.id}.manifest.json`), JSON.stringify(out, null, 2) + '\n');
  console.log(`✓ ${manifest.id}: ${(bytes / 1024).toFixed(1)} KB (gzip ~${(gzipSize(html) / 1024).toFixed(1)} KB) · sha256 ${sha256.slice(0, 12)}… · ${(performance.now() - t0).toFixed(0)} ms · deps ${Object.entries(deps).map(([k, v]) => `${k}@${v}`).join(', ') || 'none'}`);
  return out;
}

import { gzipSync } from 'node:zlib';
function gzipSize(s) { return gzipSync(s).length; }

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const oi = args.indexOf('--out');
  const outDir = resolve(oi >= 0 ? args[oi + 1] : join(root, 'crates/roda-ffi/apps'));
  const apps = args.filter((a, i) => a !== '--out' && (oi < 0 || i !== oi + 1));
  if (!apps.length) fail('usage: build.mjs <app-dir>… [--out dir]');
  for (const a of apps) await buildApp(a, outDir);
}
