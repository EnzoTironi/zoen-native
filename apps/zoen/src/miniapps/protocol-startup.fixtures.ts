import { App } from '@modelcontextprotocol/ext-apps';
import type { JSONRPCMessage, Transport } from '@modelcontextprotocol/client';
import { MiniappSession } from './protocol.ts';

const tick = () => new Promise<void>((resolve) => setImmediate(resolve));

/** Native postMessage may deliver consecutive, ordered guest packets in one
 * JS batch. The official App and AppBridge remain real; only transport is a port. */
export function protocolStartupFixture() {
  const inbound: JSONRPCMessage[] = [],
    outbound: JSONRPCMessage[] = [],
    calls: string[] = [];
  const guest = new App({ name: 'Owned App', version: '1' }, {}, { autoResize: false });
  const transport: Transport = {
    start: async () => {},
    close: async () => {
      transport.onclose?.();
    },
    send: async (message) => {
      inbound.push(message);
    },
  };
  const session = new MiniappSession({
    initialize: () => ({
      hostInfo: { name: 'zoen', version: '1' },
      hostCapabilities: { serverTools: {}, serverResources: {} },
      hostContext: { theme: 'dark', displayMode: 'fullscreen' },
    }),
    send: (message) => {
      outbound.push(message as JSONRPCMessage);
      transport.onmessage?.(message as JSONRPCMessage);
    },
    state: () => ({ content: [], structuredContent: { checkpointId: 'same-item' } }),
    context: () => ({ theme: 'dark', displayMode: 'fullscreen' }),
    tools: new Set(['app_scene_read']),
    call: async (name) => {
      calls.push(name);
      return { content: [], structuredContent: { document: 'retained scene' } };
    },
    resourceUri: 'ui://zoen/package/fixture/1/' + 'ab'.repeat(32),
    resource: async () => ({ contents: [] }),
    openLink: async () => ({}),
    share: async () => ({}),
    native: async () => ({}),
    haptic() {},
    resize() {},
    close() {},
  });
  return {
    session,
    guest,
    calls,
    inbound,
    outbound,
    async connect() {
      const connecting = guest.connect(transport);
      await tick();
      const first = inbound.splice(0);
      await Promise.all(first.map((message) => session.receive(JSON.stringify(message))));
      await connecting;
      // The ordered initialized packet is queued but has not crossed native yet.
    },
    async batch() {
      const packets = inbound.splice(0);
      const tasks = packets.map((message) => session.receive(JSON.stringify(message)));
      await Promise.all(tasks);
      await tick();
    },
    async close() {
      session.dispose();
      await guest.close();
    },
  };
}
