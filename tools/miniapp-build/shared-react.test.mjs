import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import '../../apps/zoen/src/i18n/test-setup.ts';
import { MiniappSession } from '../../apps/zoen/src/miniapps/protocol.ts';
import { JSDOM, VirtualConsole } from '../../packages/zoen-ui/node_modules/jsdom/lib/api.js';

const { buildApp } = await import(
  process.env.ZOEN_MINIAPP_BUILDER_MODULE ?? new URL('./build.mjs', import.meta.url)
);

const schedule = fileURLToPath(new URL('../../miniapps/schedule', import.meta.url));

test('production Schedule and shared UI use one React dispatcher through the real SDK', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'zoen-shared-react-'));
  let dom;
  let session;
  try {
    const manifest = await buildApp(schedule, directory);
    const html = await readFile(join(directory, `${manifest.id}.html`), 'utf8');
    const messages = [];
    const failures = [];
    const console = new VirtualConsole();
    console.on('jsdomError', (error) => failures.push(error.message));
    console.on('error', (...values) => failures.push(values.map(String).join(' ')));
    let send;
    const nativeRequests = [];
    const members = [
      { id: 'alice', name: 'QA owner', initials: 'QA', color: '#557755', isMe: true },
      { id: 'bob', name: 'QA member before', initials: 'QB', color: '#557755', isMe: false },
    ];
    const state = {
      title: 'QA hosted schedule',
      business: 'QA hosted schedule',
      timezone: 'America/Sao_Paulo',
      currency: 'BRL',
      services: [{ id: 'session', name: 'QA shared service', minutes: 50, priceCents: 9000 }],
      weeklyHours: Object.fromEntries(
        [0, 1, 2, 3, 4, 5, 6].map((day) => [day, [['09:00', '12:00']]]),
      ),
      slotMinutes: 30,
      horizonDays: 60,
      configured: true,
      busy: [],
      bookings: [
        {
          id: 'booking',
          serviceId: 'session',
          serviceName: 'QA member booking',
          minutes: 50,
          startMs: Date.parse('2026-10-19T12:00:00Z'),
          endMs: Date.parse('2026-10-19T12:50:00Z'),
          byId: 'bob',
          status: 'pending',
        },
      ],
      events: [
        {
          id: 'event',
          kind: 'book_request',
          bookingId: 'booking',
          status: 'open',
          at: Date.parse('2026-10-19T08:00:00Z'),
          by: 'bob',
        },
      ],
      viewer: { id: 'alice', role: 'owner' },
      next: 2,
    };
    dom = new JSDOM(html, {
      runScripts: 'dangerously',
      pretendToBeVisual: true,
      virtualConsole: console,
      beforeParse(window) {
        window.Date.now = () => Date.parse('2026-10-19T08:00:00Z');
        window.matchMedia = () => ({
          matches: false,
          addEventListener() {},
          removeEventListener() {},
        });
        window.addEventListener('error', (event) => {
          failures.push(event.error?.message || event.message);
          event.preventDefault();
        });
        window.ResizeObserver = class {
          observe() {}
          disconnect() {}
        };
        const parent = {
          postMessage(value) {
            const message = JSON.parse(typeof value === 'string' ? value : JSON.stringify(value));
            messages.push(message);
            void session.receive(JSON.stringify(message));
          },
        };
        send = (message) =>
          window.dispatchEvent(
            new window.MessageEvent('message', { data: message, source: parent }),
          );
        session = new MiniappSession({
          send,
          initialize: () => ({
            hostInfo: { name: 'zoen', version: 'qa' },
            hostCapabilities: {
              serverTools: {},
              message: { text: {} },
              experimental: {
                zoen: { native: ['calendar.freebusy'], services: ['state', 'members'] },
              },
            },
            hostContext: { locale: 'en', theme: 'light', zoen: { members } },
          }),
          state: () => ({ content: [], structuredContent: state }),
          context: () => ({ locale: 'en', theme: 'light', zoen: { members } }),
          tools: new Set(),
          resourceUri: 'ui://zoen/schedule',
          call: async () => {
            throw new Error('Unexpected shared mutation');
          },
          resource: async () => ({ contents: [] }),
          share: async () => {
            throw new Error('Unexpected chat send');
          },
          openLink: async () => ({}),
          native: async (capability, params) => {
            assert.equal(capability, 'calendar.freebusy');
            return new Promise((resolve, reject) =>
              nativeRequests.push({ params, resolve, reject }),
            );
          },
          haptic() {},
          resize() {},
          close() {},
        });
        Object.defineProperty(window, 'parent', { value: parent });
      },
    });
    const body = () => dom.window.document.getElementById('root').textContent;
    const waitFor = async (predicate, label) => {
      const deadline = Date.now() + 3000;
      while (!predicate()) {
        assert.deepEqual(failures, [], `Production bundle error while waiting for ${label}`);
        assert.ok(Date.now() < deadline, `Production bundle never rendered ${label}`);
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
      assert.deepEqual(failures, [], `Production bundle error after ${label}`);
    };
    const click = (label) => {
      const button = [...dom.window.document.querySelectorAll('button')].find(
        (item) => item.textContent.trim() === label,
      );
      assert.ok(button, `Missing real shared UI button: ${label}`);
      button.click();
    };
    const reads = () =>
      messages.filter((message) => message.method === 'zoen/native/calendar.freebusy');
    const slots = () => [...dom.window.document.querySelectorAll('.slot')];
    await waitFor(
      () => body().includes('QA shared service') && body().includes('QA member before'),
      'shared state/member hooks',
    );
    assert.equal(reads().length, 0, 'initialization must not read the calendar');
    send({
      jsonrpc: '2.0',
      method: 'ui/notifications/host-context-changed',
      params: {
        theme: 'light',
        zoen: {
          members: members.map((member) =>
            member.id === 'bob' ? { ...member, name: 'QA member after' } : member,
          ),
        },
      },
    });
    send({
      jsonrpc: '2.0',
      method: 'ui/notifications/tool-result',
      params: { structuredContent: { ...state, business: 'QA state update' } },
    });
    await waitFor(
      () => body().includes('QA member after') && body().includes('QA state update'),
      'real SDK subscriptions',
    );
    click('Check my availability');
    await waitFor(() => reads().length === 1, 'explicit calendar request');
    assert.deepEqual(Object.keys(reads()[0].params).sort(), ['endMs', 'startMs']);
    nativeRequests[0].reject(new Error('Synthetic calendar denied'));
    await waitFor(
      () =>
        body().includes('No filter is applied.') && body().includes('Synthetic calendar denied'),
      'denial without fabricated availability',
    );
    click('Try again');
    await waitFor(() => reads().length === 2, 'explicit retry');
    nativeRequests[1].resolve([]);
    await waitFor(
      () => body().includes('None of these business times conflict with events marked busy'),
      'successful empty calendar',
    );
    click('Check again');
    await waitFor(() => reads().length === 3, 'explicit refresh');
    nativeRequests[2].resolve([{ ...reads()[2].params }]);
    await waitFor(
      () =>
        slots().length > 0 &&
        slots().every((slot) => slot.disabled && slot.textContent.includes('Busy in my calendar')),
      'private busy intervals',
    );
    click('Remove calendar check');
    await waitFor(
      () => slots().length > 0 && slots().every((slot) => !slot.disabled),
      'restored business slots',
    );
    assert.equal(reads().length, 3, 'removal must not silently re-read the calendar');
    assert.equal(
      messages.filter((message) => ['tools/call', 'ui/message'].includes(message.method)).length,
      0,
      'private availability never publishes shared state or chat content',
    );
    assert.deepEqual(failures, []);
  } finally {
    session?.dispose();
    dom?.window.close();
    await rm(directory, { recursive: true, force: true });
  }
});

test('actual Zoen SDK uses standard App message arrays and the native result envelope', async () => {
  const { build } = await import('esbuild');
  const sdk = fileURLToPath(
    new URL('../../packages/zoen-miniapp-sdk/src/index.ts', import.meta.url),
  );
  const runtime = await build({
    stdin: {
      contents: `import zoen from ${JSON.stringify(sdk)}; globalThis.qaSdk = zoen;`,
      resolveDir: fileURLToPath(new URL('.', import.meta.url)),
    },
    nodePaths: [fileURLToPath(new URL('./node_modules', import.meta.url))],
    bundle: true,
    write: false,
    format: 'iife',
    platform: 'browser',
  });
  const shares = [];
  const native = [];
  let session;
  const dom = new JSDOM('<!doctype html><title>Official SDK test</title><body></body>', {
    runScripts: 'outside-only',
    pretendToBeVisual: true,
    beforeParse(window) {
      window.matchMedia = () => ({ matches: false });
      window.ResizeObserver = class {
        observe() {}
        disconnect() {}
      };
      const parent = {
        postMessage: (message) => {
          void session.receive(JSON.stringify(message));
        },
      };
      Object.defineProperty(window, 'parent', { value: parent });
      session = new MiniappSession({
        send: (message) =>
          window.dispatchEvent(
            new window.MessageEvent('message', { data: message, source: parent }),
          ),
        initialize: () => ({
          hostInfo: { name: 'zoen', version: 'qa' },
          hostCapabilities: {
            message: { text: {} },
            experimental: { zoen: { native: ['calendar.freebusy'] } },
          },
          hostContext: { theme: 'light' },
        }),
        state: () => ({ content: [] }),
        context: () => ({ theme: 'light' }),
        tools: new Set(),
        resourceUri: 'ui://zoen/test',
        resource: async () => ({ contents: [] }),
        call: async () => {
          throw new Error('No shared tool');
        },
        share: async (text) => {
          shares.push(text);
          return {};
        },
        openLink: async () => ({}),
        native: async (cap, params) => {
          native.push({ cap, params });
          return [{ startMs: 100, endMs: 200 }];
        },
        haptic() {},
        resize() {},
        close() {},
      });
    },
  });
  try {
    dom.window.eval(runtime.outputFiles[0].text);
    await dom.window.qaSdk.ready();
    await dom.window.qaSdk.share('Exact SDK chat content');
    assert.deepEqual(shares, ['Exact SDK chat content']);
    const blocks = await dom.window.qaSdk.native.calendar.freeBusy(50, 250);
    assert.deepEqual(JSON.parse(JSON.stringify(blocks)), [{ startMs: 100, endMs: 200 }]);
    assert.deepEqual(native, [{ cap: 'calendar.freebusy', params: { startMs: 50, endMs: 250 } }]);
    await assert.rejects(dom.window.qaSdk.native.contacts.pick(), /can't do contacts.pick/);
    assert.equal(native.length, 1, 'unadvertised native capabilities never reach the host');
  } finally {
    session?.dispose();
    dom.window.close();
  }
});

test('builder lowers literal syntax without trimming meaningful whitespace from strings', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'zoen-literal-build-'));
  let dom;
  try {
    await writeFile(
      join(directory, 'manifest.json'),
      JSON.stringify({ id: 'literal-test', name: 'Literal test', entry: 'main.ts' }),
    );
    await writeFile(
      join(directory, 'main.ts'),
      'const label="last";globalThis.literalValue=`first\n   \n${label}\n`;',
    );
    await buildApp(directory, join(directory, 'out'));
    const html = await readFile(join(directory, 'out/literal-test.html'), 'utf8');
    dom = new JSDOM(html, { runScripts: 'dangerously' });
    assert.equal(dom.window.literalValue, 'first\n   \nlast\n');
    assert.equal(
      /^[\t ]+$/m.test(html),
      false,
      'generated code must not create whitespace-only diff lines',
    );
  } finally {
    dom?.window.close();
    await rm(directory, { recursive: true, force: true });
  }
});
