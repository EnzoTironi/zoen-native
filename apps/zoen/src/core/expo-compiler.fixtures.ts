import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');

/** The optimized native pipeline, usable directly in a VM or by an esbuild adapter. */
export function compileExpoSource(file: string | URL, esModules = false): string {
  const filename = typeof file === 'string' ? file : fileURLToPath(file);
  const code = transformSync(readFileSync(file, 'utf8'), {
    filename,
    babelrc: false,
    configFile: false,
    presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
    plugins: [['babel-plugin-react-compiler', { target: '19' }]],
    caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: esModules },
  }).code as string;
  assert.match(
    code,
    /react\/compiler-runtime/,
    `${filename} must retain real compiler optimization`,
  );
  return code;
}
