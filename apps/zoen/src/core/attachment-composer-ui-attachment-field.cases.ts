import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { Handler } from './attachment-composer-ui.fixtures.ts';
import { byId, find, fixture, scan } from './attachment-composer-ui.fixtures.ts';

test('native attachment mode uses the real message field and send button without posting the normal text draft', () => {
  const f = fixture();
  const root = f.render(f.file);
  const input = find(root, byId('message-input'));
  assert.equal(input.props.value, '');
  assert.equal(input.props.editable, true);
  assert.equal(
    f.measurements.at(-1)!.text,
    '',
    'actual compiled field requests native caption fitting',
  );
  (input.props.onChangeText as Handler)('First photo caption');
  const send = find(root, byId('send-attachments'));
  assert.equal(send.props.disabled, false, 'an attachment is sendable without a caption');
  (send.props.onPress as Handler)();
  assert.deepEqual(f.calls, ['caption:First photo caption', 'send-files']);
  assert.equal(find(f.render(), byId('message-input')).props.value, 'My unsent normal text');
});

for (const platform of ['web', 'android'] as const) {
  test(`${platform} mounts, changes mode and detaches the real input ref without iOS tag lookup or fitting`, () => {
    const f = fixture(platform);
    const inputRef = { current: null as unknown };
    const first = find(f.render(undefined, { inputRef }), byId('message-input'));
    assert.equal(inputRef.current, f.nativeField);
    assert.equal(first.props.value, 'My unsent normal text');
    const editing = find(
      f.render(undefined, { inputRef, editing: true, value: 'Edit\nsecond line' }),
      byId('message-input'),
    );
    assert.equal(inputRef.current, f.nativeField, 'mode changes preserve the mounted editor');
    assert.equal(editing.props.ref, first.props.ref);
    assert.equal(editing.props.value, 'Edit\nsecond line');
    const restored = find(f.render(undefined, { inputRef }), byId('message-input'));
    assert.equal(restored.props.value, 'My unsent normal text');
    (restored.props.onChangeText as Handler)('Still editable');
    (restored.props.onContentSizeChange as Handler)({
      nativeEvent: { contentSize: { width: 264, height: 74 } },
    });
    assert.deepEqual(f.calls, ['normal:Still editable']);
    assert.equal(f.nativeHandleReads(), 0);
    assert.equal(f.measurements.length, 0);
    f.unmount();
    assert.equal(inputRef.current, null);
    assert.equal(f.nativeHandleReads(), 0, 'detaching must not invoke native tag lookup either');
  });
}

test('ambiguous attachment retry keeps the caption readonly and sends only through the batch owner', () => {
  const f = fixture();
  const root = f.render({
    ...f.file,
    editable: false,
    targetFrozen: true,
    value: 'Exact attempted caption',
    retry: true,
    remaining: 1,
  });
  assert.equal(find(root, byId('message-input')).props.editable, false);
  const send = find(root, byId('send-attachments'));
  assert.equal(send.props.label, 'media.attachmentsRetry');
  (send.props.onPress as Handler)();
  assert.deepEqual(f.calls, ['send-files']);
});

test('attachment reply UI displays the frozen batch target rather than a later normal-draft reply', () => {
  const f = fixture();
  const scope = { ...f.file.scope, replyTo: 'captured-original' };
  const context = { entry: { id: 'later-reply' }, onCancel: () => {} };
  const root = f.render({ ...f.file, scope, targetFrozen: true }, { context });
  const quote = find(root, (node) => node.type === 'AttachmentReplyContext');
  assert.equal((quote.props.scope as typeof scope).replyTo, 'captured-original');
  assert.equal(quote.props.onCancel, undefined);
  assert.equal(
    scan(root).some((node) => node.type === 'MessageContext'),
    false,
  );
  assert.equal(
    find(f.render(undefined, { context }), (node) => node.type === 'MessageContext').props.entry,
    context.entry,
  );
});

test('search takes over the message field while attachment drafts stay outside its action path', () => {
  const f = fixture();
  const search = {
    state: { active: true, completed: true, loading: false, hits: [{}], index: 0, cursor: null },
    status: '1/1',
    onClose: () => {},
    onMove: () => {},
    onAsk: () => {},
    onSearch: () => {},
  };
  const root = f.render(f.file, { search, value: 'Search text' });
  assert.equal(find(root, byId('message-input')).props.value, 'Search text');
  assert.equal(find(root, byId('message-input')).props.autoCorrect, false);
  assert.equal(find(root, byId('message-input')).props.autoCapitalize, 'none');
  assert.equal(scan(root).some(byId('send-attachments')), false);
  assert.ok(find(root, byId('older-search-result')));
  const normal = find(f.render(), byId('message-input'));
  assert.equal(normal.props.autoCorrect, undefined, 'Normal messages keep native correction');
  assert.equal(normal.props.autoCapitalize, undefined, 'Normal messages keep sentence casing');
});

test('a sent file caption can be cleared without allowing empty ordinary text edits', () => {
  const f = fixture();
  const emptyCaption = f.render(undefined, { value: '', editing: true, allowEmptyEdit: true });
  assert.equal(find(emptyCaption, byId('send-message')).props.disabled, false);
  assert.equal(
    (find(emptyCaption, byId('message-input')).props.style as { height?: number }[]).at(-1)!.height,
    40,
  );
  const emptyText = f.render(undefined, { value: '', editing: true });
  assert.equal(find(emptyText, byId('send-message')).props.disabled, true);
});

test('an unread native draft cannot be overwritten by an initially empty field; retry stays visible', () => {
  const f = fixture();
  const draft = { ready: false, error: true, onRetry: () => f.calls.push('retry-draft') };
  const root = f.render(undefined, { value: '', draft });
  assert.equal(find(root, byId('message-input')).props.editable, false);
  const status = f.renderRoot({ draft });
  (find(status, byId('retry-message-draft')).props.onPress as Handler)();
  assert.deepEqual(f.calls, ['retry-draft']);
  const loaded = f.render(undefined, { draft: { ...draft, ready: true } });
  assert.equal(find(loaded, byId('message-input')).props.editable, true);
  assert.equal(find(loaded, byId('message-input')).props.value, 'My unsent normal text');
});
