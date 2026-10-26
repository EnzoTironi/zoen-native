import {
  AppBridge,
  McpUiHostCapabilitiesSchema,
  McpUiHostContextSchema,
  McpUiInitializedNotificationSchema,
} from '@modelcontextprotocol/ext-apps/app-bridge';
import {
  CallToolResultSchema,
  ImplementationSchema,
  JSONRPCMessageSchema,
  ReadResourceResultSchema,
} from '@modelcontextprotocol/core';
import { ProtocolError, type JSONRPCMessage, type Transport } from '@modelcontextprotocol/client';
import { z } from 'zod/v4';
import { t } from '../i18n/index.ts';
import { parseMiniappRequest } from './json.ts';
import {
  MINIAPP_REQUEST_TIMEOUT,
  waitForMiniappInitialization,
} from './protocol-initialization.ts';

type Params = Record<string, unknown>;
export type HostOptions = {
  send: (message: unknown) => void;
  initialize: () => Params;
  state: () => Params;
  arguments?: () => Params;
  context: () => Params;
  tools: ReadonlySet<string>;
  call: (name: string, args: Params) => Promise<unknown>;
  resourceUri: string;
  resource: () => Promise<unknown>;
  openLink: (url: unknown) => Promise<unknown>;
  share: (text: string) => Promise<unknown>;
  updateModelContext?: (params: Params) => Promise<unknown>;
  native: (capability: string, params: Params) => Promise<unknown>;
  haptic: (kind: string) => void;
  resize: (height: number) => void;
  close: () => void;
  requestDisplayMode?: (
    mode: 'inline' | 'fullscreen' | 'pip',
  ) => Promise<{ mode: 'inline' | 'fullscreen' | 'pip' }>;
};
function record(value: unknown): Params {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Params) : {};
}
const keyOf = (id: string | number) => `${typeof id}:${id}`;

/** The frame owns source validation; the official bridge owns the Apps protocol. */
export class MiniappSession {
  private active = true;
  private initialized = false;
  private handshake = false;
  private initialization: Promise<void> | null = null;
  private finishInitialization: (() => void) | null = null;
  private inputSent = false;
  private flush: Promise<void> | null = null;
  private pending = new Map<string, { method: string; finish: () => void }>();
  private readonly host: HostOptions;
  private readonly bridge: AppBridge;
  private readonly transport: Transport;
  private readonly connected: Promise<void>;

  constructor(host: HostOptions) {
    this.host = host;
    const initial = host.initialize();
    this.bridge = new AppBridge(
      null,
      ImplementationSchema.parse(initial.hostInfo),
      McpUiHostCapabilitiesSchema.parse(initial.hostCapabilities),
      { hostContext: McpUiHostContextSchema.parse(initial.hostContext ?? host.context()) },
    );
    this.transport = {
      start: async () => {},
      close: async () => {
        this.transport.onclose?.();
      },
      send: async (message) => {
        const response = 'id' in message && message.id !== undefined && !('method' in message);
        const pending = response ? this.pending.get(keyOf(message.id!)) : undefined;
        if (pending?.method === 'ui/initialize' && 'result' in message) this.handshake = true;
        if (this.active || ('method' in message && message.method === 'ui/resource-teardown'))
          host.send(message);
        if (pending) {
          this.pending.delete(keyOf((message as { id: string | number }).id));
          pending.finish();
        }
      },
    };
    this.bridge.addEventListener('initialized', () => {
      if (!this.active || !this.handshake || this.initialized) return;
      this.initialized = true;
      this.finishInitialization?.();
      this.finishInitialization = null;
      this.sync();
    });
    this.bridge.addEventListener('sizechange', ({ height }) => {
      this.check();
      if (typeof height === 'number' && Number.isFinite(height))
        host.resize(Math.min(1600, Math.max(200, height)));
    });
    this.bridge.oncalltool = async ({ name, arguments: args }) => {
      this.check();
      if (!host.tools.has(name)) throw new Error(t('miniapps.thisToolIsNotAvailableToThisMiniapp'));
      // Unknown argument values are deliberately preserved by the official schema.
      // LosslessNumber tokens reach Rust through stringifyMiniappRequest unchanged.
      return CallToolResultSchema.parse(await host.call(name, record(args)));
    };
    this.bridge.onreadresource = async ({ uri }) => {
      this.check();
      if (uri !== host.resourceUri)
        throw new Error(t('miniapps.thisResourceDoesNotBelongToTheMiniapp'));
      return ReadResourceResultSchema.parse(await host.resource());
    };
    this.bridge.onopenlink = async ({ url }) => {
      this.check();
      await host.openLink(url);
      return {};
    };
    this.bridge.onmessage = async ({ content }) => {
      this.check();
      // Chat confirmation currently supports text only; never silently drop media.
      if (!content.length || content.some((block) => block.type !== 'text'))
        throw new Error(t('miniapps.thisMiniappSMessageIsInvalid'));
      const text = content.map((block) => (block.type === 'text' ? block.text : '')).join('\n');
      if (!text.trim() || text.length > 32_000)
        throw new Error(t('miniapps.thisMiniappSMessageIsInvalid'));
      await host.share(text);
      return {};
    };
    this.bridge.onrequestdisplaymode = async ({ mode }) => {
      this.check();
      if (host.requestDisplayMode) return host.requestDisplayMode(mode);
      if (mode === 'inline') host.close();
      return { mode: mode === 'inline' ? 'inline' : 'fullscreen' };
    };
    if (host.updateModelContext)
      this.bridge.onupdatemodelcontext = async (params) => {
        this.check();
        await host.updateModelContext!(params);
        return {};
      };
    this.bridge.fallbackRequestHandler = async (request) => {
      this.check();
      if (request.method.startsWith('zoen/native/'))
        return {
          data: await host.native(
            request.method.slice('zoen/native/'.length),
            record(request.params),
          ),
        };
      throw new ProtocolError(-32601, t('miniapps.thisFeatureIsNotAvailableOnThisHost'));
    };
    this.bridge.setNotificationHandler(
      'zoen/haptics',
      { params: z.object({ kind: z.string().max(32) }) },
      ({ kind }) => {
        this.check();
        host.haptic(kind);
      },
    );
    this.connected = this.bridge.connect(this.transport);
  }

  async receive(encoded: string) {
    if (encoded.length > 1_500_000) return;
    let message: JSONRPCMessage;
    try {
      message = JSONRPCMessageSchema.parse(parseMiniappRequest(encoded));
    } catch {
      return;
    }
    // Teardown acknowledgements may arrive after authority is synchronously retired.
    if (!('method' in message)) {
      await this.connected;
      this.transport.onmessage?.(message);
      return;
    }
    if (!this.active) return;
    if (!('id' in message)) {
      if (
        !this.initialized &&
        !(message.method === 'ui/notifications/initialized' && this.handshake)
      )
        return;
      if (!this.initialized) {
        if (!McpUiInitializedNotificationSchema.safeParse(message).success) return;
        // Native may deliver initialized and the first tool request in one JS
        // batch. Readiness still belongs to the official bridge's event, rather
        // than to receipt of the packet or completion of its transport send.
        this.initialization ??= new Promise<void>((resolve) => {
          this.finishInitialization = resolve;
        });
      }
      await this.connected;
      if (this.active) this.transport.onmessage?.(message);
      return;
    }
    const id = message.id;
    if (
      !(
        (typeof id === 'string' && id.length <= 128) ||
        (typeof id === 'number' && Number.isSafeInteger(id))
      )
    )
      return;
    const key = keyOf(id);
    if (this.pending.has(key)) return;
    if (this.pending.size >= 16) {
      this.error(id, t('miniapps.anActionIsInProgress'));
      return;
    }
    const initialization = this.initialized ? null : this.initialization;
    if (!this.initialized && !initialization && message.method !== 'ui/initialize') {
      this.error(id, t('miniapps.theInterfaceHasNotInitializedYet'));
      return;
    }
    let finish!: () => void;
    const completed = new Promise<void>((resolve) => {
      finish = resolve;
    });
    this.pending.set(key, { method: message.method, finish });
    try {
      if (initialization && message.method !== 'ui/initialize')
        await waitForMiniappInitialization(initialization);
      await this.connected;
    } catch {
      this.pending.delete(key);
      finish();
      this.error(id, t('miniapps.theInterfaceHasNotInitializedYet'));
      return;
    }
    if (this.active) this.transport.onmessage?.(message);
    else {
      this.pending.delete(key);
      finish();
    }
    await completed;
  }

  sync() {
    if (!this.active || !this.initialized) return;
    if (!this.inputSent) {
      this.inputSent = true;
      void this.bridge.sendToolInput({ arguments: this.host.arguments?.() ?? {} }).catch(() => {});
    }
    void this.bridge.sendToolResult(CallToolResultSchema.parse(this.host.state())).catch(() => {});
    this.syncContext();
  }

  syncContext() {
    if (!this.active || !this.initialized) return;
    this.bridge.setHostContext(McpUiHostContextSchema.parse(this.host.context()));
  }

  get ready() {
    return this.active && this.initialized;
  }

  /** Native UI may request a declared app-side tool without routing through a model. */
  async callAppTool(name: string, args: Params = {}) {
    this.check();
    const result = await this.bridge.callTool(
      { name, arguments: args },
      { timeout: MINIAPP_REQUEST_TIMEOUT },
    );
    this.check();
    return result;
  }

  /** Keep tool authority alive until the app acknowledges its last pending edit. */
  flushResource(timeout = MINIAPP_REQUEST_TIMEOUT): Promise<void> {
    if (!this.active) return Promise.reject(new Error(t('miniapps.thisInterfaceHasClosed')));
    if (!this.initialized) return Promise.resolve();
    if (this.flush) return this.flush;
    const pending = this.bridge.teardownResource({}, { timeout }).then(() => {
      if (!this.active) throw new Error(t('miniapps.thisInterfaceHasClosed'));
    });
    this.flush = pending;
    void pending
      .finally(() => {
        if (this.flush === pending) this.flush = null;
      })
      .catch(() => {});
    return pending;
  }

  dispose() {
    if (!this.active) return;
    this.active = false;
    this.finishInitialization?.();
    this.finishInitialization = null;
    for (const pending of this.pending.values()) pending.finish();
    this.pending.clear();
    void this.bridge
      .teardownResource({}, { timeout: 250 })
      .catch(() => {})
      .finally(() => this.bridge.close());
  }

  private check() {
    if (!this.active || !this.initialized)
      throw new Error(t('miniapps.theInterfaceHasNotInitializedYet'));
  }
  private error(id: string | number, message: string) {
    if (this.active) this.host.send({ jsonrpc: '2.0', id, error: { code: -32000, message } });
  }
}
