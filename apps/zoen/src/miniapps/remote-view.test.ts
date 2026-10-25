import assert from 'node:assert/strict';
import test from 'node:test';
import { deferred } from './remote-session.fixture.ts';
import { externalLaunch, remoteViewFixture } from './remote-view.fixture.ts';

test('actual compiled screen opens a host-configured external release through its existing button and frame', async () => {
  const f = await remoteViewFixture();
  try {
    const launch = externalLaunch();
    await f.render({ launch });
    launch.arguments.document = 'mutated after opening';
    const button = f.button();
    assert.ok(button, 'A trusted configured release must reach the real screen launch control');
    assert.equal(button.disabled, false);
    assert.equal(f.provider.requests.length, 0, 'mounting does not contact the provider');
    await f.act(() => {
      button.onPress();
      button.onPress();
    });
    await f.settle();
    const frame = f.frame();
    assert.ok(frame);
    assert.equal(frame.title, 'External editor');
    assert.ok(frame.html.includes('Provider app'));
    assert.ok(
      frame.html.includes('Content-Security-Policy'),
      'the real frame receives policy-constrained HTML',
    );
    const calls = f.provider.requests.filter((r) => r.method === 'tools/call');
    assert.equal(calls.length, 1, 'same activation cannot launch twice');
    assert.deepEqual(calls[0]!.params, { name: 'paint', arguments: externalLaunch().arguments });
    assert.deepEqual(
      f.coreCalls,
      [],
      'a remote session cannot create an alternate canonical document',
    );
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('real screen AppBridge receives launch data and confirms even self-reported read-only server tools', async () => {
  const f = await remoteViewFixture();
  try {
    await f.render();
    await f.act(() => f.button()!.onPress());
    await f.settle();
    const app = await f.connectApp();
    assert.equal(app.getHostContext()?.displayMode, 'fullscreen');
    const initial = f.sent.find((m: any) => m.method === 'ui/notifications/tool-input') as any;
    assert.deepEqual(initial.params.arguments, externalLaunch().arguments);
    const allowed = await app.callServerTool({ name: 'query', arguments: { q: 'synthetic' } });
    assert.equal(allowed.isError, undefined);
    assert.equal(
      f.confirmations.length,
      1,
      'provider readOnlyHint cannot bypass host confirmation',
    );
    f.decide(false);
    await assert.rejects(app.callServerTool({ name: 'save', arguments: { text: 'not approved' } }));
    assert.equal(f.confirmations.length, 2);
    assert.deepEqual(
      f.provider.requests.filter((r) => r.method === 'tools/call').map((r) => r.params?.name),
      ['paint', 'query'],
    );
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('changed release bytes never reach the real frame or invoke its launch tool', async () => {
  const f = await remoteViewFixture({ resource: { text: '<!doctype html><p>Changed app</p>' } });
  try {
    await f.render();
    await f.act(() => f.button()!.onPress());
    await f.settle();
    assert.equal(f.frame(), null);
    assert.equal(f.frameHistory.length, 0);
    assert.equal(f.provider.requests.filter((r) => r.method === 'tools/call').length, 0);
    assert.ok(
      f.alerts.some((message) => String(message).includes('release bytes or policy changed')),
    );
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('route strings cannot install releases and malformed host arguments fail before contacting a provider', async () => {
  const f = await remoteViewFixture();
  try {
    await f.render({ id: JSON.stringify(externalLaunch()) });
    assert.equal(f.button(), undefined);
    assert.equal(f.provider.requests.length, 0);
    const launch = { ...externalLaunch(), arguments: { text: 'x'.repeat(1_000_001) } };
    await f.render({ launch });
    assert.equal(f.button(), undefined);
    assert.equal(f.frame(), null);
    assert.equal(f.provider.requests.length, 0);
    assert.ok(f.alerts.some((message) => String(message).includes('limit')));
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('account closure during actual resource admission cannot publish a late frame or launch', async () => {
  const gate = deferred<void>(),
    f = await remoteViewFixture({ readWait: gate.promise });
  try {
    await f.render();
    await f.act(() => f.button()!.onPress());
    await f.settle();
    assert.equal(f.provider.requests.filter((r) => r.method === 'resources/read').length, 1);
    await f.act(() => f.store.close());
    gate.resolve();
    await f.settle();
    assert.equal(f.frameHistory.length, 0);
    assert.equal(f.provider.requests.filter((r) => r.method === 'tools/call').length, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    gate.resolve();
    await f.close();
  }
});

test('replacing trusted configuration retires the old opening before its resource returns', async () => {
  const gate = deferred<void>(),
    f = await remoteViewFixture({ readWait: gate.promise });
  try {
    await f.render();
    await f.act(() => f.button()!.onPress());
    await f.settle();
    const next = externalLaunch();
    const launch = {
      ...next,
      binding: { ...next.binding, release: '2026.10.26' },
      arguments: { document: 'new opening' },
    };
    await f.render({ launch });
    await f.act(() => f.button()!.onPress());
    await f.settle();
    gate.resolve();
    await f.settle();
    const calls = f.provider.requests.filter((r) => r.method === 'tools/call');
    assert.equal(calls.length, 1);
    assert.deepEqual(calls[0]!.params?.arguments, launch.arguments);
    assert.ok(f.frame());
    assert.equal(f.frameHistory.length, 1);
    assert.deepEqual(f.errors, []);
  } finally {
    gate.resolve();
    await f.close();
  }
});

test('blur disposes the actual frame and stale screen callbacks cannot make new requests', async () => {
  const f = await remoteViewFixture();
  try {
    await f.render();
    const oldButton = f.button()!;
    await f.act(() => oldButton.onPress());
    await f.settle();
    const oldFrame = f.frame()!;
    await f.blur();
    assert.equal(f.frame(), null);
    assert.ok(f.disposals() > 0);
    const before = f.provider.requests.length;
    await f.act(() => {
      oldButton.onPress();
      oldFrame.onMessage(
        JSON.stringify({
          jsonrpc: '2.0',
          id: 'stale',
          method: 'tools/call',
          params: { name: 'save', arguments: {} },
        }),
      );
    });
    await f.settle();
    assert.equal(f.provider.requests.length, before);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('legacy Excalidraw route uses the same real screen opening and retains its scoped presentation', async () => {
  const uri = 'ui://excalidraw/mcp-app.html';
  const f = await remoteViewFixture({
    endpoint: 'https://mcp.excalidraw.com/mcp',
    toolList: [
      {
        name: 'create_view',
        inputSchema: { type: 'object' },
        _meta: { ui: { resourceUri: uri, visibility: ['model'] } },
      },
    ],
    resource: { uri },
    toolResult: { content: [], structuredContent: { checkpointId: 'synthetic-checkpoint' } },
  });
  try {
    await f.render({ id: 'excalidraw' });
    assert.ok(f.button());
    await f.act(() => f.button()!.onPress());
    await f.settle();
    assert.equal(f.frame()?.title, 'Excalidraw');
    assert.equal(f.provider.requests.filter((r) => r.method === 'tools/call').length, 1);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
