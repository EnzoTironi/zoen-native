import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { messageBubbleGeometry } from '../components/message-graphics-contract.ts';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const sourcePath = fileURLToPath(
  new URL('../components/message-graphics.ios.tsx', import.meta.url),
);
const compiled = transformSync(
  readFileSync(process.env.ZOEN_MESSAGE_GRAPHICS_IOS_SOURCE ?? sourcePath, 'utf8'),
  {
    filename: sourcePath,
    configFile: false,
    babelrc: false,
    presets: [require.resolve('babel-preset-expo')],
    caller: {
      name: 'metro',
      platform: 'ios',
      engine: 'hermes',
      isDev: false,
      supportsReactCompiler: true,
    },
  },
).code;
assert.match(compiled, /react\/compiler-runtime/, 'Exercise the actual iOS compiler output');
const ports = new Set(['expo', 'react/compiler-runtime', 'react/jsx-runtime']);
const output = await build({
  stdin: { contents: compiled, resolveDir: dirname(sourcePath), loader: 'js' },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'message-graphics-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          ports.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});
type Element = { type: unknown; props: Record<string, unknown> };
const nativeViews: string[][] = [];
const makeElement = (type: unknown, props: Record<string, unknown>) => ({ type, props });
const modules: Record<string, unknown> = {
  expo: {
    requireNativeView: (...args: string[]) => {
      nativeViews.push(args);
      return `Native:${args[1]}`;
    },
  },
  'react/compiler-runtime': {
    c: (size: number) => Array(size).fill(Symbol.for('react.memo_cache_sentinel')),
  },
  'react/jsx-runtime': { jsx: makeElement, jsxs: makeElement },
};
const module = {
  exports: {} as {
    MessageBubbleGraphic(props: Record<string, unknown>): Element;
    MessageReceiptGraphic(props: Record<string, unknown>): Element;
  },
};
runInNewContext(output.outputFiles[0]!.text, {
  module,
  exports: module.exports,
  require: (name: string) => {
    assert.ok(Object.hasOwn(modules, name), name);
    return modules[name];
  },
});

test('iOS graphics bind explicit named views and never replace the default page-input view', () => {
  assert.deepEqual(nativeViews, [
    ['ZoenCore', 'MessageBubble'],
    ['ZoenCore', 'MessageReceipt'],
  ]);
  for (const shape of ['clock', 'single', 'double']) {
    const tree = module.exports.MessageReceiptGraphic({
      ownerKey: 'message_1',
      shape,
      color: '#123456',
    });
    assert.equal(tree.type, 'Native:MessageReceipt');
    assert.equal(tree.props.ownerKey, 'message_1');
    assert.equal(tree.props.shape, shape);
    assert.equal(tree.props.color, '#123456');
    assert.equal(tree.props.pointerEvents, 'none');
    assert.equal(tree.props.accessible, false);
    assert.equal(tree.props.accessibilityElementsHidden, true);
    assert.equal((tree.props.style as Record<string, unknown>).width, 13);
    assert.equal(Object.hasOwn(tree.props, 'onPress'), false);
  }
});

test('actual iOS adapter maps visual above/below continuity to the exact side and tail contract', () => {
  const expected = [
    [false, false, [16, 16, 16, 16], true],
    [true, false, [16, 8, 16, 16], true],
    [false, true, [16, 16, 16, 8], false],
    [true, true, [16, 8, 16, 8], false],
  ] as const;
  for (const incoming of [false, true])
    for (const [above, below, outgoing, tail] of expected) {
      const tree = module.exports.MessageBubbleGraphic({
        ownerKey: 'current_message',
        incoming,
        above,
        below,
        fill: '#203040',
      });
      assert.equal(tree.type, 'Native:MessageBubble');
      const corners = [
        tree.props.topLeft,
        tree.props.topRight,
        tree.props.bottomLeft,
        tree.props.bottomRight,
      ];
      const expectedCorners = incoming
        ? [outgoing[1], outgoing[0], outgoing[3], outgoing[2]]
        : [...outgoing];
      assert.deepEqual(corners, expectedCorners);
      assert.equal(tree.props.tail, tail);
      assert.equal(tree.props.incoming, incoming);
      assert.equal(tree.props.ownerKey, 'current_message');
      assert.equal(tree.props.fill, '#203040');
      assert.equal(tree.props.pointerEvents, 'none');
      assert.equal(tree.props.accessible, false);
      assert.equal(
        tree.props.animateChanges,
        false,
        'No unrelated animation without the admitted parent pass',
      );
    }
});

test('pure visual projection mirrors corner geometry without changing message or delivery authority', () => {
  const input = Object.freeze({ incoming: false, above: true, below: false });
  assert.deepEqual(messageBubbleGeometry(input), {
    topLeft: 16,
    topRight: 8,
    bottomLeft: 16,
    bottomRight: 16,
    tail: true,
  });
  assert.deepEqual(input, { incoming: false, above: true, below: false });
  const enabled = module.exports.MessageBubbleGraphic({
    ownerKey: 'm',
    ...input,
    fill: '#FFFFFF',
    animateChanges: true,
  });
  assert.equal(enabled.props.animateChanges, true);
});
