import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as content from '../media/attachment-content.ts';
import {
  editorCode,
  fixture,
  hooks,
  jsx,
  nodes,
  type Element,
} from './attachment-route-ui.fixtures.ts';
const require = createRequire(import.meta.url);

test('production-compiled Article route survives first unloaded render and sends only explicit final native capture', async () => {
  const f = fixture();
  f.render();
  await f.tick();
  let tree = f.render();
  assert.equal(f.counts().sends, 0);
  const editor = nodes(tree).find((node) => node.type === 'ArticleEditor')!;
  editor.props.onChange('**typed** 👩‍💻\n');
  await f.tick();
  tree = f.render();
  assert.equal(f.read().text, '**typed** 👩‍💻\n');
  assert.equal(f.counts().sends, 0);
  f.native('**final native**  \n');
  const send = f.button(tree, 'attachmentTabs.send');
  assert.equal(send.props.disabled, false);
  send.props.onPress();
  send.props.onPress();
  await f.tick();
  assert.equal(f.counts().captured, 1);
  assert.equal(f.counts().sends, 1);
  assert.equal(f.counts().backs, 1);
  editor.props.onChange('late native input after accepted Send');
  await f.tick();
  assert.equal(f.read().text, '');
});

test('compiled Article guard flushes exact final input on Back without sending, and unavailable route never traps dismissal', async () => {
  const f = fixture();
  f.render();
  await f.tick();
  f.render();
  f.native('unsent **final**');
  let repeated = 0;
  assert.equal(f.guard().enabled, true);
  f.guard().action({
    repeat: () => {
      repeated++;
    },
  });
  await f.tick();
  assert.equal(f.read().text, 'unsent **final**');
  assert.equal(f.counts().sends, 0);
  assert.equal(repeated, 1);
  const revoked = fixture();
  revoked.render();
  await revoked.tick();
  revoked.revoke();
  const tree = revoked.render();
  assert.equal(revoked.guard().enabled, false);
  revoked.button(tree, 'attachmentTabs.close').props.onPress();
  assert.equal(revoked.counts().backs, 1);
});

test('retained compiled handlers refuse blur, background, owner replacement and membership loss', async () => {
  for (const retire of ['blur', 'background', 'replaceOwner', 'revoke'] as const) {
    const f = fixture();
    f.render();
    await f.tick();
    const tree = f.render();
    const editor = nodes(tree).find((node) => node.type === 'ArticleEditor')!;
    const send = f.button(tree, 'attachmentTabs.send');
    f[retire]();
    editor.props.onChange('stale input');
    send.props.onPress();
    await f.tick();
    assert.equal(f.counts().sends, 0, retire);
    assert.equal(f.read().text, '**old draft**', retire);
  }
});

test('compiled mode toggle captures once, never sends, and fences a retained old editor callback', async () => {
  const f = fixture();
  f.render();
  await f.tick();
  let tree = f.render();
  const old = nodes(tree).find((node) => node.type === 'ArticleEditor')!;
  f.native('**exact markdown**');
  f.button(tree, 'attachmentTabs.source').props.onPress();
  await f.tick();
  tree = f.render();
  old.props.onChange('late old editor');
  await f.tick();
  assert.equal(f.read().text, '**exact markdown**');
  assert.equal(f.counts().sends, 0);
  assert.equal(nodes(tree).find((node) => node.type === 'ArticleEditor')!.props.source, true);
});

test('compiled Location stages one reviewed GeoJSON in the existing journal, preserving text/reply and never sending', async () => {
  const f = fixture('location');
  f.render();
  await f.tick();
  let tree = f.render();
  assert.equal(f.counts().geoReads, 0);
  f.button(tree, 'attachmentTabs.chooseLocation').props.onPress();
  await f.tick();
  tree = f.render();
  assert.equal(f.counts().geoReads, 1);
  assert.equal(f.read().text, '**old draft**');
  assert.equal(f.readMedia().content_json, 'null', 'selection is transient until Add');
  const caption = nodes(tree).find(
    (node) => node.type === 'Field' && node.props.label === 'attachmentTabs.locationCaption',
  )!;
  assert.ok(
    caption,
    'the reviewed location accepts its own attachment caption without replacing message text',
  );
  caption.props.onChangeText('Reviewed caption');
  tree = f.render();
  const add = f.button(tree, 'attachmentTabs.addToMessage');
  add.props.onPress();
  add.props.onPress();
  await f.tick();
  assert.equal(f.prepared.length, 1);
  assert.deepEqual(JSON.parse(f.prepared[0]!), {
    type: 'Feature',
    geometry: { type: 'Point', coordinates: [2, 1] },
    properties: {},
  });
  const saved = JSON.parse(f.readMedia().content_json);
  assert.equal(saved.reply_to, 'synthetic-reply');
  assert.equal(saved.items[0].caption, 'Reviewed caption');
  assert.equal(f.readMedia().assets[0]!.mime, 'application/geo+json');
  assert.equal(f.read().text, '**old draft**');
  assert.equal(f.counts().sends, 0);
  assert.equal(f.counts().backs, 1);
});

test('compiled Poll stages private input and explicitly retries a lost ACK without a second poll', async () => {
  const f = fixture('poll');
  f.render();
  await f.tick();
  f.render();
  await f.tick();
  let tree = f.render();
  const fields = nodes(tree).filter((node) => node.type === 'Field');
  fields[0]!.props.onChangeText('Question?');
  fields[1]!.props.onChangeText('A');
  tree = f.render();
  nodes(tree)
    .filter((node) => node.type === 'Field')[2]!
    .props.onChangeText('B');
  tree = f.render();
  assert.equal(f.counts().polls, 0);
  f.failPoll();
  const create = f.button(tree, 'attachmentTabs.createPoll');
  create.props.onPress();
  await f.tick();
  tree = f.render();
  assert.equal(f.counts().polls, 1);
  assert.equal(f.mediaChannel.getSnapshot().frozen, true);
  f.button(tree, 'chat.tryAgain2').props.onPress();
  await f.tick();
  assert.equal(f.counts().polls, 1);
  assert.equal(f.counts().backs, 1);
});

test('actual native Article editor formatting handlers use installed native methods and do not capture/send by rendering', async () => {
  const h = hooks();
  const calls: string[] = [];
  const handle = { current: null as any };
  const module: { exports: any } = { exports: {} };
  let editable = true;
  runInNewContext(editorCode, {
    module,
    exports: module.exports,
    require: (name: string) => {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      if (name === 'react') return h.react;
      if (name === 'react/compiler-runtime') return h.runtime;
      if (name === 'react/jsx-runtime') return jsx;
      if (name === 'react-native')
        return { View: 'View', ScrollView: 'ScrollView', TextInput: 'TextInput' };
      if (name === './article-native-input') return { ArticleNativeInput: 'NativeMarkdown' };
      if (name === '../components/primitives')
        return { IconButton: 'IconButton', NativeButton: 'NativeButton', Field: 'Field' };
      if (name === '../i18n') return { t: (key: string) => key };
      if (name === '../theme') return { useTheme: () => ({ colors: {} }) };
      if (name === './attachment-content') return content;
      throw Error(name);
    },
  });
  h.begin();
  const tree = module.exports.ArticleEditor({
    initial: '**initial**',
    disabled: false,
    source: false,
    onChange: () => calls.push('change'),
    canEdit: () => editable,
    ref: handle,
  }) as Element;
  h.commit();
  const native = nodes(tree).find((node) => node.type === 'NativeMarkdown')!;
  native.props.ref.current = {
    toggleBold: () => calls.push('bold'),
    toggleItalic: () => calls.push('italic'),
    toggleStrikethrough: () => calls.push('strike'),
    toggleHeading: () => calls.push('heading'),
    toggleUnorderedList: () => calls.push('bullet'),
    toggleOrderedList: () => calls.push('numbered'),
    getMarkdown: async () => {
      calls.push('capture');
      return '**native**';
    },
  };
  assert.deepEqual(calls, []);
  const buttons = nodes(tree).filter((node) => node.type === 'IconButton');
  for (const button of buttons.slice(0, 6)) button.props.onPress();
  assert.deepEqual(calls, ['bold', 'italic', 'strike', 'heading', 'bullet', 'numbered']);
  assert.equal(await handle.current.capture(), '**native**');
  editable = false;
  buttons[0]!.props.onPress();
  assert.equal(calls.length, 7);
});

test('all compiled attachment sheets use one focus-aware keyboard owner and remeasure on sheet layout', async () => {
  for (const kind of ['article', 'poll', 'contact', 'location']) {
    const f = fixture(kind);
    f.render();
    await f.tick();
    const tree = f.render();
    assert.equal(tree.type, 'KeyboardAwareScrollView', kind);
    assert.equal(tree.props.automaticallyAdjustKeyboardInsets, false, kind);
    assert.equal(tree.props.keyboardShouldPersistTaps, 'handled', kind);
    assert.equal(tree.props.contentInsetAdjustmentBehavior, 'automatic', kind);
    assert.ok(tree.props.bottomOffset > 0, kind);
    const before = f.counts();
    for (const [width, height] of [
      [393, 550],
      [852, 160],
      [393, 550],
    ])
      tree.props.onLayout({ nativeEvent: { layout: { x: 0, y: 0, width, height } } });
    assert.deepEqual(f.counts(), { ...before, reveals: before.reveals + 3 }, kind);
    assert.equal(f.read().text, '**old draft**', kind);
  }
});

test('compiled Poll relayout preserves every typed field and once-only create without remounting', async () => {
  const f = fixture('poll');
  f.render();
  await f.tick();
  f.render();
  await f.tick();
  let tree = f.render();
  const values = ['Question? 👩‍💻', 'First', 'Second'];
  for (let index = 0; index < values.length; index++) {
    nodes(tree)
      .filter((node) => node.type === 'Field')
      [index]!.props.onChangeText(values[index]);
    tree = f.render();
  }
  f.button(tree, 'attachmentTabs.addOption').props.onPress();
  tree = f.render();
  nodes(tree)
    .filter((node) => node.type === 'Field')[3]!
    .props.onChangeText('Third');
  tree = f.render();
  const create = f.button(tree, 'attachmentTabs.createPoll');
  for (const [width, height] of [
    [852, 160],
    [393, 550],
  ]) {
    tree.props.onLayout({ nativeEvent: { layout: { x: 0, y: 0, width, height } } });
    tree = f.render();
    assert.deepEqual(
      nodes(tree)
        .filter((node) => node.type === 'Field')
        .map((node) => node.props.value),
      [...values, 'Third'],
    );
    assert.equal(tree.props.key, undefined);
    assert.equal(f.counts().polls, 0);
    assert.equal(f.counts().backs, 0);
  }
  create.props.onPress();
  create.props.onPress();
  await f.tick();
  assert.equal(f.counts().polls, 1);
  assert.equal(f.counts().backs, 1);
  tree.props.onLayout({ nativeEvent: { layout: { width: 393, height: 550 } } });
  assert.equal(f.counts().reveals, 2, 'closed sheet cannot reissue focus work');
});

test('retained sheet layout callbacks refuse scope retirement without altering or sending drafts', async () => {
  for (const retire of ['blur', 'background', 'replaceOwner', 'revoke', 'retire'] as const) {
    const f = fixture();
    f.render();
    await f.tick();
    const tree = f.render();
    f[retire]();
    tree.props.onLayout({ nativeEvent: { layout: { width: 852, height: 160 } } });
    assert.equal(f.counts().reveals, 0, retire);
    assert.equal(f.counts().captured, 0, retire);
    assert.equal(f.counts().sends, 0, retire);
    assert.equal(f.read().text, '**old draft**', retire);
  }
});
