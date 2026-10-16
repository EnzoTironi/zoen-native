import { build } from 'esbuild';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFile, cp, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
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
// Project-owned native artwork: copied byte-for-byte, never redrawn by the capture fixture.
await cp(join(root, 'apple/AvatarV1/Heroes'), join(out, 'assets/avatars/Heroes'), { recursive: true });
await cp(join(root, 'apple/AvatarV1/Loops'), join(out, 'assets/avatars/Loops'), { recursive: true });
await mkdir(join(out, 'assets/avatars/Stills'), { recursive: true });
const avatarManifest = JSON.parse(await readFile(join(root, 'apple/AvatarV1/AvatarV1Manifest.json'), 'utf8'));
for (const asset of avatarManifest.assets) {
  if (!/^[A-Za-z0-9]+$/.test(asset.name)) throw new Error('Invalid native avatar asset name.');
  const imageset = join(root, 'apple/AvatarV1/AvatarV1.xcassets', `${asset.name}.imageset`);
  const contents = JSON.parse(await readFile(join(imageset, 'Contents.json'), 'utf8'));
  const image = contents.images.filter(image => image.filename).sort((a, b) => parseInt(b.scale) - parseInt(a.scale))[0];
  if (!image || image.filename.includes('/') || image.filename.includes('..')) throw new Error('Invalid native avatar still.');
  await copyFile(join(imageset, image.filename), join(out, 'assets/avatars/Stills', `${asset.name}.png`));
}
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
  define: { ZOEN_WORKER_INTEGRITY: JSON.stringify(workerIntegrity), ZOEN_SOURCE_REVISION: JSON.stringify(execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim()) } });
const integrity = async (file) => `sha384-${createHash('sha384').update(await readFile(join(out, file))).digest('base64')}`;
await writeFile(join(out, 'favicon.svg'), '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="18" fill="#191919"/><path d="M20 22h24L20 42h24" fill="none" stroke="#fff" stroke-width="6" stroke-linecap="round" stroke-linejoin="round"/></svg>\n');
const html = `<!doctype html><html lang="pt-BR"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Zoen</title><link rel="icon" href="/favicon.svg" type="image/svg+xml"><link rel="stylesheet" href="/main.css" integrity="${await integrity('main.css')}" crossorigin="anonymous"></head><body><div id="root"></div><script type="module" src="/main.js" integrity="${await integrity('main.js')}" crossorigin="anonymous"></script></body></html>`;
await writeFile(join(out, 'index.html'), html);
const files = [...(await readdir(out)).filter((file) => /\.(js|css|html)$/.test(file)), 'core/zoen_web_core_bg.wasm'];
const manifest = { format: 'zoen-web-build-v1', files: Object.fromEntries(await Promise.all(files.map(async (file) => [file, await integrity(file)]))) };
await writeFile(join(out, 'build-manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
