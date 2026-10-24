import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import type { PageBlockDto } from '../core/generated/PageBlockDto.ts';
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const path = new URL('./page-blocks.tsx', import.meta.url);
const compiled = transformSync(readFileSync(path, 'utf8'), {
  filename: path.pathname,
  configFile: false,
  babelrc: false,
  presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
  plugins: [['babel-plugin-react-compiler', { target: '19' }]],
  caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
}).code;
type Node = { type: string; props: Record<string, any> };
function nodes(value: any): Node[] {
  return Array.isArray(value)
    ? value.flatMap(nodes)
    : value?.props
      ? [value, ...nodes(value.props.children)]
      : [];
}
function block(kind = 'paragraph'): PageBlockDto {
  return {
    id: 'original',
    kind,
    text: 'Keep 👩‍💻',
    spans: [{ start: 0, end: 4, key: 'future', value: 'opaque' }],
    level: 0,
    indent: 0,
    number: 1,
    checked: false,
    lang: '',
    url: '',
    alt: '',
  };
}
async function render(options: Record<string, unknown> = {}) {
  const duplicates: PageBlockDto[] = [];
  const module = { exports: {} as any };
  const jsx = (type: string, props: Record<string, any>) => ({ type, props });
  const duplicate = await import('./page-duplicate.ts');
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require: (name: string) => {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      if (name === 'react/compiler-runtime')
        return { c: (size: number) => Array(size).fill(Symbol.for('react.memo_cache_sentinel')) };
      if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx };
      if (name === 'react-native') return { View: 'View', Text: 'Text', Pressable: 'Pressable' };
      if (name === './page-image-block') return { PageImageBlock: 'PageImageBlock' };
      if (name === './page-image-items')
        return { usePageImageItems: () => ({ items: [], current: () => true }) };
      if (name === './page-duplicate') return duplicate;
      if (name === '../components/primitives') return { Icon: 'Icon', IconButton: 'IconButton' };
      if (name === '../components/rich-text') return { RichText: 'RichText' };
      if (name === '../theme') return { useTheme: () => ({ colors: {} }) };
      if (name === '../i18n/index.ts')
        return {
          t: (key: string, values?: { position: number }) => `${key}:${values?.position ?? ''}`,
        };
      throw Error(name);
    },
  });
  const selected = options.block ?? block(String(options.kind ?? 'paragraph'));
  const tree = module.exports.PageBlocks({
    blocks: [selected],
    editable: true,
    organizing: true,
    onDuplicate: (value: PageBlockDto) => duplicates.push(value),
    ...options,
  });
  return {
    selected,
    duplicates,
    button: nodes(tree).find((node) => node.props.label === 'documents.duplicateBlockPosition:1'),
  };
}

test('organizing a supported Page block exposes a real Duplicate action without triggering it on render', async () => {
  const f = await render();
  assert.ok(f.button, 'missing Duplicate block control');
  assert.equal(f.button.props.disabled, false);
  assert.deepEqual(f.duplicates, []);
  f.button.props.onPress();
  assert.equal(f.duplicates.length, 1);
  assert.equal(f.duplicates[0], f.selected);
});

test('duplicate control is absent for historical/read-only/ordinary reading and unprojected kinds; busy refuses input', async () => {
  for (const options of [
    { historical: true },
    { editable: false },
    { organizing: false },
    { kind: 'image' },
    { kind: 'heading' },
    { kind: 'future' },
  ])
    assert.equal((await render(options)).button, undefined);
  const f = await render({ busy: true });
  assert.equal(f.button!.props.disabled, true);
  f.button!.props.onPress();
  assert.deepEqual(f.duplicates, []);
});

test('complete heading/image projections expose the existing accessible Duplicate control', async () => {
  for (const metadata of [
    { kind: 'heading', level: 1, setext: true },
    { kind: 'heading', level: 6, setext: false },
    { kind: 'image', title: 'Title' },
    { kind: 'image', title: '' },
  ]) {
    const source = { ...block(), ...metadata };
    const f = await render({ block: source });
    assert.ok(f.button);
    f.button.props.onPress();
    assert.deepEqual(f.duplicates, [source]);
    for (const options of [{ historical: true }, { editable: false }, { organizing: false }])
      assert.equal((await render({ block: source, ...options })).button, undefined);
  }
});
