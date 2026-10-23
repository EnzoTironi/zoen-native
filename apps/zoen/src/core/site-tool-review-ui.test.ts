import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, nodes, settle } from './tool-review-ui.fixture.ts';

test('actual Site review retires an in-flight signed proposal on account change and ignores late HTML', async () => {
  const f = fixture();
  const ready: boolean[] = [];
  let finish!: (value: any) => void;
  f.reads(
    () =>
      new Promise<any>((resolve) => {
        finish = resolve;
      }),
  );
  f.site((value) => ready.push(value));
  await settle();
  f.state.snapshot.account.identity_id = 'foreign-owner';
  for (const fn of f.listeners) fn();
  finish({
    request_id: 'site-request',
    content_hash: 'ab'.repeat(32),
    title: 'Private Site',
    version: 1,
    before_html: null,
    after_html: '<h1>Private</h1>',
  });
  await settle();
  assert.ok(!ready.includes(true));
  f.close();
  assert.equal(f.preflight.size, 0);
  assert.equal(f.canonical.size, 0);
});

test('actual Site review waits for its native load callback and retires late callbacks on preflight', async () => {
  const f = fixture();
  const ready: boolean[] = [];
  f.reads(
    async () =>
      ({
        request_id: 'site-request',
        content_hash: 'ab'.repeat(32),
        title: 'Private Site',
        version: 1,
        before_html: null,
        after_html: '<h1>Private</h1>',
      }) as any,
  );
  f.site((value) => ready.push(value));
  await settle();
  assert.equal(
    ready.at(-1),
    false,
    'verified bytes and module availability do not prove a loaded preview',
  );
  const preview = nodes(f.site((value) => ready.push(value))).find(
    (n) => n.type === 'SitePreview',
  )!;
  preview.props.onPreviewStatus({ nativeEvent: { ownerKey: 'foreign-view', ready: true } });
  assert.equal(ready.at(-1), false);
  preview.props.onPreviewStatus({ nativeEvent: { ownerKey: preview.props.ownerKey, ready: true } });
  assert.equal(ready.at(-1), true);
  preview.props.onPreviewStatus({
    nativeEvent: { ownerKey: preview.props.ownerKey, ready: false },
  });
  assert.equal(ready.at(-1), false);
  preview.props.onPreviewStatus({ nativeEvent: { ownerKey: preview.props.ownerKey, ready: true } });
  for (const fn of f.preflight) fn();
  assert.equal(ready.at(-1), false);
  preview.props.onPreviewStatus({ nativeEvent: { ownerKey: preview.props.ownerKey, ready: true } });
  assert.equal(ready.at(-1), false, 'a queued old native callback cannot restore retired consent');
  f.close();
});

test('request card cannot approve a Site before its isolated exact proposal preview is ready', () => {
  const f = fixture();
  const request = f.base()[0];
  request.status = 'Pending';
  request.tool_approval.expires_ms = String(Date.now() + 60_000);
  request.tool_approval.preview_kind = 'site';
  request.space_id = 'site-space';
  request.agent.name = 'Agent';
  request.opened_ms = '1';
  const card = f.card(request);
  const buttons = nodes(card).filter((n) => n.type === 'NativeButton');
  assert.equal(buttons.find((n) => n.props.label === 'agents.allowOnce')?.props.disabled, true);
  assert.equal(buttons.find((n) => n.props.label === 'agents.doNotAllow')?.props.disabled, false);
  f.close();
});

test('switching Before and Proposed requires a new current native preview and rejects queued callbacks', async () => {
  const f = fixture();
  const ready: boolean[] = [];
  const onReady = (value: boolean) => ready.push(value);
  f.reads(
    async () =>
      ({
        request_id: 'site-request',
        content_hash: 'ab'.repeat(32),
        title: 'Site',
        version: 1,
        before_html: '<h1>Before</h1>',
        after_html: '<h1>Proposed</h1>',
      }) as any,
  );
  f.site(onReady);
  await settle();
  const original = nodes(f.site(onReady)).find((n) => n.type === 'SitePreview')!;
  const loaded = (view: typeof original) =>
    view.props.onPreviewStatus({ nativeEvent: { ownerKey: view.props.ownerKey, ready: true } });
  loaded(original);
  assert.equal(ready.at(-1), true);
  nodes(f.site(onReady))
    .find((n) => n.props.label === 'sites.reviewBefore')!
    .props.onPress();
  assert.equal(ready.at(-1), false);
  const before = nodes(f.site(onReady)).find((n) => n.type === 'SitePreview')!;
  loaded(original);
  loaded(before);
  assert.equal(ready.at(-1), false, 'the old version cannot be the proposed-version review');
  nodes(f.site(onReady))
    .find((n) => n.props.label === 'sites.reviewAfter')!
    .props.onPress();
  const proposed = nodes(f.site(onReady)).find((n) => n.type === 'SitePreview')!;
  assert.notEqual(proposed.props.ownerKey, original.props.ownerKey);
  loaded(original);
  assert.equal(ready.at(-1), false);
  loaded(proposed);
  assert.equal(ready.at(-1), true);
  f.close();
});
