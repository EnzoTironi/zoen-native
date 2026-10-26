import assert from 'node:assert/strict';
import test from 'node:test';
import { nativeEventAdapter } from './composer-native-event-adapter.fixtures.ts';
import { byId, find, fixture, type Handler } from './attachment-composer-ui.fixtures.ts';

test('installed Reanimated observer preserves callable RN onChange, onChangeText and native synchronization', async () => {
  const adapter = await nativeEventAdapter();
  const calls: string[] = [];
  const observe = (event: { text: string }) => calls.push(`ui:${event.text}`);
  const onChange = (event: { nativeEvent: { text: string } }) =>
    calls.push(`change:${event.nativeEvent.text}`);
  const onChangeText = (text: string) => calls.push(`text:${text}`);
  const field = adapter.connect({ onChange, onChangeText }, observe);
  assert.equal(field.filtered.onChange, onChange);
  assert.equal(field.filtered.onChangeText, onChangeText);
  assert.equal('onNativeChange' in field.filtered, false);
  assert.equal(adapter.registrations.length, 1);
  assert.equal(adapter.registrations[0]!.tag, 101);
  assert.equal(adapter.registrations[0]!.event, 'onChange');
  const event = { text: 'Última correção 🪷', eventCount: 8 };
  adapter.registrations[0]!.worklet(event);
  assert.deepEqual(calls, ['ui:Última correção 🪷']);
  field.change({ nativeEvent: event });
  assert.deepEqual(calls, [
    'ui:Última correção 🪷',
    'change:Última correção 🪷',
    'text:Última correção 🪷',
  ]);
  assert.deepEqual(field.internal, { text: event.text, count: 8 });
  field.detach();
});

test('the real RN callback can arrive after compiled Send without restoring its accepted text', async () => {
  const adapter = await nativeEventAdapter();
  const f = fixture();
  let draft = 'A native corre';
  let sending = false;
  const sent: string[] = [];
  const tree = f.render(undefined, {
    value: draft,
    onChangeText: (text: string) => {
      draft = text;
    },
    onSend: () => {
      if (sending) return;
      sending = true;
      sent.push(draft);
      draft = '';
    },
  });
  const input = find(tree, byId('message-input'));
  const { onNativeChange, ...props } = input.props;
  const field = adapter.connect(props, onNativeChange as Handler);
  const correction = { text: 'A native correction ✓', eventCount: 12 };
  adapter.registrations[0]!.worklet(correction);
  assert.equal(draft, 'A native corre');
  const send = find(tree, byId('send-message')).props.onPress as Handler;
  send();
  send();
  assert.deepEqual(sent, [correction.text]);
  field.change({ nativeEvent: correction });
  assert.equal(draft, '');
  assert.deepEqual(field.internal, { text: correction.text, count: 12 });
  const next = { text: correction.text, eventCount: 13 };
  adapter.registrations[0]!.worklet(next);
  field.change({ nativeEvent: next });
  assert.equal(draft, next.text);
  assert.deepEqual(field.internal, { text: next.text, count: 13 });
  field.detach();
});
