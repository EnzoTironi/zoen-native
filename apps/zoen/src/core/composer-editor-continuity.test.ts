import assert from 'node:assert/strict';
import test from 'node:test';
import { byId, find, fixture, styleOf, type Handler } from './attachment-composer-ui.fixtures.ts';

const tick = () => new Promise<void>((done) => setImmediate(done));

test('a retired native layout cannot schedule fitting for the replacement session', () => {
  const f = fixture();
  const first = find(f.render(), byId('message-input'));
  f.client.store.sessionEpoch++;
  f.render();
  const before = f.measurements.length;
  (first.props.onLayout as Handler)({ nativeEvent: { layout: { width: 117, height: 40 } } });
  f.render();
  assert.ok(f.measurements.slice(before).every((measurement) => measurement.width !== 117));
});

for (const changed of ['identity', 'device', 'epoch', 'relay'] as const)
  test(`same-Space ${changed} replacement and ABA retire native callbacks without borrowing the new draft`, () => {
    const f = fixture();
    const account = f.client.snapshot.account;
    const originalAccount = { ...account };
    const originalEpoch = f.client.store.sessionEpoch;
    const first = find(f.render(), byId('message-input'));
    if (changed === 'identity') account.identity_id = 'other-person';
    if (changed === 'device') account.device_id = 'other-device';
    if (changed === 'relay') account.relay_url = 'https://other-relay.example';
    if (changed === 'epoch') f.client.store.sessionEpoch++;
    const second = find(
      f.render(undefined, { onChangeText: (text: string) => f.calls.push(`new:${text}`) }),
      byId('message-input'),
    );
    (first.props.onChangeText as Handler)('old native event');
    assert.deepEqual(f.calls, []);
    (second.props.onChangeText as Handler)('Live new draft');
    assert.deepEqual(f.calls, ['new:Live new draft']);
    Object.assign(account, originalAccount);
    f.client.store.sessionEpoch = originalEpoch;
    const returned = find(f.render(), byId('message-input'));
    (first.props.onChangeText as Handler)('retired original owner');
    (second.props.onChangeText as Handler)('retired second owner');
    assert.deepEqual(f.calls, ['new:Live new draft']);
    (returned.props.onChangeText as Handler)('Live returned draft');
    assert.deepEqual(f.calls, ['new:Live new draft', 'normal:Live returned draft']);
    assert.equal(returned.props.ref, first.props.ref);
  });

test('normal and caption callbacks cannot cross the mounted editor handoff or its return', () => {
  const f = fixture();
  const inputRef = { current: null as unknown };
  const normal = find(f.render(undefined, { inputRef }), byId('message-input'));
  const caption = find(f.render(f.file, { inputRef }), byId('message-input'));
  (normal.props.onChangeText as Handler)('late normal event');
  assert.deepEqual(f.calls, []);
  (caption.props.onChangeText as Handler)('Live caption');
  assert.deepEqual(f.calls, ['caption:Live caption']);
  const restored = find(f.render(undefined, { inputRef }), byId('message-input'));
  (normal.props.onChangeText as Handler)('retired normal generation');
  (caption.props.onChangeText as Handler)('late caption');
  assert.deepEqual(f.calls, ['caption:Live caption']);
  (restored.props.onChangeText as Handler)('Live normal draft');
  assert.deepEqual(f.calls, ['caption:Live caption', 'normal:Live normal draft']);
  assert.equal(normal.props.ref, restored.props.ref);
  assert.equal(inputRef.current, f.nativeField);
});

test('caption identity follows the selected mutation rather than its reused grid position', () => {
  const f = fixture();
  const first = find(f.render({ ...f.file, mutationId: 'first' }), byId('message-input'));
  const second = find(
    f.render({
      ...f.file,
      mutationId: 'second',
      onChange: (text: string) => f.calls.push(`second:${text}`),
    }),
    byId('message-input'),
  );
  (first.props.onChangeText as Handler)('late first caption');
  assert.deepEqual(f.calls, []);
  (second.props.onChangeText as Handler)('Live second caption');
  assert.deepEqual(f.calls, ['second:Live second caption']);
  f.render({
    ...f.file,
    mutationId: 'second',
    position: 2,
    onChange: (text: string) => f.calls.push(`reordered:${text}`),
  });
  (second.props.onChangeText as Handler)('same selected asset');
  assert.deepEqual(f.calls, ['second:Live second caption', 'reordered:same selected asset']);
});

test('editing A then B fences old text and focus callbacks without remounting', () => {
  const f = fixture();
  let edited = 'a';
  const changes: string[] = [];
  const props = {
    editing: true,
    onChangeText: (text: string) => changes.push(`${edited}:${text}`),
    onFocus: () => changes.push(`${edited}:focus`),
    onBlur: () => changes.push(`${edited}:blur`),
  };
  const first = find(
    f.render(undefined, { ...props, context: { entry: { id: 'a' }, onCancel() {} } }),
    byId('message-input'),
  );
  edited = 'b';
  const second = find(
    f.render(undefined, { ...props, context: { entry: { id: 'b' }, onCancel() {} } }),
    byId('message-input'),
  );
  (first.props.onChangeText as Handler)('late edit');
  (first.props.onFocus as Handler)();
  (first.props.onBlur as Handler)();
  assert.deepEqual(changes, []);
  (second.props.onChangeText as Handler)('Live edit');
  assert.deepEqual(changes, ['b:Live edit']);
  assert.equal(first.props.ref, second.props.ref);
  edited = 'a';
  const reopened = find(
    f.render(undefined, { ...props, context: { entry: { id: 'a' }, onCancel() {} } }),
    byId('message-input'),
  );
  (first.props.onChangeText as Handler)('retired A generation');
  (second.props.onChangeText as Handler)('retired B generation');
  assert.deepEqual(changes, ['b:Live edit']);
  (reopened.props.onChangeText as Handler)('Live reopened edit');
  assert.deepEqual(changes, ['b:Live edit', 'a:Live reopened edit']);
});

test('same-draft typing survives reply metadata, measured height and callback rerenders', async () => {
  const f = fixture();
  const original = find(f.render(), byId('message-input'));
  const values = ['T', 'Te', 'Text\nnext line', 'Text\nnext line 🪷'];
  for (const [index, value] of values.entries()) {
    const current = find(
      f.render(undefined, {
        value,
        context: { entry: { id: `reply-${index}` }, onCancel() {} },
        onChangeText: (text: string) => f.calls.push(`current:${text}`),
      }),
      byId('message-input'),
    );
    (original.props.onChangeText as Handler)(value);
    assert.equal(current.props.value, value);
    assert.equal(current.props.ref, original.props.ref);
    await tick();
  }
  assert.deepEqual(
    f.calls,
    values.map((value) => `current:${value}`),
  );
});

test('leaving search retires its submit action while keeping the mounted normal draft', () => {
  const f = fixture();
  const search = {
    state: { active: true, completed: false, loading: false, hits: [], index: -1, cursor: null },
    status: '',
    onClose() {},
    onMove() {},
    onAsk() {},
    onSearch: () => f.calls.push('search'),
  };
  const input = find(f.render(undefined, { search, value: 'Query' }), byId('message-input'));
  (input.props.onSubmitEditing as Handler)({});
  assert.deepEqual(f.calls, ['search']);
  const normal = find(f.render(), byId('message-input'));
  (input.props.onSubmitEditing as Handler)({});
  assert.deepEqual(f.calls, ['search']);
  assert.equal(normal.props.value, 'My unsent normal text');
  assert.equal(normal.props.ref, input.props.ref);
});

test('readonly admission and unmount retire writes while same-context callbacks use current handlers', () => {
  const f = fixture();
  const original = find(f.render(), byId('message-input'));
  f.render(undefined, { onChangeText: (text: string) => f.calls.push(`current:${text}`) });
  (original.props.onChangeText as Handler)('current draft');
  assert.deepEqual(f.calls, ['current:current draft']);
  const readonly = find(f.render(f.file, { draft: { ready: false } }), byId('message-input'));
  f.render({ ...f.file, editable: false });
  (readonly.props.onChangeText as Handler)('late readonly event');
  assert.deepEqual(f.calls, ['current:current draft']);
  f.unmount();
  (readonly.props.onChangeText as Handler)('late unmounted event');
  assert.deepEqual(f.calls, ['current:current draft']);
});

test('multiline growth retains validated geometry until the next same-editor fit without borrowing another mode', async () => {
  const f = fixture();
  f.deferFits();
  f.render(undefined, { value: 'First\nsecond\nthird' });
  const initial = f.pendingFits.at(-1)!;
  initial.resolve({ ...initial.request, height: 118 });
  await tick();
  let input = find(f.render(undefined, { value: 'First\nsecond\nthird' }), byId('message-input'));
  assert.equal(styleOf(input).height, 118);
  input = find(
    f.render(undefined, { value: 'First\nsecond\nthird\nfourth' }),
    byId('message-input'),
  );
  assert.equal(styleOf(input).height, 118, 'a pending fit must not collapse the mounted editor');
  const next = f.pendingFits.at(-1)!;
  next.resolve({ ...next.request, height: 143 });
  await tick();
  input = find(
    f.render(undefined, { value: 'First\nsecond\nthird\nfourth' }),
    byId('message-input'),
  );
  assert.equal(styleOf(input).height, 143);
  const caption = find(f.render({ ...f.file, value: 'Caption' }), byId('message-input'));
  assert.notEqual(styleOf(caption).height, 143, 'another destination owns a different fit');
  const empty = find(f.render(undefined, { value: '' }), byId('message-input'));
  assert.equal(styleOf(empty).height, 40, 'clearing restores the single-line minimum immediately');
});
