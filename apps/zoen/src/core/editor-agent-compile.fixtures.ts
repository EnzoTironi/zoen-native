import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
const require = createRequire(import.meta.url),
  { transformSync } = require('@babel/core');
function compile(path: string) {
  const file = new URL(path, import.meta.url);
  const output = transformSync(readFileSync(file, 'utf8'), {
    filename: file.pathname,
    babelrc: false,
    configFile: false,
    presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
    plugins: [['babel-plugin-react-compiler', { target: '19' }]],
    caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
  }).code as string;
  if (path.endsWith('/editor-agent-sheet.tsx') || path.endsWith('/detail-screen.tsx'))
    assert.match(output, /react\/compiler-runtime/);
  return output;
}
export function loadEditorAgent(path: string, ports: Record<string, any>) {
  const module = { exports: {} as any };
  runInNewContext(compile(path), {
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: 'ios', NODE_ENV: 'production' } },
    setTimeout,
    clearTimeout,
    require: (name: string) => {
      if (name.startsWith('@babel/runtime/')) return require(name);
      const value = ports[name];
      if (!value) throw Error(`Unmocked ${name}`);
      return value;
    },
  });
  return module.exports;
}
