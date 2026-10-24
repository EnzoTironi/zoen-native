import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import * as content from '../media/attachment-content.ts';
import {
  editorCode,
  fixture as routeFixture,
  hooks,
  jsx,
  nodes,
  type Element,
} from './attachment-route-ui.fixtures.ts';
const require = createRequire(import.meta.url);
const tick = async () => {
  for (let i = 0; i < 30; i++) await Promise.resolve();
};
function fixture() {
  const h = hooks(),
    calls: unknown[][] = [],
    handle = { current: null as any };
  let generation = 0,
    current = true,
    markdown = 'before selected after',
    blocked: Promise<string> | null = null,
    changes = 0;
  const module = { exports: {} as any };
  runInNewContext(editorCode, {
    module,
    exports: module.exports,
    require: (name: string) => {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      if (name === 'react') return h.react;
      if (name === 'react/compiler-runtime') return h.runtime;
      if (name === 'react/jsx-runtime') return jsx;
      if (name === 'react-native')
        return { View: 'View', Text: 'Text', ScrollView: 'ScrollView', TextInput: 'TextInput' };
      if (name === 'react-native-enriched-markdown')
        return { EnrichedMarkdownTextInput: 'NativeMarkdown' };
      if (name === '../components/primitives')
        return { IconButton: 'IconButton', NativeButton: 'NativeButton', Field: 'Field' };
      if (name === '../i18n') return { t: (key: string) => key };
      if (name === '../theme') return { useTheme: () => ({ colors: {} }) };
      if (name === './attachment-content') return content;
      throw Error(name);
    },
  });
  const editor = {
    getMarkdown: async () => {
      calls.push(['capture']);
      return blocked ?? markdown;
    },
    focus: () => calls.push(['focus']),
    setSelection: (start: number, end: number) => calls.push(['selection', start, end]),
    setLink: (url: string) => calls.push(['link', url]),
  };
  function render() {
    h.begin();
    const tree = module.exports.ArticleEditor({
      initial: markdown,
      disabled: false,
      source: false,
      canEdit: () => current,
      captureEditLease: () => {
        const captured = generation;
        return () => current && generation === captured;
      },
      onChange: () => changes++,
      ref: handle,
    }) as Element;
    h.commit();
    nodes(tree).find((node) => node.type === 'NativeMarkdown')!.props.ref.current = editor;
    return tree;
  }
  const find = (tree: Element, label: string) =>
    nodes(tree).find((node) => node.props.label === label)!;
  const native = (tree: Element) => nodes(tree).find((node) => node.type === 'NativeMarkdown')!;
  async function open() {
    let tree = render();
    native(tree).props.onChangeSelection({ start: 7, end: 15 });
    tree = render();
    assert.equal(find(tree, 'attachmentTabs.link').props.disabled, false);
    find(tree, 'attachmentTabs.link').props.onPress();
    await tick();
    tree = render();
    assert.ok(find(tree, 'attachmentTabs.linkUrl'));
    return tree;
  }
  return {
    render,
    find,
    native,
    open,
    calls,
    handle,
    changes: () => changes,
    text: (value: string) => {
      markdown = value;
    },
    block: (value: Promise<string> | null) => {
      blocked = value;
    },
    retire: () => {
      generation++;
      current = false;
    },
    resume: () => {
      current = true;
    },
    unmount: () => h.retire(),
  };
}

test('Article Link is disabled at a plain caret and enabled only by an actual native selection', () => {
  const f = fixture();
  let tree = f.render();
  assert.equal(f.find(tree, 'attachmentTabs.link').props.disabled, true);
  f.native(tree).props.onChangeSelection({ start: 7, end: 7 });
  tree = f.render();
  assert.equal(f.find(tree, 'attachmentTabs.link').props.disabled, true);
  f.native(tree).props.onChangeSelection({ start: 7, end: 15 });
  tree = f.render();
  assert.equal(f.find(tree, 'attachmentTabs.link').props.disabled, false);
  assert.deepEqual(f.calls, []);
  assert.equal(f.changes(), 0);
});

test('Article URL applies once to original selected text after URL input owns focus; opening never updates the draft', async () => {
  const f = fixture();
  let tree = await f.open();
  assert.equal(f.changes(), 0);
  assert.equal(
    f.native(tree).props.editable,
    false,
    'URL form owns focus; body cannot drift under its selected offsets',
  );
  assert.deepEqual(f.calls, [['capture']]);
  f.native(tree).props.onChangeSelection({ start: 0, end: 0 });
  f.find(tree, 'attachmentTabs.linkUrl').props.onChangeText('https://example.invalid/reviewed');
  tree = f.render();
  const apply = f.find(tree, 'attachmentTabs.applyLink');
  assert.equal(apply.props.disabled, false);
  apply.props.onPress();
  apply.props.onPress();
  await tick();
  assert.deepEqual(f.calls, [
    ['capture'],
    ['capture'],
    ['selection', 7, 15],
    ['link', 'https://example.invalid/reviewed'],
  ]);
  assert.equal(f.changes(), 0, 'native Markdown event remains the only draft update');
  assert.equal(f.find(f.render(), 'attachmentTabs.linkUrl'), undefined);
  assert.deepEqual(f.calls.at(-1), ['focus']);
  f.native(f.render()).props.onChangeMarkdown(
    'before [selected](https://example.invalid/reviewed) after',
  );
  assert.equal(f.changes(), 1);
});

test('URL apply refuses changed native bytes rather than formatting old offsets in a replacement document', async () => {
  const f = fixture();
  let tree = await f.open();
  f.find(tree, 'attachmentTabs.linkUrl').props.onChangeText('https://example.invalid');
  tree = f.render();
  f.text('replacement content');
  f.find(tree, 'attachmentTabs.applyLink').props.onPress();
  await tick();
  assert.deepEqual(f.calls, [['capture'], ['capture']]);
  assert.equal(f.changes(), 0);
  assert.ok(nodes(f.render()).some((node) => node.props.accessibilityRole === 'alert'));
});

test('cancelled URL callbacks cannot modify a later link session', async () => {
  const f = fixture(),
    old = await f.open();
  const oldField = f.find(old, 'attachmentTabs.linkUrl'),
    oldApply = f.find(old, 'attachmentTabs.applyLink');
  f.find(old, 'common.cancel').props.onPress();
  let tree = await f.open();
  oldField.props.onChangeText('https://example.invalid/late');
  oldApply.props.onPress();
  await tick();
  tree = f.render();
  assert.equal(f.find(tree, 'attachmentTabs.linkUrl').props.value, '');
  assert.equal(f.find(tree, 'attachmentTabs.applyLink').props.disabled, true);
  assert.equal(f.calls.filter((call) => call[0] === 'link').length, 0);
});

test('same-owner refocus cannot revive a retired link during either native await', async () => {
  for (const phase of ['open', 'apply'] as const) {
    const f = fixture();
    let done!: (value: string) => void;
    const pending = new Promise<string>((resolve) => {
      done = resolve;
    });
    if (phase === 'open') {
      let tree = f.render();
      f.native(tree).props.onChangeSelection({ start: 7, end: 15 });
      tree = f.render();
      f.block(pending);
      f.find(tree, 'attachmentTabs.link').props.onPress();
    } else {
      let tree = await f.open();
      f.find(tree, 'attachmentTabs.linkUrl').props.onChangeText('https://example.invalid');
      tree = f.render();
      f.block(pending);
      f.find(tree, 'attachmentTabs.applyLink').props.onPress();
    }
    f.retire();
    f.resume();
    done('before selected after');
    await tick();
    assert.equal(
      f.calls.filter((call) => call[0] === 'link' || call[0] === 'focus').length,
      0,
      phase,
    );
    assert.equal(f.changes(), 0);
    assert.equal(f.find(f.render(), 'attachmentTabs.linkUrl'), undefined);
  }
});

test('unmounted editor cannot consume late native capture', async () => {
  const f = fixture();
  let tree = f.render();
  f.native(tree).props.onChangeSelection({ start: 7, end: 15 });
  tree = f.render();
  let done!: (value: string) => void;
  f.block(
    new Promise<string>((resolve) => {
      done = resolve;
    }),
  );
  f.find(tree, 'attachmentTabs.link').props.onPress();
  f.unmount();
  done('before selected after');
  await tick();
  assert.equal(f.calls.filter((call) => call[0] === 'link').length, 0);
  assert.equal(f.changes(), 0);
});

test('actual route lease stays retired across same-owner background and foreground without changing its draft', async () => {
  const f = routeFixture();
  f.render();
  await f.tick();
  let tree = f.render();
  const editor = nodes(tree).find((node) => node.type === 'ArticleEditor')!;
  const old = editor.props.captureEditLease();
  assert.equal(old(), true);
  f.background();
  assert.equal(old(), false);
  f.foreground();
  assert.equal(old(), false);
  tree = f.render();
  const fresh = nodes(tree)
    .find((node) => node.type === 'ArticleEditor')!
    .props.captureEditLease();
  assert.equal(fresh(), true);
  assert.equal(f.read().text, '**old draft**');
  assert.equal(f.counts().sends, 0);
});

test('retired idle URL form can be cancelled without refocusing or mutating and then explicitly reopened', async () => {
  const f = fixture();
  let tree = await f.open();
  f.retire();
  f.resume();
  f.find(tree, 'common.cancel').props.onPress();
  tree = f.render();
  assert.equal(f.find(tree, 'attachmentTabs.linkUrl'), undefined);
  assert.equal(f.calls.filter((call) => call[0] === 'focus').length, 0);
  await f.open();
  assert.equal(f.changes(), 0);
});
