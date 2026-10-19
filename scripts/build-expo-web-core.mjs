#!/usr/bin/env node
import { build } from '../apps/zoen/node_modules/esbuild/lib/main.js';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cp, mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const app = join(root, 'apps/zoen');
const generated = join(app, 'modules/zoen-core/generated/web');
const output = join(app, 'public');
const profile = process.argv.includes('--release') ? 'release' : 'dev';
const folder = profile === 'dev' ? 'debug' : 'release';
const target = process.env.CARGO_TARGET_DIR ?? join(root, 'target');
const env = {
  ...process.env,
  PATH: `${process.env.HOME}/.cargo/bin:${process.env.PATH}`,
  CARGO_BUILD_JOBS: '2',
  CARGO_PROFILE_DEV_DEBUG: '0',
  CARGO_INCREMENTAL: '0',
};
function run(command, args) {
  return new Promise((ok, fail) => {
    const child = spawn(command, args, { cwd: root, env, stdio: 'inherit' });
    child.once('error', fail);
    child.once('exit', (code) =>
      code === 0 ? ok() : fail(new Error(`${command} exited ${code}`)),
    );
  });
}
await mkdir(generated, { recursive: true });
await mkdir(join(output, 'core'), { recursive: true });
await run('cargo', [
  'build',
  '--locked',
  '-p',
  'zoen-web-core',
  '--target',
  'wasm32-unknown-unknown',
  '--profile',
  profile,
]);
const bindgenArgs = [
  join(target, `wasm32-unknown-unknown/${folder}/zoen_web_core.wasm`),
  '--target',
  'web',
  '--out-dir',
  generated,
  '--out-name',
  'zoen_web_core',
];
// Keep development stack names; production does not need the large debug table.
if (profile === 'release') bindgenArgs.push('--remove-name-section');
await run('wasm-bindgen', bindgenArgs);
await cp(join(generated, 'zoen_web_core_bg.wasm'), join(output, 'core/zoen_web_core_bg.wasm'));
const integrity = async (path) =>
  `sha384-${createHash('sha384')
    .update(await readFile(path))
    .digest('base64')}`;
await build({
  absWorkingDir: root,
  bundle: true,
  format: 'esm',
  platform: 'browser',
  target: ['es2022'],
  entryPoints: [join(root, 'packages/zoen-ui/web/worker.ts')],
  outfile: join(output, 'worker.js'),
  define: {
    ZOEN_WASM_INTEGRITY: JSON.stringify(
      await integrity(join(output, 'core/zoen_web_core_bg.wasm')),
    ),
  },
  plugins: [
    {
      name: 'zoen-wasm',
      setup(plugin) {
        plugin.onResolve({ filter: /\/core\/zoen_web_core\.js$/ }, () => ({
          path: join(generated, 'zoen_web_core.js'),
        }));
      },
    },
  ],
});
await writeFile(
  join(app, 'src/core/generated/web-assets.json'),
  `${JSON.stringify({ workerIntegrity: await integrity(join(output, 'worker.js')) }, null, 2)}\n`,
);
