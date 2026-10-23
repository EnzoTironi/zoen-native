import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { inlineImageFrame } from '../media/inline-image-frame.ts';
const compiled = await build({
  stdin: {
    contents: readFileSync(new URL('../media/inline-file-image.tsx', import.meta.url), 'utf8'),
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
runInNewContext(compiled.outputFiles[0]!.text, {
  module,
  require(name: string) {
    if (name === 'react/jsx-runtime')
      return {
        jsx: (type: unknown, props: unknown) => ({ type, props }),
        jsxs: (type: unknown, props: unknown) => ({ type, props }),
      };
    if (name === 'expo-image') return { Image: 'NativeImage' };
    if (name === 'react-native') return { View: 'View' };
    if (name === '../components/message-interaction') return { MessagePressable: 'Pressable' };
    if (name === '../i18n') return { t: (_: string, values: any) => values.name };
    throw new Error(`Unexpected module ${name}`);
  },
});
function fixture() {
  let active = true,
    opens = 0,
    errors = 0;
  const decoded: number[][] = [];
  const element = module.exports.InlineFileImage({
    local: { itemId: 'photo', version: 2, info: { sha256: 'hash' }, uri: 'file:///verified-copy' },
    label: 'Sky above the mountains',
    mine: false,
    frame: inlineImageFrame(320, 2 / 3),
    outline: 'outline',
    isCurrent: () => active,
    onOpen: () => opens++,
    onFailure: () => errors++,
    onDecoded: (...args: number[]) => decoded.push(args),
  });
  return {
    element,
    image: element.props.children[0],
    decoded,
    counts: () => ({ opens, errors }),
    retire: () => {
      active = false;
    },
  };
}

test('actual shared image surface contains the full portrait and keeps ALT on its preview control', () => {
  const f = fixture();
  const style = f.element.props.style({ pressed: false });
  assert.equal(f.image.props.contentFit, 'contain');
  assert.equal(f.image.props.cachePolicy, 'none');
  assert.equal(f.element.props.accessibilityLabel, 'Sky above the mountains');
  assert.equal(style.width / style.height, 2 / 3);
  f.image.props.onLoad({ source: { url: 'file:///verified-copy', width: 800, height: 1200 } });
  assert.deepEqual(f.decoded, [[800, 1200]]);
  f.element.props.onPress();
  assert.equal(f.counts().opens, 1);
});

test('wrong URI, invalid decoded dimensions and retired callbacks cannot replace geometry/open old previews', () => {
  const f = fixture();
  for (const source of [
    { url: 'file:///old', width: 100, height: 100 },
    { url: 'file:///verified-copy', width: 0, height: 100 },
    { url: 'file:///verified-copy', width: 100, height: Infinity },
  ])
    f.image.props.onLoad({ source });
  assert.deepEqual(f.decoded, []);
  f.retire();
  f.image.props.onLoad({ source: { url: 'file:///verified-copy', width: 800, height: 1200 } });
  f.image.props.onError();
  f.element.props.onPress();
  assert.deepEqual(f.decoded, []);
  assert.deepEqual(f.counts(), { opens: 0, errors: 0 });
});
