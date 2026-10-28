import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after } from 'node:test';
import { runInNewContext } from 'node:vm';
import { fileURLToPath } from 'node:url';
import { preparePackage } from './message-markdown-native.fixtures.ts';
import { require, transformSync } from './attachment-composer-ui.fixtures.ts';

const app = fileURLToPath(new URL('../../', import.meta.url));
const directory = mkdtempSync(join(tmpdir(), 'zoen-composer-rich-event-'));
after(() => rmSync(directory, { recursive: true, force: true }));
const pkg = preparePackage(app, directory);
const adapterPath = join(app, 'src/components/composer-rich-input.tsx');
const adapter = transformSync(readFileSync(adapterPath, 'utf8'), {
  filename: adapterPath,
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
}).code;
const facade = transformSync(
  readFileSync(join(pkg, 'lib/module/EnrichedMarkdownTextInput.js'), 'utf8'),
  {
    filename: 'EnrichedMarkdownTextInput.js',
    configFile: false,
    babelrc: false,
    ast: true,
  },
) as unknown as { ast: unknown };
const callbacks: unknown[] = [];
require('@babel/traverse').default(facade.ast, {
  VariableDeclarator(path: { node: { id: { name?: string }; init: { arguments?: unknown[] } } }) {
    if (path.node.id.name === 'handleChangeMarkdown') callbacks.push(path.node.init.arguments?.[0]);
  },
});
assert.equal(
  callbacks.length,
  1,
  'the actual package entry must expose one native Markdown callback',
);
const callback = require('@babel/generator').default(callbacks[0]).code;

/** Real Expo/ReactCompiler adapter and actual packaged ENRM event callback.
 * Fabric dispatch/UIKit are ports; this is not a native interaction acceptance. */
export function richEventAdapter(props: Record<string, any>) {
  const nativeFacade = { focus() {}, blur() {}, getNativeRef: () => ({ nativeTag: 101 }) };
  const module = { exports: {} as Record<string, any> };
  const jsx = (type: unknown, props: Record<string, any>) => ({ type, props });
  const ports: Record<string, any> = {
    react: {
      useRef: () => ({ current: nativeFacade }),
      useImperativeHandle: (ref: ((node: unknown) => void) | undefined, read: () => unknown) =>
        ref?.(read()),
    },
    'react/compiler-runtime': {
      c: (size: number) => Array(size).fill(Symbol.for('react.memo_cache_sentinel')),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native-enriched-markdown': { EnrichedMarkdownTextInput: 'ActualENRMFacade' },
    'react-native': {
      StyleSheet: { flatten: (value: unknown) => value },
      useWindowDimensions: () => ({ fontScale: 1 }),
    },
    '../i18n': { t: (key: string) => key },
  };
  runInNewContext(adapter, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  const rendered = module.exports.ComposerRichInput(props);
  assert.equal(rendered.type, 'ActualENRMFacade');
  let receipt = { binding: props.composerBinding, eventCount: 0 };
  const change = runInNewContext(`(${callback})`, {
    composerBinding: props.composerBinding,
    onChangeMarkdown: undefined,
    onComposerChange: rendered.props.onComposerChange,
    setComposerReceipt: (update: (previous: typeof receipt) => typeof receipt) => {
      receipt = update(receipt);
    },
  });
  return { rendered, change, receipt: () => receipt };
}
