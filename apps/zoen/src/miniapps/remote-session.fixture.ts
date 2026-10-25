import assert from 'node:assert/strict';
import type { FetchLike, Tool } from '@modelcontextprotocol/client';
import type { RemoteAppSelection } from './remote-binding.ts';
import { createRemoteMcpSession } from './remote-session.ts';

export const uri = 'ui://provider/main.html';
const StreamingResponse = globalThis.Response;
export const tools: Tool[] = [
  {
    name: 'paint',
    inputSchema: { type: 'object' },
    _meta: { ui: { resourceUri: uri, visibility: ['model'] } },
  },
  { name: 'save', inputSchema: { type: 'object' }, _meta: { ui: { visibility: ['app'] } } },
  { name: 'query', inputSchema: { type: 'object' }, annotations: { readOnlyHint: true } },
];
export function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
export function fixture(
  options: {
    resource?: Record<string, unknown>;
    selection?: RemoteAppSelection;
    endpoint?: string;
    toolList?: Tool[];
    readWait?: Promise<void>;
    callWait?: Promise<void>;
    auth?: boolean;
    sse?: boolean;
  } = {},
) {
  const requests: {
    method: string;
    params?: Record<string, unknown>;
    credentials?: RequestCredentials;
    redirect?: RequestRedirect;
  }[] = [];
  let active = true;
  const fetcher: FetchLike = async (input, init) => {
    assert.equal(String(input), options.endpoint ?? 'https://mcp.excalidraw.com/mcp');
    if (options.auth) return new StreamingResponse('Authorization required', { status: 401 });
    if (init?.method === 'GET') return new StreamingResponse(null, { status: 405 });
    const request = JSON.parse(String(init?.body)) as {
      id?: number;
      method: string;
      params?: Record<string, unknown>;
    };
    requests.push({
      method: request.method,
      params: request.params,
      credentials: init?.credentials,
      redirect: init?.redirect,
    });
    const result = (value: unknown) => {
      const json = JSON.stringify({ jsonrpc: '2.0', id: request.id, result: value });
      return new StreamingResponse(options.sse ? `event: message\ndata: ${json}\n\n` : json, {
        headers: { 'content-type': options.sse ? 'text/event-stream' : 'application/json' },
      });
    };
    if (request.method === 'server/discover')
      return new StreamingResponse('Legacy endpoint', { status: 404 });
    if (request.method === 'initialize')
      return result({
        protocolVersion: '2025-11-25',
        serverInfo: { name: 'Synthetic remote provider', version: '1' },
        capabilities: { tools: {}, resources: {} },
      });
    if (
      request.method === 'notifications/initialized' ||
      request.method === 'notifications/cancelled'
    )
      return new StreamingResponse(null, { status: 202 });
    if (request.method === 'tools/list') return result({ tools: options.toolList ?? tools });
    if (request.method === 'resources/read') {
      await options.readWait;
      return result({
        contents: [
          {
            uri,
            mimeType: 'text/html;profile=mcp-app',
            text: '<!doctype html><p>Provider app</p>',
            _meta: {
              ui: {
                csp: {
                  resourceDomains: ['https://assets.example.com'],
                  connectDomains: ['https://api.example.com'],
                },
                permissions: { clipboardWrite: {} },
              },
            },
            ...options.resource,
          },
        ],
      });
    }
    if (request.method === 'tools/call') {
      await options.callWait;
      return result({ content: [{ type: 'text', text: 'Synthetic result' }] });
    }
    throw new Error(`Unexpected synthetic RPC ${request.method}`);
  };
  const session = createRemoteMcpSession(
    options.selection ?? 'excalidraw',
    () => {
      if (!active) throw new Error('Account changed');
    },
    { fetch: fetcher },
  );
  return {
    session,
    requests,
    retireOwner: () => {
      active = false;
    },
  };
}
