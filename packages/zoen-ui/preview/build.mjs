import { build, context } from 'esbuild';
import { mkdir, copyFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const out = join(here, 'dist');
await mkdir(out, { recursive: true });
await copyFile(join(here, 'index.html'), join(out, 'index.html'));
await copyFile(join(here, 'focus.html'), join(out, 'focus.html'));
const options = {
  entryPoints: {
    main: join(here, 'main.tsx'),
    focus: join(here, 'focus-main.tsx'),
  },
  outdir: out,
  bundle: true,
  format: 'esm',
  platform: 'browser',
  target: ['es2022'],
  jsx: 'automatic',
  sourcemap: true,
  metafile: true,
  logLevel: 'info',
};
if (process.argv.includes('--serve')) {
  const preview = await context(options);
  await preview.watch();
  const server = await preview.serve({ servedir: out, host: '127.0.0.1', port: 4173 });
  console.info(`Local shell preview: http://127.0.0.1:${server.port}`);
  const stop = async () => {
    await preview.dispose();
    process.exit(0);
  };
  process.on('SIGINT', stop);
  process.on('SIGTERM', stop);
} else {
  const result = await build(options);
  if (Object.keys(result.metafile.inputs).some((path) => path.includes('zoen-miniapp-sdk'))) {
    throw new Error('The independent shell preview must not bundle the mini-app SDK.');
  }
}
