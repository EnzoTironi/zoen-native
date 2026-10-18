import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { JSDOM, VirtualConsole } from '../../packages/zoen-ui/node_modules/jsdom/lib/api.js';

const { buildApp } = await import(
  process.env.ZOEN_MINIAPP_BUILDER_MODULE ?? new URL('./build.mjs', import.meta.url)
);

const schedule = fileURLToPath(new URL('../../miniapps/schedule', import.meta.url));

test('production Schedule and shared UI use one React dispatcher through the real SDK', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'zoen-shared-react-'));
  let dom;
  try {
    const manifest = await buildApp(schedule, directory);
    const html = await readFile(join(directory, `${manifest.id}.html`), 'utf8');
    const messages = [];
    const failures = [];
    const console = new VirtualConsole();
    console.on('jsdomError', (error) => failures.push(error.message));
    console.on('error', (...values) => failures.push(values.map(String).join(' ')));
    let send;
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
        send = (message) =>
          window.dispatchEvent(new window.MessageEvent('message', { data: message }));
        Object.defineProperty(window, 'parent', {
          value: {
            postMessage(value) {
              const message = JSON.parse(typeof value === 'string' ? value : JSON.stringify(value));
              messages.push(message);
              if (message.method === 'ui/initialize')
                window.queueMicrotask(() =>
                  send({
                    jsonrpc: '2.0',
                    id: message.id,
                    result: {
                      hostInfo: { name: 'zoen', version: 'qa' },
                      hostCapabilities: {
                        experimental: {
                          zoen: { native: ['calendar.freebusy'], services: ['state', 'members'] },
                        },
                      },
                      hostContext: { locale: 'en', theme: 'light', zoen: { members } },
                    },
                  }),
                );
              if (message.method === 'ui/notifications/initialized') {
                send({
                  jsonrpc: '2.0',
                  method: 'ui/notifications/tool-result',
                  params: { structuredContent: state },
                });
              }
            },
          },
        });
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
    send({
      jsonrpc: '2.0',
      id: reads()[0].id,
      error: { code: -32000, message: 'Synthetic calendar denied' },
    });
    await waitFor(
      () =>
        body().includes('No filter is applied.') && body().includes('Synthetic calendar denied'),
      'denial without fabricated availability',
    );
    click('Try again');
    await waitFor(() => reads().length === 2, 'explicit retry');
    send({ jsonrpc: '2.0', id: reads()[1].id, result: [] });
    await waitFor(
      () => body().includes('None of these business times conflict with events marked busy'),
      'successful empty calendar',
    );
    click('Check again');
    await waitFor(() => reads().length === 3, 'explicit refresh');
    send({ jsonrpc: '2.0', id: reads()[2].id, result: [{ ...reads()[2].params }] });
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
    dom?.window.close();
    await rm(directory, { recursive: true, force: true });
  }
});
