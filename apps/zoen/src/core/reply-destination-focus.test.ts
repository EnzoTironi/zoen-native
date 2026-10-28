import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const file = new URL('./use-reply-destination-focus.ts', import.meta.url);
const compiled = transformSync(readFileSync(file, 'utf8'), {
  filename: file.pathname,
  babelrc: false,
  configFile: false,
  presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
  caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
}).code;
test('destination focus waits for navigation and draft, cancels on blur, and consumes only once', async () => {
  const root = createReactTestRoot();
  const frames = new Map<number, () => void>();
  let next = 0,
    focuses = 0,
    focused = false;
  const module = { exports: {} as { useReplyDestinationFocus: Function } };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    requestAnimationFrame: (work: () => void) => {
      frames.set(++next, work);
      return next;
    },
    cancelAnimationFrame: (id: number) => frames.delete(id),
    require(name: string) {
      if (name === 'react') return React;
      if (name === 'react/compiler-runtime') return require(name);
      if (name === 'expo-router')
        return {
          // Navigation is the port. This test runs the actual compiled focus hook.
          useFocusEffect: (work: () => (() => void) | undefined) => {
            const active = focused;
            React.useEffect(() => (active ? work() : undefined), [work, active]);
          },
        };
      throw new Error(name);
    },
  });
  const input = { current: { focus: () => focuses++ } };
  function Probe({ token = 'draft-a', ready = true }) {
    module.exports.useReplyDestinationFocus(token, ready, input);
    return null;
  }
  try {
    await root.render(React.createElement(Probe));
    assert.equal(frames.size, 0);
    focused = true;
    await root.render(React.createElement(Probe, { ready: false }));
    assert.equal(frames.size, 0);
    await root.render(React.createElement(Probe));
    assert.equal(frames.size, 1);
    focused = false;
    await root.render(React.createElement(Probe));
    assert.equal(frames.size, 0);
    assert.equal(focuses, 0);
    focused = true;
    await root.render(React.createElement(Probe));
    for (const work of frames.values()) work();
    frames.clear();
    assert.equal(focuses, 1);
    focused = false;
    await root.render(React.createElement(Probe));
    focused = true;
    await root.render(React.createElement(Probe));
    assert.equal(frames.size, 0);
    await root.render(React.createElement(Probe, { token: 'draft-b' }));
    assert.equal(frames.size, 1);
  } finally {
    await root.close();
  }
});
