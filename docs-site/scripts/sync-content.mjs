#!/usr/bin/env node
// Builds the site's pages from the repository's Markdown, so docs/ stays the single source.
//
//   docs/product/*.md  docs/guides/*.md  docs/technical/*.md  docs/reference/*.md
//     → src/content/docs/<section>/<name>.md   (generated, git-ignored)
//
// - The first "# Heading" becomes the page title (frontmatter); the first paragraph its description.
// - Links to synced pages become site routes; images under docs/ are copied to public/media/.
// - Links to any other repository file become GitHub links, so nothing points at a dead path.
import { readFileSync, writeFileSync, mkdirSync, rmSync, readdirSync, existsSync, statSync, copyFileSync } from 'node:fs';
import { dirname, join, relative, resolve, extname, basename, posix } from 'node:path';
import { fileURLToPath } from 'node:url';

const site = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repo = resolve(site, '..');
const GITHUB = 'https://github.com/EnzoTironi/zoen-native';
const SECTIONS = ['product', 'guides', 'technical', 'reference'];
const outRoot = join(site, 'src/content/docs');
const media = join(site, 'public/media');
const IMAGE = new Set(['.png', '.jpg', '.jpeg', '.gif', '.webp', '.svg']);

const KEEP_ON_GITHUB = new Set([
  'docs/product/vision.md', 'docs/product/convites.md', 'docs/product/metricas.md',
  'docs/product/ux-audit.md', 'docs/product/hig-compliance.md', 'docs/product/live-pages.md',
]);

const pages = new Map(); // absolute md path → route
for (const s of SECTIONS) {
  const dir = join(repo, 'docs', s);
  for (const f of readdirSync(dir)) {
    if (extname(f) !== '.md') continue;
    const file = join(dir, f);
    if (!isSynced(file)) continue;
    pages.set(file, `/${s}/${basename(f, '.md')}/`);
  }
}

// Only the structured docs are synced; older notes in docs/product stay on GitHub.
function isSynced(file) {
  const first = readFileSync(file, 'utf8').split('\n', 1)[0];
  return first.startsWith('# ') && !KEEP_ON_GITHUB.has(relative(repo, file));
}

for (const s of SECTIONS) rmSync(join(outRoot, s), { recursive: true, force: true });
rmSync(media, { recursive: true, force: true });

function rewrite(target, fromFile) {
  if (/^(https?:|mailto:|#|\/)/.test(target)) return target;
  const [path, anchor] = target.split('#');
  const abs = resolve(dirname(fromFile), decodeURI(path));
  const hash = anchor ? `#${anchor}` : '';
  if (pages.has(abs)) return pages.get(abs) + hash;
  const rel = relative(repo, abs).split('\\').join('/');
  if (rel.startsWith('..')) throw new Error(`${relative(repo, fromFile)}: link leaves the repository: ${target}`);
  if (!existsSync(abs)) throw new Error(`${relative(repo, fromFile)}: broken link: ${target}`);
  if (IMAGE.has(extname(abs).toLowerCase()) && rel.startsWith('docs/')) {
    const dst = join(media, rel.slice('docs/'.length));
    mkdirSync(dirname(dst), { recursive: true });
    copyFileSync(abs, dst);
    return posix.join('/media', rel.slice('docs/'.length));
  }
  const kind = statSync(abs).isDirectory() ? 'tree' : 'blob';
  return `${GITHUB}/${kind}/main/${rel}${hash}`;
}

const plain = (md) => md.replace(/!\[[^\]]*\]\([^)]*\)/g, '').replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
  .replace(/[*_`>]/g, '').replace(/\s+/g, ' ').trim();

let count = 0;
for (const [file, route] of pages) {
  let src = readFileSync(file, 'utf8');
  const m = src.match(/^# (.+)\n/);
  const title = m[1].trim();
  src = src.slice(m[0].length).replace(/^\s*\n/, '');
  const firstPara = src.split(/\n\s*\n/).find((p) => p.trim() && !p.startsWith('```') && !p.startsWith('|') && !p.startsWith('#')) ?? '';
  const description = plain(firstPara).slice(0, 220);
  // Markdown links/images, then HTML src/href attributes. Code fences are left alone.
  const parts = src.split(/(```[\s\S]*?```)/g);
  src = parts.map((chunk, i) => (i % 2 ? chunk : chunk
    .replace(/(!?\[[^\]]*\]\()([^)\s]+)(\))/g, (_, a, t, b) => a + rewrite(t, file) + b)
    .replace(/\b(src|href)="([^"]+)"/g, (_, attr, t) => `${attr}="${rewrite(t, file)}"`))).join('');
  const fm = `---\ntitle: ${JSON.stringify(title)}\ndescription: ${JSON.stringify(description)}\n` +
    `editUrl: ${JSON.stringify(`${GITHUB}/edit/main/${relative(repo, file)}`)}\n---\n\n`;
  const out = join(outRoot, route.slice(1, -1) + '.md');
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, fm + src);
  count++;
}
// Images the landing page uses directly.
// Product shots are iPhone captures (docs/screens/iphone, framed), the app's best-looking version.
const phones = readdirSync(join(repo, 'docs/screens/iphone')).filter((f) => IMAGE.has(extname(f))).map((f) => `screens/iphone/${f}`);
for (const img of phones) {
  const dst = join(media, img);
  mkdirSync(dirname(dst), { recursive: true });
  copyFileSync(join(repo, 'docs', img), dst);
}
console.log(`synced ${count} pages from docs/`);
