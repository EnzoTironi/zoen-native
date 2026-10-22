import assert from 'node:assert/strict';
import { test } from 'node:test';
import { DEFAULT_THEME_CUSTOMIZATION, telegramPresentation } from '../theme/telegram.ts';
import type { Element, Handler } from './attachment-composer-ui.fixtures.ts';
import { byId, find, fixture, parentOf, scan, styleOf } from './attachment-composer-ui.fixtures.ts';

test('iOS keeps the exact send intent inside the field while empty audio and search navigation stay outside', () => {
  const f = fixture();
  const surface = (tree: Element) => find(tree, byId('message-composer-field-surface'));
  let tree = f.render();
  assert.equal(
    scan(surface(tree)).some(byId('send-message')),
    true,
    'The real Send callback must be rendered inside the native field surface',
  );
  const send = find(surface(tree), byId('send-message'));
  (send.props.onPress as Handler)();
  assert.deepEqual(f.calls, ['send-normal']);
  assert.equal(scan(tree).filter(byId('send-message')).length, 1);

  tree = f.render(undefined, { value: '' });
  assert.equal(
    scan(surface(tree)).some(byId('send-message')),
    true,
    'Send remains mounted while hidden',
  );
  assert.equal(
    find(tree, (node) => node.type === 'ComposerMotionSlot' && node.props.kind === 'send').props
      .active,
    false,
  );
  assert.equal(
    find(tree, (node) => node.type === 'ComposerMotionSlot' && node.props.kind === 'media').props
      .children,
    'Microphone',
  );
  assert.equal(
    scan(surface(tree)).some((node) => node.props.children === 'Microphone'),
    false,
  );

  tree = f.render(f.file);
  (find(surface(tree), byId('send-attachments')).props.onPress as Handler)();
  assert.deepEqual(f.calls, ['send-normal', 'send-files']);
  assert.equal(scan(tree).some(byId('send-message')), false);
  const moves: unknown[] = [];
  const search = {
    state: {
      active: true,
      completed: true,
      loading: false,
      hits: [{}],
      index: 0,
      cursor: null,
      query: 'term',
    },
    status: '1/1',
    onClose: () => {},
    onMove: (direction: unknown) => moves.push(direction),
    onAsk: () => moves.push('ai'),
    onSearch: () => {},
  };
  tree = f.render(f.file, { search, value: 'term' });
  assert.equal(scan(surface(tree)).some(byId('older-search-result')), false);
  assert.ok(find(tree, byId('older-search-result')).props.disabled);
  assert.ok(find(tree, byId('newer-search-result')).props.disabled);
  assert.equal(scan(tree).some(byId('send-attachments')), false);

  tree = f.render(f.file, {
    search: { ...search, state: { ...search.state, hits: [], index: -1 } },
    value: 'term',
  });
  (find(surface(tree), byId('ask-conversation')).props.onPress as Handler)();
  assert.deepEqual(moves, ['ai']);
  assert.deepEqual(f.calls, ['send-normal', 'send-files']);
});

test('the mounted iOS field uses one font for display and scoped native fitting at larger system text sizes', async () => {
  const f = fixture('ios', 1.8);
  const inputRef = { current: null as unknown };
  const root = f.render(undefined, {
    inputRef,
    context: { entry: { id: 'reply' }, onCancel: () => {} },
  });
  const field = find(root, byId('message-input'));
  const textStyle = (field.props.style as Record<string, unknown>[])[0]!;
  assert.equal(textStyle.fontSize, 17);
  assert.equal(textStyle.minHeight, 40);
  assert.equal(textStyle.maxHeight, 160);
  assert.equal(textStyle.paddingTop, 9.5);
  assert.equal(textStyle.paddingBottom, 9.5);
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.ok(f.measurements.length);
  assert.ok(f.measurements.every((request) => Math.abs(request.fontSize - 30.6) < 1e-9));
  assert.equal(inputRef.current, f.nativeField);
  assert.equal(
    find(root, (node) => node.type === 'MessageContext').props.entry &&
      scan(root).filter((node) => node.type === 'MessageContext').length,
    1,
  );
  const outer = find(f.renderRoot({}), (node) => node.type === 'AnimatedView');
  const outerStyle = Object.assign({}, ...(outer.props.style as object[]));
  assert.equal(
    outerStyle.paddingBottom,
    34,
    'the native viewport already applies the keyboard correction',
  );
  assert.equal(outerStyle.paddingTop, 0);
  assert.equal(outerStyle.paddingHorizontal, 8);
  f.render(f.file, { inputRef });
  assert.equal(
    inputRef.current,
    f.nativeField,
    'caption mode must keep the same real editor handle',
  );
});

test('Android and Web keep the existing outside-send layout and composer sizing', () => {
  for (const platform of ['android', 'web'] as const) {
    const f = fixture(platform);
    const tree = f.render();
    const surface = find(tree, byId('message-composer-field-surface'));
    assert.equal(scan(surface).some(byId('send-message')), false);
    assert.equal(scan(tree).filter(byId('send-message')).length, 1);
    const style = (find(tree, byId('message-input')).props.style as Record<string, unknown>[])[0]!;
    assert.equal(style.minHeight, 52);
    assert.equal(style.fontSize, 16);
    assert.equal(f.measurements.length, 0);
  }
});

test('a theme font change measures the displayed font and rejects late fitting of the old font', async () => {
  const f = fixture('ios', 1.5);
  f.deferFits();
  let tree = f.render();
  assert.ok(f.pendingFits.length);
  const first = f.pendingFits.map((fit) => fit);
  assert.ok(first.every((fit) => fit.request.fontSize === 25.5));
  f.theme.presentation = telegramPresentation('night', {
    ...DEFAULT_THEME_CUSTOMIZATION,
    textSize: 23,
  });
  tree = f.render();
  const actual = find(tree, (node) => node.type === 'TextInput');
  const flattened = Object.assign({}, ...(actual.props.style as any[]));
  assert.equal(flattened.fontSize, 23);
  const latest = f.pendingFits.at(-1)!;
  assert.equal(latest.request.fontSize, 34.5);
  for (const fit of first) fit.resolve({ ...fit.request, height: 154 });
  await new Promise((done) => setImmediate(done));
  tree = f.render();
  assert.notEqual(
    Object.assign({}, ...(find(tree, (node) => node.type === 'TextInput').props.style as any[]))
      .height,
    154,
  );
  latest.resolve({ ...latest.request, height: 88 });
  await new Promise((done) => setImmediate(done));
  tree = f.render();
  assert.equal(
    Object.assign({}, ...(find(tree, (node) => node.type === 'TextInput').props.style as any[]))
      .height,
    88,
  );
});

test('compiled iOS renderer keeps a full-width field and reserves the source inset only once beside absolute controls', () => {
  const f = fixture();
  let tree = f.render();
  const field = find(tree, byId('message-input'));
  const surface = find(tree, byId('message-composer-field-surface'));
  const fieldStyle = styleOf(field);
  assert.equal(fieldStyle.fontSize, 17);
  assert.equal(fieldStyle.flex, undefined, 'vertical flex must not collapse multiline fitting');
  assert.equal(fieldStyle.minWidth, 0);
  assert.equal(fieldStyle.width, '100%', 'full-width editor is not shortened by the send slot');
  assert.equal(
    fieldStyle.paddingRight,
    67,
    '32pt mode control measurement +46pt send are reserved once',
  );
  assert.equal(fieldStyle.paddingLeft, 12);
  assert.equal(fieldStyle.paddingTop, 9.5);
  assert.equal(fieldStyle.paddingBottom, 9.5);
  assert.equal(field.props.selectionColor, f.theme.presentation.chat.input.action);
  assert.equal(styleOf(find(tree, byId('message-composer-field-frame'))).flex, 1);
  assert.notEqual(
    styleOf(surface).flexDirection,
    'row',
    'children must not shrink the field horizontally',
  );
  const mode = find(tree, byId('composer-input-mode'));
  assert.equal(styleOf(mode).width, 32);
  assert.equal(styleOf(mode).height, 40);
  assert.deepEqual(styleOf(parentOf(tree, mode)), { position: 'absolute', bottom: 0, right: 4 });
  assert.equal(
    parentOf(tree, mode).props.hasSend,
    true,
    'mode translates with the retained Send slot',
  );
  const send = find(tree, byId('send-message'));
  assert.equal(send.props.minimumHeight, 40);
  assert.deepEqual(styleOf(parentOf(tree, send)), { position: 'absolute', bottom: 0, right: 0 });
  tree = f.render(undefined, { value: '' });
  assert.equal(styleOf(find(tree, byId('message-input'))).paddingRight, 21);
  const emptyMode = find(tree, byId('composer-input-mode'));
  assert.deepEqual(styleOf(parentOf(tree, emptyMode)), {
    position: 'absolute',
    bottom: 0,
    right: 4,
  });
  assert.equal(
    find(tree, (node) => node.type === 'ComposerMotionSlot' && node.props.kind === 'media').props
      .children,
    'Microphone',
  );
  assert.equal(styleOf(find(tree, byId('message-composer-field-frame'))).marginRight, 46);
});

test('compiled field preserves its actual native owner while input-mode, caption, edit and search change', () => {
  const f = fixture();
  const inputRef = { current: null as unknown };
  let tree = f.render(undefined, { inputRef });
  const first = find(tree, byId('message-input'));
  const firstMode = find(tree, byId('composer-input-mode'));
  assert.equal(find(firstMode, (node) => node.type === 'ComposerGlyph').props.name, 'face.smiling');
  (firstMode.props.onPress as Handler)();
  tree = f.render(undefined, { inputRef });
  assert.equal(f.keyboard.kind, 'emoji');
  assert.equal(
    find(find(tree, byId('composer-input-mode')), (node) => node.type === 'ComposerGlyph').props
      .name,
    'keyboard',
  );
  assert.equal(find(tree, byId('message-input')).props.ref, first.props.ref);
  assert.equal(inputRef.current, f.nativeField);
  const cases = [
    { files: f.file, patch: {}, context: 'space::caption:1', value: '' },
    {
      files: undefined,
      patch: { editing: true, value: 'Edited draft' },
      context: 'space::edit',
      value: 'Edited draft',
    },
    {
      files: undefined,
      patch: {
        search: {
          state: {
            active: true,
            completed: true,
            loading: false,
            hits: [{}],
            index: 0,
            cursor: null,
          },
          status: '1/1',
        },
        value: 'term',
      },
      context: 'space::search',
      value: 'term',
    },
    { files: undefined, patch: {}, context: 'space::normal', value: 'My unsent normal text' },
  ];
  for (const current of cases) {
    tree = f.render(current.files, { inputRef, ...current.patch });
    const field = find(tree, byId('message-input'));
    assert.equal(field.props.ref, first.props.ref);
    assert.equal(field.props.value, current.value);
    assert.equal(scan(tree).filter(byId('message-input')).length, 1);
    assert.equal(
      find(tree, (node) => node.type === 'ComposerMotionSlot' && node.props.kind === 'media').props
        .children,
      'Microphone',
      'normal, caption, edit and search retain the exact media gesture path',
    );
    assert.equal(inputRef.current, f.nativeField);
    assert.ok(
      f.keyboardFields.some(([tag, context]) => tag === 101 && context === current.context),
    );
    if (current.context.endsWith('search'))
      assert.equal(parentOf(tree, find(tree, byId('composer-input-mode'))).props.active, false);
  }
  const providerFor = (patch: Record<string, unknown>) =>
    find(f.renderRoot(patch), (node) => node.type === 'AttachmentKeyboardProvider');
  const provider = providerFor({ search: { state: { completed: false } } });
  assert.equal(provider.type, 'AttachmentKeyboardProvider');
  assert.equal(provider.props.scope, 'space:');
  assert.equal(provider.props.enabled, false, 'search retires accessory input authority');
  assert.equal(providerFor({ suspended: true }).props.enabled, false);
  assert.equal(providerFor({}).props.enabled, true);
});

test('compiled source minimum tracks larger named font geometry for field, mode and send without changing the native ref', () => {
  const f = fixture();
  const inputRef = { current: null as unknown };
  const first = find(f.render(undefined, { inputRef }), byId('message-input'));
  f.theme.presentation = telegramPresentation('night', {
    ...DEFAULT_THEME_CUSTOMIZATION,
    textSize: 23,
  });
  const tree = f.render(undefined, { inputRef });
  const field = find(tree, byId('message-input'));
  assert.equal(field.props.ref, first.props.ref);
  assert.equal(styleOf(field).fontSize, 23);
  assert.equal(styleOf(field).minHeight, 47);
  assert.equal(find(tree, byId('message-composer-field-surface')).props.cornerRadius, 23);
  assert.equal(styleOf(find(tree, byId('composer-input-mode'))).height, 47);
  assert.equal(find(tree, byId('send-message')).props.minimumHeight, 47);
  assert.equal(styleOf(field).paddingRight, 67);
  assert.equal(inputRef.current, f.nativeField);
});

test('compiled composer routes its exact field children through the reusable original glass surfaces', () => {
  for (const available of [true, false]) {
    const f = fixture('ios', 1, available);
    const tree = f.render();
    const row = find(tree, byId('message-composer-row'));
    const field = find(tree, byId('message-composer-field-surface'));
    assert.equal(row.type, 'ComposerGlassContainer');
    assert.equal(field.type, 'ComposerGlassSurface');
    assert.equal(field.props.cornerRadius, 20);
    assert.equal(scan(field).filter(byId('message-input')).length, 1);
    assert.equal(
      styleOf(field).backgroundColor,
      undefined,
      'the original renderer owns its fill and stroke',
    );
  }
});

test('the shared composer guards both main and thread content while retaining query-only search', () => {
  for (const threadRootId of [undefined, 'thread-root']) {
    const f = fixture();
    const root = f.renderRoot({ threadRootId });
    assert.equal(root.type, 'ConversationComposerAccess');
    assert.equal(root.props.spaceId, 'space');
    assert.equal(root.props.searching, false);
    assert.equal(
      find(root, (node) => node.type === 'MediaComposer').props.threadRootId,
      threadRootId ?? null,
    );
    const search = f.renderRoot({ search: { state: {} } });
    assert.equal(search.props.searching, true);
  }
});
