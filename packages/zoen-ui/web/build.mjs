import { build } from 'esbuild';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFile, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../../..');
const out = join(here, 'dist');
const generated = join(here, 'generated');

function run(command, args) {
  return new Promise((resolveRun, reject) => {
    const child = spawn(command, args, { cwd: root, stdio: 'inherit', env: process.env });
    child.on('error', reject);
    child.on('exit', (code) => code === 0 ? resolveRun() : reject(new Error(`${command} failed (${code})`)));
  });
}

await mkdir(out, { recursive: true });
await mkdir(join(out, 'core'), { recursive: true });
if (!process.argv.includes('--skip-wasm')) {
  await run('cargo', ['build', '-p', 'zoen-web-core', '--target', 'wasm32-unknown-unknown', '--release', '-j', '1']);
  const target = process.env.CARGO_TARGET_DIR ?? join(root, 'target');
  await run(process.env.ZOEN_WASM_BINDGEN ?? 'wasm-bindgen', [join(target, 'wasm32-unknown-unknown/release/zoen_web_core.wasm'),
    '--target', 'web', '--out-dir', generated, '--out-name', 'zoen_web_core']);
}
await copyFile(join(generated, 'zoen_web_core_bg.wasm'), join(out, 'core/zoen_web_core_bg.wasm'));
const wasmIntegrity = `sha384-${createHash('sha384').update(await readFile(join(out, 'core/zoen_web_core_bg.wasm'))).digest('base64')}`;
const options = { bundle: true, format: 'esm', platform: 'browser', target: ['es2022'], jsx: 'automatic', logLevel: 'info' };
await build({ ...options, entryPoints: [join(here, 'worker.ts')], outfile: join(out, 'worker.js'),
  define: { ZOEN_WASM_INTEGRITY: JSON.stringify(wasmIntegrity) },
  plugins: [{ name: 'generated-core', setup(plugin) {
    plugin.onResolve({ filter: /\/core\/zoen_web_core\.js$/ }, () => ({ path: join(generated, 'zoen_web_core.js') }));
  } }] });
const workerIntegrity = `sha384-${createHash('sha384').update(await readFile(join(out, 'worker.js'))).digest('base64')}`;
await build({ ...options, entryPoints: [join(here, 'main.tsx')], outfile: join(out, 'main.js'),
  define: { ZOEN_WORKER_INTEGRITY: JSON.stringify(workerIntegrity) } });
const integrity = async (file) => `sha384-${createHash('sha384').update(await readFile(join(out, file))).digest('base64')}`;
const html = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Zoen</title><link rel="stylesheet" href="/main.css" integrity="${await integrity('main.css')}" crossorigin="anonymous"></head><body><div id="root"></div><script type="module" src="/main.js" integrity="${await integrity('main.js')}" crossorigin="anonymous"></script></body></html>`;
await writeFile(join(out, 'index.html'), html);
const files = [...(await readdir(out)).filter((file) => /\.(js|css|html)$/.test(file)), 'core/zoen_web_core_bg.wasm'];
const manifest = { format: 'zoen-web-build-v1', files: Object.fromEntries(await Promise.all(files.map(async (file) => [file, await integrity(file)]))) };
await writeFile(join(out, 'build-manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
