import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { build } from 'esbuild';
import * as jsx from 'react/jsx-runtime';
import * as formats from './binary-formats.ts';
import type { ItemDetail } from '../core/types';
import type { MiniappView } from './view-host';
import type { AppSpecDto } from '../core/generated/AppSpecDto';

const source = new URL('./view-host.tsx', import.meta.url);
const compiled = await build({
  stdin: {
    contents: readFileSync(source, 'utf8'),
    sourcefile: fileURLToPath(source),
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'native-ports',
      setup(b) {
        b.onResolve({ filter: /.*/ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});
const ports: Record<string, unknown> = {
  'react/jsx-runtime': jsx,
  './binary-formats': formats,
  './app-keyboard-viewport': { AppKeyboardViewport: 'KeyboardViewport' },
  './deckcraft-view': { DeckcraftView: 'ManagedBinary' },
  './canonical-view': { CanonicalDrawingView: 'Drawing' },
  './view-host-builtin': { BuiltinMiniappView: 'Generic' },
};
const module = { exports: {} as { MiniappView: typeof MiniappView } };
runInNewContext(compiled.outputFiles[0]!.text, {
  module,
  exports: module.exports,
  require(name: string) {
    assert.ok(Object.hasOwn(ports, name), name);
    return ports[name];
  },
});

for (const editor of ['deckcraft', 'gridcraft', 'wordcraft', 'excalidraw', 'other'] as const) {
  test(`actual fullscreen ${editor} UI selects its document host and preserves lifetime props`, () => {
    const selectedHead = { version: '1', sha256: 'a'.repeat(64) };
    const onDocumentController = () => {};
    const onDocumentDiscard = () => {};
    const item = { id: 'synthetic', app: { app_id: editor } } as ItemDetail;
    const spec = { id: editor, resource_uri: `ui://zoen/${editor}/index.html` } as AppSpecDto;
    const tree = module.exports.MiniappView({
      item,
      spec,
      fill: true,
      selectedHead,
      onDocumentController,
      onDocumentDiscard,
    });
    assert.equal(tree.type, 'KeyboardViewport');
    assert.equal(tree.props.enabled, true);
    assert.equal(tree.props.scope, JSON.stringify([item.id, spec.resource_uri]));
    const child = tree.props.children;
    assert.equal(
      child.type,
      editor === 'excalidraw' ? 'Drawing' : editor === 'other' ? 'Generic' : 'ManagedBinary',
    );
    assert.equal(child.props.item, item);
    assert.equal(child.props.spec, spec);
    if (child.type === 'ManagedBinary') {
      assert.equal(child.props.selectedHead, selectedHead);
      assert.equal(child.props.onDocumentController, onDocumentController);
      assert.equal(child.props.onDocumentDiscard, onDocumentDiscard);
    }
  });
}
