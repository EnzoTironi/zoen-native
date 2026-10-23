import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';

const source = await build({
  stdin: {
    contents: readFileSync(new URL('./page-blocks.tsx', import.meta.url), 'utf8'),
    loader: 'tsx',
  },
  bundle: false,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
});
const reads: { page: unknown; enabled: boolean }[] = [];
const module = { exports: {} as any };
const jsx = (type: unknown, props: unknown) => ({ type, props });
runInNewContext(source.outputFiles[0]!.text, {
  module,
  require(name: string) {
    if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx };
    if (name === 'react-native') return { Text: 'Text', View: 'View', Pressable: 'Pressable' };
    if (name === './page-image-block') return { PageImageBlock: 'PageImageBlock' };
    if (name === './page-image-items')
      return {
        usePageImageItems(page: unknown, enabled: boolean) {
          reads.push({ page, enabled });
          return { items: null, active: enabled, current: () => false, retry() {} };
        },
      };
    if (name === '../components/primitives') return { Icon: 'Icon', IconButton: 'IconButton' };
    if (name === '../components/rich-text') return { RichText: 'RichText' };
    if (name === '../i18n/index.ts') return { t: (key: string) => key };
    if (name === '../theme') return { useTheme: () => ({ colors: {} }) };
    throw new Error(`Unexpected module ${name}`);
  },
});
function nodes(value: any): any[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  return value?.props ? [value, ...nodes(value.props.children)] : [];
}

test('actual PageBlocks disables current File lookup for page_at and forwards the historical boundary', () => {
  const page = { item_id: 'page', space_id: 'space', path: 'page.md', version: 1 };
  for (const historical of [false, true, false]) {
    const tree = module.exports.PageBlocks({
      page,
      historical,
      blocks: [{ id: 'image', kind: 'image', url: 'photo.png', alt: 'A portrait', indent: 0 }],
    });
    assert.equal(reads.at(-1)?.page, page);
    assert.equal(reads.at(-1)?.enabled, !historical);
    const image = nodes(tree).find((element) => element.type === 'PageImageBlock');
    assert.equal(image.props.historical, historical);
    assert.equal(image.props.page, page);
  }
});
