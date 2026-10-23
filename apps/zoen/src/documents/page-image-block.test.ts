import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as reference from './page-image-reference.ts';

const source = await build({
  stdin: {
    contents: readFileSync(new URL('./page-image-block.tsx', import.meta.url), 'utf8'),
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  external: ['*'],
});
const module = { exports: {} as any };
runInNewContext(source.outputFiles[0]!.text, {
  module,
  require(name: string) {
    if (name === 'react/jsx-runtime')
      return {
        jsx: (type: unknown, props: unknown, key: unknown) => ({ type, props, key }),
        jsxs: (type: unknown, props: unknown, key: unknown) => ({ type, props, key }),
      };
    if (name === 'react-native') return { Text: 'Text', View: 'View' };
    if (name === '../components/primitives') return { Icon: 'Icon', NativeButton: 'NativeButton' };
    if (name === '../components/rich-text') return { RichText: 'RichText' };
    if (name === '../media/MediaAttachment') return { FileAttachment: 'ExistingPrivateMedia' };
    if (name === './page-image-reference') return reference;
    if (name === '../i18n') return { t: (key: string) => key };
    if (name === '../theme') return { useTheme: () => ({ colors: {} }) };
    throw new Error(`Unexpected module ${name}`);
  },
});
function nodes(value: any): any[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  return value?.props ? [value, ...nodes(value.props.children)] : [];
}
function render(patch: Record<string, unknown> = {}) {
  return module.exports.PageImageBlock({
    page: { space_id: 'chat', path: 'page.md' },
    block: { id: 'b1', kind: 'image', url: 'sky.png', alt: 'Blue sky over mountains' },
    items: [
      {
        id: 'signed-photo',
        space_id: 'chat',
        kind_id: 'file',
        path: 'sky.png',
        version: 3,
        file: { mime: 'image/png', sha256: 'ab'.repeat(32) },
      },
    ],
    active: true,
    loading: false,
    error: null,
    retry: () => {},
    binding: 'owner:page:version:hash',
    ...patch,
  });
}

test('the actual compiled image block uses the existing private media view with exact binding and ALT', () => {
  const element = render();
  assert.equal(element.type, 'ExistingPrivateMedia');
  assert.deepEqual(JSON.parse(JSON.stringify(element.props)), {
    itemId: 'signed-photo',
    version: 3,
    expectedHash: 'ab'.repeat(32),
    title: 'Blue sky over mountains',
    presentation: 'message',
  });
  assert.match(element.key, /owner:page:version:hash:b1:sky.png:signed-photo:3:/);
  assert.notEqual(render({ binding: 'other-page-version' }).key, element.key);
  assert.equal('uri' in element.props, false);
  assert.equal('source' in element.props, false);
});

test('unresolved, revoked and inactive references never reach private media or turn a URL into an SDK URI', () => {
  for (const patch of [
    { active: false },
    { items: null },
    { items: [] },
    { block: { id: 'b1', url: 'https://example.test/image.png', alt: 'Remote image' } },
    { block: { id: 'b1', url: 'file:///private/secret.png', alt: 'Local file' } },
    { block: { id: 'b1', url: 'data:image/png;base64,YQ==', alt: 'Inline input' } },
  ]) {
    const elements = nodes(render(patch));
    assert.equal(
      elements.some((element) => element.type === 'ExistingPrivateMedia'),
      false,
    );
  }
});

test('failed canonical reads expose retry instead of claiming the ALT is a rendered image', () => {
  const elements = nodes(render({ items: null, error: 'Canonical read failed' }));
  assert.equal(
    elements.find(
      (element) => element.type === 'Text' && element.props.accessibilityRole === 'alert',
    )?.props.children,
    'Canonical read failed',
  );
  assert.equal(
    elements.some((element) => element.type === 'NativeButton'),
    true,
  );
});

test('historical Pages never materialize a current replacement File as their original image', () => {
  for (const version of [1, 3, 5]) {
    const elements = nodes(
      render({
        historical: true,
        page: { item_id: 'old-page', version: 1, space_id: 'chat', path: 'page.md' },
        items: [
          {
            id: 'signed-photo',
            space_id: 'chat',
            kind_id: 'file',
            path: 'sky.png',
            version,
            file: { mime: 'image/png', sha256: 'cd'.repeat(32) },
          },
        ],
      }),
    );
    assert.equal(
      elements.some((element) => element.type === 'ExistingPrivateMedia'),
      false,
    );
    assert.equal(
      elements.some(
        (element) =>
          element.type === 'Text' &&
          element.props.children === 'documents.pageImageHistoricalUnavailable',
      ),
      true,
    );
    assert.equal(
      elements.some(
        (element) =>
          element.type === 'Text' && element.props.children === 'Blue sky over mountains',
      ),
      true,
    );
    assert.equal(
      elements.some((element) => element.type === 'RichText' && element.props.text === 'sky.png'),
      true,
    );
  }
});
