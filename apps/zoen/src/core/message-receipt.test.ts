import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { contrastRatio, resolveSkyTheme, SKY_PHASES } from '../theme/sky.ts';
import type { Delivery } from './generated/Delivery';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const sourcePath = fileURLToPath(new URL('../components/message-receipt.tsx', import.meta.url));
const compiled = transformSync(
  readFileSync(process.env.ZOEN_MESSAGE_RECEIPT_SOURCE ?? sourcePath, 'utf8'),
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
const ports = new Set([
  'react/compiler-runtime',
  'react/jsx-runtime',
  'react-native',
  '../i18n/index.ts',
  '../theme',
  './primitives',
]);
const output = await build({
  stdin: { contents: compiled, resolveDir: dirname(sourcePath), loader: 'js' },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'receipt-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          ports.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});
type Element = { type: unknown; props: Record<string, unknown> };
function elements(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(elements);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [
    node,
    ...elements(typeof node.type === 'function' ? node.type(node.props) : node.props.children),
  ];
}
function render(delivery: Delivery, phase: (typeof SKY_PHASES)[number], mine: boolean) {
  const theme = resolveSkyTheme(phase, new Date('2026-10-18T12:00:00'));
  const makeElement = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const modules: Record<string, unknown> = {
    'react/compiler-runtime': {
      c: (size: number) => Array(size).fill(Symbol.for('react.memo_cache_sentinel')),
    },
    'react/jsx-runtime': { jsx: makeElement, jsxs: makeElement },
    'react-native': { View: 'View' },
    '../i18n/index.ts': { t: (key: string) => key },
    '../theme': { useTheme: () => theme },
    './primitives': { Icon: 'Icon' },
  };
  const module = {
    exports: {} as {
      MessageReceipt(props: Record<string, unknown>): Element;
      messageDeliveryLabel(delivery: Delivery): string;
    },
  };
  runInNewContext(output.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(Object.hasOwn(modules, name), `Unexpected receipt native import: ${name}`);
      return modules[name];
    },
  });
  const face = mine ? theme.presentation.chat.outgoing : theme.presentation.chat.incoming;
  const ink = face.text;
  const background = face.fill;
  const backgrounds = face.fillColors;
  return {
    tree: module.exports.MessageReceipt({ delivery, ink, background, backgrounds }),
    ink,
    background,
    backgrounds,
  };
}

test('compiled receipt keeps all six core acknowledgement states and actual accessible labels', () => {
  const expected = [
    ['Local', 'chat.receiptLocal', 'iphone', 1],
    ['Sending', 'chat.receiptSending', 'clock', 1],
    ['Sent', 'chat.receiptSent', 'checkmark', 1],
    ['Delivered', 'chat.receiptDelivered', 'checkmark', 2],
    ['Read', 'chat.receiptRead', 'checkmark', 2],
    ['Failed', 'chat.receiptFailed', 'exclamationmark.circle', 1],
  ] as const;
  for (const [delivery, label, ios, count] of expected) {
    const { tree } = render(delivery, 'day', true);
    assert.equal(tree.props.accessibilityLabel, label);
    const icons = elements(tree).filter((node) => node.type === 'Icon');
    assert.equal(icons.length, count);
    assert.equal((icons[0]!.props.name as { ios: string }).ios, ios);
    assert.equal(icons[0]!.props.size, 11);
    const style = tree.props.style as Record<string, unknown>;
    assert.equal(style.width, 13);
    if (count === 2) {
      const overlap = elements(tree).find(
        (node) =>
          (node.props.style as Record<string, unknown> | undefined)?.position === 'absolute',
      );
      assert.ok(overlap);
      assert.equal((overlap.props.style as Record<string, unknown>).left, -4);
    }
  }
});

test('read acknowledgement stays legible against every actual bubble gradient stop; delivery states never gain read color', () => {
  for (const phase of SKY_PHASES)
    for (const mine of [false, true]) {
      for (const delivery of ['Local', 'Sending', 'Sent', 'Delivered', 'Read', 'Failed'] as const) {
        const { tree, ink, backgrounds } = render(delivery, phase, mine);
        const icons = elements(tree).filter((node) => node.type === 'Icon');
        for (const icon of icons) {
          if (delivery === 'Read') {
            for (const paper of backgrounds)
              assert.ok(
                contrastRatio(icon.props.color as string, paper) >= 3,
                `${phase}/${mine}/${paper}`,
              );
          } else assert.equal(icon.props.color, ink);
        }
      }
    }
});
