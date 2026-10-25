import {
  Client,
  StreamableHTTPClientTransport,
  type CallToolResult,
  type FetchLike,
  type Tool,
} from '@modelcontextprotocol/client';
import { sha256 } from '@noble/hashes/sha2.js';
import { parseRemoteResourcePolicy, type RemoteResourceCsp } from './remote-policy.ts';
import { resolveRemoteApp, sameRemotePolicy, type RemoteAppSelection } from './remote-binding.ts';

export { parseRemoteResourcePolicy } from './remote-policy.ts';
export type { RemoteResourceCsp } from './remote-policy.ts';
export type { RemoteAppBinding, RemoteServerId } from './remote-binding.ts';
export type RemoteAppResource = Readonly<{
  html: string;
  mimeType: 'text/html;profile=mcp-app';
  resourceUri: string;
  csp: RemoteResourceCsp;
  permissions: Readonly<Record<string, Record<string, never>>>;
  sha256: string;
  bytes: number;
}>;
export type RemoteMcpDescriptor = Readonly<{
  server: Readonly<{
    id: string;
    name: string;
    endpoint: string;
    protocolVersion: string;
    release?: string;
  }>;
  tools: readonly Tool[];
  appTools: readonly Tool[];
  mainToolName: string;
  resourceUri: string;
}>;
const MIME = 'text/html;profile=mcp-app' as const;
const MAX_RESPONSE = 12 * 1024 * 1024;
const MAX_HTML = 8 * 1024 * 1024;
const MAX_ARGUMENTS = 1_000_000;
const MAX_TOOLS = 128;
const REQUEST_TIMEOUT = 30_000;
const encoder = new TextEncoder();
const record = (v: unknown): v is Record<string, unknown> =>
  !!v && typeof v === 'object' && !Array.isArray(v);
function frozen<T>(value: T): T {
  if (value && typeof value === 'object') {
    for (const child of Object.values(value)) frozen(child);
    Object.freeze(value);
  }
  return value;
}
function copyJson<T>(value: T, maximum: number): T {
  const json = JSON.stringify(value);
  if (typeof json !== 'string' || encoder.encode(json).length > maximum)
    throw new Error('The remote MCP data exceeds its limit.');
  return JSON.parse(json) as T;
}
/** Keep the fetch implementation's native response; RN's Response cannot wrap a stream. */
function streamingResponse(response: Response, initial: ReadableStream<Uint8Array>): Response {
  let body = initial;
  let consumed = false;
  const read = async () => {
    if (consumed || body.locked) throw new TypeError('The remote response body is already used.');
    consumed = true;
    const reader = body.getReader();
    const chunks: Uint8Array[] = [];
    let length = 0;
    try {
      while (true) {
        const next = await reader.read();
        if (next.done) break;
        length += next.value.byteLength;
        chunks.push(next.value);
      }
    } finally {
      reader.releaseLock();
    }
    const bytes = new Uint8Array(length);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.byteLength;
    }
    return bytes;
  };
  const methods = {
    text: async () => new TextDecoder().decode(await read()),
    json: async () => JSON.parse(new TextDecoder().decode(await read())) as unknown,
    arrayBuffer: async () => (await read()).buffer,
    bytes: read,
    clone: () => {
      if (consumed || body.locked) throw new TypeError('The remote response body is already used.');
      const [current, copy] = body.tee();
      body = current;
      return streamingResponse(response, copy);
    },
    // MCP consumes JSON or SSE. Never delegate other readers around the byte limit.
    blob: async () => {
      throw new TypeError('MCP response blobs are unsupported.');
    },
    formData: async () => {
      throw new TypeError('MCP response form data is unsupported.');
    },
  };
  return new Proxy(response, {
    get(target, property) {
      if (property === 'body') return body;
      if (property === 'bodyUsed') return consumed || body.locked;
      if (Object.hasOwn(methods, property)) return methods[property as keyof typeof methods];
      const value: unknown = Reflect.get(target, property, target);
      return typeof value === 'function' ? value.bind(target) : value;
    },
  });
}
function uiMeta(tool: Tool): Record<string, unknown> {
  return record(tool._meta?.ui) ? tool._meta.ui : {};
}
function visibility(tool: Tool): readonly string[] {
  const value = uiMeta(tool).visibility;
  if (value === undefined) return ['model', 'app'];
  if (!Array.isArray(value) || value.some((entry) => entry !== 'model' && entry !== 'app'))
    throw new Error('The remote tool declares invalid visibility.');
  return value;
}
function resourceUri(tool: Tool): string | null {
  const value = uiMeta(tool).resourceUri;
  if (value === undefined) return null;
  if (typeof value !== 'string' || value.length > 2048) throw new Error('Invalid MCP App URI.');
  const uri = new URL(value);
  if (uri.protocol !== 'ui:' || uri.username || uri.password || !uri.hostname)
    throw new Error('The remote tool does not reference an MCP App resource.');
  return value;
}

export class RemoteMcpSession {
  private readonly client: Client;
  private readonly transport: StreamableHTTPClientTransport;
  private readonly lifetime = new AbortController();
  private active = true;
  private pending = 0;
  private connection: Promise<RemoteMcpDescriptor> | null = null;
  private resource: Promise<RemoteAppResource> | null = null;
  private descriptor: RemoteMcpDescriptor | null = null;
  private closeTask: Promise<void> | null = null;
  private readonly declared = new Map<string, Tool>();
  private readonly endpoint: URL;
  private readonly registration: ReturnType<typeof resolveRemoteApp>;
  private readonly assertActive: () => void;

  constructor(
    server: RemoteAppSelection,
    assertActive: () => void,
    options: { fetch?: FetchLike } = {},
  ) {
    this.registration = resolveRemoteApp(server);
    this.assertActive = assertActive;
    this.endpoint = new URL(this.registration.endpoint);
    this.client = new Client(
      { name: 'zoen', version: '0.1.0' },
      {
        versionNegotiation: { mode: 'auto', probe: { timeoutMs: 10_000, maxRetries: 0 } },
        capabilities: { extensions: { 'io.modelcontextprotocol/ui': { mimeTypes: [MIME] } } },
        listMaxPages: 4,
      },
    );
    this.transport = new StreamableHTTPClientTransport(this.endpoint, {
      fetch: this.guardedFetch(options.fetch ?? globalThis.fetch.bind(globalThis)),
      requestInit: { credentials: 'omit', redirect: 'error', cache: 'no-store' },
      redirectPolicy: 'same-origin',
      onInsufficientScope: 'throw',
      reconnectionOptions: {
        maxRetries: 0,
        initialReconnectionDelay: 1000,
        maxReconnectionDelay: 1000,
        reconnectionDelayGrowFactor: 1,
      },
    });
  }
  private check() {
    if (!this.active) throw new Error('This remote MCP session has closed.');
    try {
      this.assertActive();
    } catch (error) {
      void this.close().catch(() => {});
      throw error;
    }
  }
  private guardedFetch(fetcher: FetchLike): FetchLike {
    return async (input, init) => {
      this.check();
      const url = new URL(String(input));
      if (url.href !== this.endpoint.href)
        throw new Error('The remote MCP request changed server.');
      const headers = new Headers(init?.headers);
      if (headers.has('authorization') || headers.has('cookie'))
        throw new Error('Credentials are not supported by this public MCP connection.');
      const controller = new AbortController();
      const abort = () => controller.abort();
      this.lifetime.signal.addEventListener('abort', abort, { once: true });
      init?.signal?.addEventListener('abort', abort, { once: true });
      const cleanup = () => {
        this.lifetime.signal.removeEventListener('abort', abort);
        init?.signal?.removeEventListener('abort', abort);
      };
      if (this.lifetime.signal.aborted || init?.signal?.aborted) controller.abort();
      try {
        const response = await fetcher(input, {
          ...init,
          credentials: 'omit',
          redirect: 'error',
          cache: 'no-store',
          signal: controller.signal,
        });
        this.check();
        if (response.redirected || (response.url && response.url !== this.endpoint.href))
          throw new Error('The remote MCP server redirected the request.');
        if (response.status === 401 || response.status === 403)
          throw new Error(
            'This server requires authorization; sign-in is not supported by this public connection.',
          );
        if (Number(response.headers.get('content-length')) > MAX_RESPONSE)
          throw new Error('The remote MCP response exceeds its limit.');
        if (!response.body) {
          cleanup();
          return response;
        }
        const reader = response.body.getReader();
        let bytes = 0;
        const body = new ReadableStream<Uint8Array>({
          pull: async (sink) => {
            try {
              this.check();
              const next = await reader.read();
              this.check();
              if (next.done) {
                cleanup();
                sink.close();
                return;
              }
              bytes += next.value.byteLength;
              if (bytes > MAX_RESPONSE)
                throw new Error('The remote MCP response exceeds its limit.');
              sink.enqueue(next.value);
            } catch (error) {
              cleanup();
              controller.abort();
              void reader.cancel().catch(() => {});
              sink.error(error);
            }
          },
          cancel: async (reason) => {
            cleanup();
            controller.abort();
            await reader.cancel(reason);
          },
        });
        return streamingResponse(response, body);
      } catch (error) {
        cleanup();
        controller.abort();
        throw error;
      }
    };
  }
  connect(): Promise<RemoteMcpDescriptor> {
    this.check();
    if (!this.connection) this.connection = this.open();
    return this.connection;
  }
  private async open(): Promise<RemoteMcpDescriptor> {
    try {
      await this.client.connect(this.transport, {
        signal: this.lifetime.signal,
        timeout: REQUEST_TIMEOUT,
      });
      this.check();
      const result = await this.client.listTools(undefined, {
        signal: this.lifetime.signal,
        timeout: REQUEST_TIMEOUT,
      });
      this.check();
      if (result.tools.length > MAX_TOOLS)
        throw new Error('The remote server declares too many tools.');
      const tools = frozen(copyJson(result.tools, MAX_ARGUMENTS));
      for (const tool of tools) {
        if (!tool.name || tool.name.length > 128 || this.declared.has(tool.name))
          throw new Error('Invalid or duplicate remote tool.');
        visibility(tool);
        resourceUri(tool);
        this.declared.set(tool.name, tool);
      }
      const binding = this.registration.binding;
      const main = tools.find((tool) =>
        binding
          ? tool.name === binding.launchTool
          : resourceUri(tool) && visibility(tool).includes('model'),
      );
      if (!main || !visibility(main).includes('model') || !resourceUri(main))
        throw new Error('This MCP server does not expose the selected interactive MCP App.');
      if (binding && resourceUri(main) !== binding.resource.uri)
        throw new Error('The remote MCP App release resource changed.');
      const descriptor = frozen({
        server: {
          id: this.registration.id,
          name: binding
            ? this.registration.name
            : (this.client.getServerVersion()?.name ?? this.registration.name),
          ...(binding ? { release: binding.release } : {}),
          endpoint: this.endpoint.href,
          protocolVersion: this.client.getNegotiatedProtocolVersion() ?? '',
        },
        tools,
        appTools: tools.filter((tool) => visibility(tool).includes('app')),
        mainToolName: main.name,
        resourceUri: resourceUri(main)!,
      });
      this.descriptor = descriptor;
      return descriptor;
    } catch (error) {
      await this.close().catch(() => {});
      throw error;
    }
  }
  readApp(): Promise<RemoteAppResource> {
    this.check();
    if (!this.resource) this.resource = this.read();
    return this.resource;
  }
  private async read(): Promise<RemoteAppResource> {
    const descriptor = await this.connect();
    this.check();
    const result = await this.client.readResource(
      { uri: descriptor.resourceUri },
      { signal: this.lifetime.signal, timeout: REQUEST_TIMEOUT, cacheMode: 'bypass' },
    );
    this.check();
    if (result.contents.length > 8) throw new Error('The MCP App resource is ambiguous.');
    const entries = result.contents.filter(
      (entry) => entry.uri === descriptor.resourceUri && entry.mimeType === MIME && 'text' in entry,
    );
    if (entries.length !== 1 || !('text' in entries[0]) || typeof entries[0].text !== 'string')
      throw new Error('The server did not return the declared MCP App HTML resource.');
    const entry = entries[0];
    const bytes = encoder.encode(entry.text);
    if (bytes.length > MAX_HTML) throw new Error('The MCP App HTML exceeds its limit.');
    const policy = parseRemoteResourcePolicy(entry._meta);
    const resource: RemoteAppResource = {
      html: entry.text,
      mimeType: MIME,
      resourceUri: entry.uri,
      ...policy,
      bytes: bytes.length,
      sha256: Array.from(sha256(bytes), (byte) => byte.toString(16).padStart(2, '0')).join(''),
    };
    const expected = this.registration.binding?.resource;
    if (
      expected &&
      (resource.sha256 !== expected.sha256 ||
        resource.bytes !== expected.bytes ||
        !sameRemotePolicy(resource, expected))
    )
      throw new Error('The remote MCP App release bytes or policy changed.');
    return frozen(resource);
  }
  async call(
    name: string,
    args: Record<string, unknown>,
    options: { origin?: 'host' | 'app' } = {},
  ): Promise<CallToolResult> {
    this.check();
    const argumentsCopy = copyJson(args, MAX_ARGUMENTS);
    const descriptor = await this.connect();
    this.check();
    // A configured release cannot execute tools around failed resource admission.
    if (this.registration.binding) await this.readApp();
    this.check();
    const tool = this.declared.get(name);
    if (
      !tool ||
      (options.origin === 'host'
        ? name !== descriptor.mainToolName
        : !visibility(tool).includes('app'))
    )
      throw new Error('This tool is not available to this remote MCP App.');
    if (this.pending >= 8) throw new Error('Too many remote MCP actions are in progress.');
    this.pending++;
    try {
      const result = await this.client.callTool(
        { name, arguments: argumentsCopy },
        { signal: this.lifetime.signal, timeout: REQUEST_TIMEOUT },
      );
      this.check();
      return copyJson(result, MAX_RESPONSE);
    } finally {
      this.pending--;
    }
  }
  close(): Promise<void> {
    if (!this.closeTask) {
      this.active = false;
      this.lifetime.abort();
      this.declared.clear();
      this.descriptor = null;
      this.closeTask = this.client.close();
    }
    return this.closeTask;
  }
}
export function createRemoteMcpSession(
  server: RemoteAppSelection,
  assertActive: () => void,
  options: { fetch?: FetchLike } = {},
) {
  return new RemoteMcpSession(server, assertActive, options);
}
