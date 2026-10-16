// Build/run the explicitly labelled, in-memory visual fixture. Never serves account data.
import { build } from 'esbuild';
import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { resolve } from 'node:path';
const root = resolve(import.meta.dirname, '../../node_modules/.cache/search-edit-fixture');
await mkdir(root, { recursive: true });
await build({ entryPoints: [resolve(import.meta.dirname, 'search-edit.tsx')], outdir: root, bundle: true, format: 'esm', jsx: 'automatic' });
await writeFile(resolve(root, 'index.html'), '<!doctype html><html lang="pt-BR"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Zoen — demonstração local, não E2E</title><link rel="stylesheet" href="search-edit.css"><style>#root { height:100dvh; display:flex; flex-direction:column } #root > .zm-app { flex:1; min-height:0; height:auto }</style><div id="root"></div><script type="module" src="search-edit.js"></script></html>');
console.log('Fixture built: production React components with in-memory data; not E2E.');
if (process.argv.includes('--serve')) {
  createServer(async (req, res) => {
    const name = req.url === '/' ? 'index.html' : req.url?.slice(1);
    if (!['index.html', 'search-edit.js', 'search-edit.css'].includes(name)) { res.writeHead(404).end(); return; }
    try { const body = await readFile(resolve(root, name)); res.setHeader('Content-Type', name.endsWith('.css') ? 'text/css' : name.endsWith('.js') ? 'text/javascript' : 'text/html'); res.end(body); }
    catch { res.writeHead(404).end(); }
  }).listen(4178, '127.0.0.1', () => console.log('Local fixture: http://127.0.0.1:4178'));
}
