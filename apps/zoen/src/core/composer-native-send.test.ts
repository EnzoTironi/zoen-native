import assert from 'node:assert/strict';
import test from 'node:test';
import { byId, find, fixture, type Handler } from './attachment-composer-ui.fixtures.ts';

test('Send captures the final native correction before its JS delivery and keeps the keyboard open', () => {
  const f = fixture();
  let draft = 'Send teh';
  const sent: string[] = [];
  let blurred = false;
  const tree = f.render(undefined, {
    value: draft,
    onChangeText: (text: string) => {
      draft = text;
    },
    onBlur: () => {
      blurred = true;
    },
    onSend: () => {
      sent.push(draft);
      draft = '';
    },
  });
  const input = find(tree, byId('message-input'));
  f.nativeChange(input, 'Send the 🪷', 7);
  assert.equal(draft, 'Send teh', 'the native correction has not reached the draft writer');
  (find(tree, byId('send-message')).props.onPress as Handler)();
  assert.deepEqual(sent, ['Send the 🪷']);
  assert.equal(draft, '');
  f.drainNativeEvents();
  assert.equal(draft, '', 'late JS delivery cannot recreate the accepted message');
  assert.equal(blurred, false);
  const next = f.render(undefined, {
    value: draft,
    onChangeText: (text: string) => {
      draft = text;
    },
  });
  f.nativeChange(find(next, byId('message-input')), 'Send the 🪷', 8);
  f.drainNativeEvents();
  assert.equal(draft, 'Send the 🪷', 'retyping the same text with a later event is legitimate');
});

for (const changed of [
  'identity',
  'device',
  'epoch',
  'relay',
  'thread',
  'edit',
  'caption',
  'search',
] as const)
  test(`a ${changed} handoff and ABA cannot borrow the old UI receipt or Send action`, () => {
    const f = fixture();
    const originalAccount = { ...f.client.snapshot.account };
    const epoch = f.client.store.sessionEpoch;
    const initial = f.render();
    f.nativeChange(find(initial, byId('message-input')), 'Private native text', 10);
    let patch: Record<string, unknown> = {};
    if (changed === 'identity') f.client.snapshot.account.identity_id = 'other';
    if (changed === 'device') f.client.snapshot.account.device_id = 'other';
    if (changed === 'relay') f.client.snapshot.account.relay_url = 'https://other.example';
    if (changed === 'epoch') f.client.store.sessionEpoch++;
    if (changed === 'thread') patch.threadRootId = 'another-thread';
    if (changed === 'edit')
      patch = { editing: true, context: { entry: { id: 'another-message' }, onCancel() {} } };
    if (changed === 'search')
      patch.search = {
        state: { query: 'query', hits: [], completed: false },
        onClose() {},
        onMove() {},
        onAsk() {},
        onSearch() {},
      };
    f.render(changed === 'caption' ? f.file : undefined, patch);
    (find(initial, byId('send-message')).props.onPress as Handler)();
    f.drainNativeEvents();
    assert.deepEqual(f.calls, []);
    Object.assign(f.client.snapshot.account, originalAccount);
    f.client.store.sessionEpoch = epoch;
    const returned = f.render();
    f.nativeChange(find(returned, byId('message-input')), 'Current native text', 12);
    f.nativeChange(find(initial, byId('message-input')), 'Retired native callback', 99);
    (find(initial, byId('send-message')).props.onPress as Handler)();
    assert.deepEqual(f.calls, []);
    (find(returned, byId('send-message')).props.onPress as Handler)();
    assert.deepEqual(f.calls, ['normal:Current native text', 'send-normal']);
    f.drainNativeEvents();
    assert.deepEqual(f.calls, ['normal:Current native text', 'send-normal']);
  });

test('a consumed native receipt cannot replace newer programmatic text or reply metadata', () => {
  const f = fixture();
  const first = f.render();
  f.nativeChange(find(first, byId('message-input')), 'First native value', 1);
  f.drainNativeEvents();
  assert.deepEqual(f.calls, ['normal:First native value']);
  f.render(undefined, {
    value: 'A later controlled draft',
    context: { entry: { id: 'reply' }, onCancel() {} },
    onSend: () => f.calls.push('current-send'),
  });
  (find(first, byId('send-message')).props.onPress as Handler)();
  assert.deepEqual(f.calls, ['normal:First native value', 'current-send']);
});

test('readonly and unmounted editors cannot capture or send a native receipt', () => {
  for (const retired of ['readonly', 'unmounted']) {
    const f = fixture();
    const first = f.render();
    f.nativeChange(find(first, byId('message-input')), 'Unsent native text', 1);
    if (retired === 'readonly') f.render(undefined, { draft: { ready: false } });
    else f.unmount();
    (find(first, byId('send-message')).props.onPress as Handler)();
    f.drainNativeEvents();
    assert.deepEqual(f.calls, [], retired);
  }
});
