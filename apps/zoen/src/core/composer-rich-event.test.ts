import assert from 'node:assert/strict';
import { test } from 'node:test';
import { byId, find, fixture, type Handler } from './attachment-composer-ui.fixtures.ts';
import { nativeEventAdapter } from './composer-native-event-adapter.fixtures.ts';
import { richEventAdapter } from './composer-rich-event.fixtures.ts';

test('actual ENRM canonical event reaches compiled Send before delayed JS and cannot resurrect accepted text', async () => {
  const f = fixture('ios', 1, false, undefined, true);
  const reanimated = await nativeEventAdapter();
  let draft = 'café 🦊';
  const sent: string[] = [];
  const tree = f.render(undefined, {
    value: draft,
    onChangeText: (value: string) => {
      draft = value;
    },
    onSend: () => {
      sent.push(draft);
      draft = '';
    },
  });
  const input = find(tree, byId('message-input'));
  const { onNativeChange, ...props } = input.props;
  const connection = reanimated.connect(props, onNativeChange as Handler, 'onChangeMarkdown');
  const native = richEventAdapter(connection.filtered);
  const binding = props.composerBinding;
  const event = { text: '**café 🦊**', value: '**café 🦊**', binding, eventCount: 8 };
  assert.equal(reanimated.registrations[0]!.event, 'onChangeMarkdown');
  reanimated.registrations[0]!.worklet({ ...event, binding: 'retired-owner' });
  assert.equal(draft, 'café 🦊');
  reanimated.registrations[0]!.worklet(event);
  (find(tree, byId('send-message')).props.onPress as Handler)();
  assert.deepEqual(sent, ['**café 🦊**']);
  native.change({ nativeEvent: event });
  assert.equal(draft, '', 'late actual ENRM callback cannot recreate accepted text');
  assert.equal(native.receipt().eventCount, 8);
  const next = { ...event, eventCount: 9 };
  reanimated.registrations[0]!.worklet(next);
  native.change({ nativeEvent: next });
  assert.equal(draft, next.text, 'new typing with the same canonical text is admitted');
  native.change({ nativeEvent: { ...next, binding: 'retired-owner', eventCount: 10 } });
  assert.equal(native.receipt().eventCount, 9, 'actual public facade rejects another binding');
  connection.detach();
});

test('rich input switches plain search and caption on the same editor while stale owner callbacks retire', () => {
  const f = fixture('ios', 1, false, undefined, true);
  const first = find(f.render(), byId('message-input'));
  assert.equal(first.props.composerRichText, true);
  const old = richEventAdapter(first.props);
  const search = find(
    f.render(undefined, {
      search: {
        state: { query: '**literal**', hits: [], completed: false },
        onClose() {},
        onMove() {},
        onAsk() {},
        onSearch() {},
      },
    }),
    byId('message-input'),
  );
  assert.equal(search.type, first.type);
  assert.equal(search.props.composerRichText, false);
  assert.notEqual(search.props.composerBinding, first.props.composerBinding);
  const field = richEventAdapter(search.props).rendered;
  assert.equal(field.props.selectionMenuConfig.format.enabled, false);
  assert.equal(field.props.composerSubmitOnReturn, true);
  old.change({
    nativeEvent: {
      text: '**private**',
      value: '**private**',
      binding: first.props.composerBinding,
      eventCount: 3,
    },
  });
  assert.deepEqual(f.calls, []);
  const returned = find(f.render(), byId('message-input'));
  assert.notEqual(
    returned.props.composerBinding,
    first.props.composerBinding,
    'A→search→A cannot reuse a binding for a delayed native link/menu callback',
  );
  const caption = find(f.render(f.file), byId('message-input'));
  assert.equal(caption.type, first.type);
  assert.equal(caption.props.composerRichText, true);
  assert.notEqual(caption.props.composerBinding, first.props.composerBinding);
  const restored = richEventAdapter(caption.props).rendered;
  assert.equal(restored.props.selectionMenuConfig.format.enabled, true);
  assert.equal(restored.props.formatMenuConfig.underline.enabled, false);
});
