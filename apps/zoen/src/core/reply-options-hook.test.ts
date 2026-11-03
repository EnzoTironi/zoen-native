import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { fixture, main, deferred, key } from './normal-message-drafts.fixtures.ts';
import { captureReplyOptions } from './reply-options.ts';
import { selectedMessageQuote } from './selected-message-quote.ts';
import { plainMessage } from './message-caption.fixtures.ts';

const require = createRequire(import.meta.url);
const file = new URL('../components/use-reply-options.ts', import.meta.url);
const compiled = require('@babel/core').transformSync(readFileSync(file, 'utf8'), {
  filename: file.pathname,
  babelrc: false,
  configFile: false,
  presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
  caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
}).code;

async function harness(threadRootId: string | null = null) {
  const f = fixture();
  const save = f.port.save;
  f.port.save = async (...args) => {
    const full = { ...(await save(...args)), reply_quote: args[4], shared_reply: args[5] };
    f.stored.set(key(args[0]), full);
    return full;
  };
  const scope = { ...main, threadRootId };
  const channel = { controller: f.drafts, scope };
  const source = {
    ...plainMessage('😀 exact **quotation**'),
    id: 'source',
    text_revision: 'signed',
  };
  const root = createReactTestRoot();
  await f.drafts.activate(scope);
  f.drafts.setReply(scope, source.id);
  f.drafts.setText(scope, 'Keep my answer');
  await f.drafts.flush(scope);
  const listeners = new Set<() => void>();
  const routes: any[] = [],
    requests: any[] = [];
  let focused = true,
    access = true,
    options: any;
  let read: () => Promise<any> = async () => source;
  let review: (q: any) => Promise<any> = async (q) => q;
  const store = {
    sessionEpoch: 1,
    getSnapshot: () => ({ timelines: { [scope.spaceId]: { entries: [source] } } }),
    subscribe: (f: () => void) => {
      listeners.add(f);
      return () => listeners.delete(f);
    },
    messageDrafts: { channel: () => channel, isCurrent: () => access },
    driver: {
      durableMessageDrafts: true,
      request: async (command: any) => {
        requests.push(command);
        if (command.op === 'message') return read();
        if (command.op === 'review_reply_selection') return review(command.selection);
        throw new Error(command.op);
      },
    },
  };
  const module = { exports: {} as any };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name === 'react') return React;
      if (name === 'react/compiler-runtime') return require(name);
      if (name.startsWith('@babel/runtime/')) return require(name);
      if (name === 'expo-router')
        return {
          router: { push: (route: any) => routes.push(route) },
          useFocusEffect: (work: () => any) => {
            const value = focused;
            React.useEffect(() => (value ? work() : undefined), [work, value]);
          },
        };
      if (name === '../core/provider') return { useClient: () => ({ store }) };
      if (name === '../core/reply-options') return { captureReplyOptions };
      if (name === '../core/selected-message-quote') return { selectedMessageQuote };
      if (name === '../home/access')
        return { homeSpacesFor: () => new Set(access ? [scope.spaceId] : []) };
      if (name === '../i18n') return { t: (key: string) => key };
      throw new Error(name);
    },
  });
  function Probe() {
    options = module.exports.useReplyOptions(scope.spaceId, threadRootId);
    return null;
  }
  const render = () => root.render(React.createElement(Probe));
  const tick = async () => {
    await React.act(async () => {
      await new Promise((resolve) => setImmediate(resolve));
    });
    await render();
  };
  await render();
  return {
    f,
    source,
    scope,
    store,
    routes,
    requests,
    root,
    tick,
    options: () =>
      new Proxy(options, {
        get(target, name) {
          const value = target[name];
          return typeof value === 'function'
            ? (...args: any[]) =>
                React.act(() => {
                  value(...args);
                })
            : value;
        },
      }),
    focus: async (value: boolean) => {
      focused = value;
      await render();
    },
    access: (value: boolean) => {
      React.act(() => {
        access = value;
        for (const listener of listeners) listener();
      });
    },
    read: (value: typeof read) => {
      read = value;
    },
    review: (value: typeof review) => {
      review = value;
    },
  };
}

for (const thread of [null, 'thread-root'])
  test(`reply options select, apply, remove and cancel while preserving text (${thread})`, async () => {
    const h = await harness(thread);
    try {
      h.options().open();
      await h.tick();
      assert.equal(h.options().entry.id, 'source');
      h.options().select();
      await h.tick();
      assert.equal(h.options().selecting, true);
      h.options().review('exact', JSON.stringify([{ start_utf16: 3, end_utf16: 8 }]));
      await h.tick();
      assert.equal(h.options().selection.text, 'exact');
      assert.equal(h.requests.at(-1).thread_root_id, thread);
      assert.equal(
        h.f.drafts.getSnapshot(h.scope).replyQuote,
        null,
        'preview is not saved until Apply',
      );
      h.options().apply();
      await h.tick();
      assert.equal(h.f.current(h.scope).reply_quote?.text, 'exact');
      assert.equal(h.f.current(h.scope).text, 'Keep my answer');
      h.options().open();
      await h.tick();
      h.options().remove();
      await h.tick();
      assert.equal(h.f.current(h.scope).reply_to, 'source');
      assert.equal(h.f.current(h.scope).reply_quote, null);
      h.options().open();
      await h.tick();
      h.options().cancelReply();
      await h.tick();
      assert.equal(h.f.current(h.scope).reply_to, null);
      assert.equal(h.f.current(h.scope).text, 'Keep my answer');
      assert.equal(h.f.posts.length, 0);
    } finally {
      await h.root.close();
    }
  });

test('moving a selected thread reply flushes the source and navigates with identifiers only', async () => {
  const h = await harness('thread-root');
  try {
    h.options().open();
    await h.tick();
    h.options().review('exact', JSON.stringify([{ start_utf16: 3, end_utf16: 8 }]));
    await h.tick();
    h.options().move();
    await h.tick();
    assert.equal(h.routes.length, 0, 'navigation waits for the presentation dismissal');
    h.options().close();
    await h.tick();
    assert.equal(h.routes.length, 1);
    const params = h.routes[0].params;
    assert.equal(params.draftThread, 'thread-root');
    assert.equal(params.draftRevision, h.f.current(h.scope).revision);
    assert.equal(JSON.stringify(params).includes('exact'), false);
    assert.equal(h.f.current(h.scope).reply_quote?.text, 'exact');
    assert.equal(h.f.posts.length, 0);
  } finally {
    await h.root.close();
  }
});

test('blur, revoke/restore and newer replies prevent stale async selection from reviving', async () => {
  for (const change of ['blur', 'access', 'reply', 'source']) {
    const h = await harness();
    try {
      h.options().open();
      await h.tick();
      const pending = deferred<any>();
      h.review(() => pending.promise);
      h.options().review('exact', JSON.stringify([{ start_utf16: 3, end_utf16: 8 }]));
      await h.tick();
      if (change === 'blur') {
        await h.focus(false);
        await h.focus(true);
      }
      if (change === 'access') {
        h.access(false);
        h.access(true);
      }
      if (change === 'reply') {
        h.f.drafts.setReply(h.scope, 'newer');
        h.f.drafts.setReply(h.scope, 'source');
      }
      if (change === 'source') {
        h.source.text_revision = 'edited';
        h.access(true);
      }
      pending.resolve({
        source_id: 'source',
        revision: 'signed',
        text: 'exact',
        spans: [{ start_utf16: 3, end_utf16: 8 }],
      });
      await h.tick();
      assert.equal(h.f.drafts.getSnapshot(h.scope).replyQuote, null, change);
      assert.equal(h.options().selection, null, change);
      assert.equal(h.f.posts.length, 0);
    } finally {
      await h.root.close();
    }
  }
});

test('reply options cannot resume an ambiguous Send when opening or applying', async () => {
  const h = await harness();
  try {
    h.options().open();
    await h.tick();
    h.f.hooks.sent = async () => {
      throw new Error('Lost acknowledgement');
    };
    await React.act(async () => {
      await assert.rejects(h.f.drafts.send(h.scope));
    });
    const sends = h.f.calls.filter((call) => call.op === 'send').length;
    h.options().apply();
    await h.tick();
    h.options().open();
    await h.tick();
    assert.equal(h.f.calls.filter((call) => call.op === 'send').length, sends);
    assert.equal(h.options().entry, null);
    assert.equal(h.f.drafts.hasPendingSend(h.scope), true);
  } finally {
    await h.root.close();
  }
});
