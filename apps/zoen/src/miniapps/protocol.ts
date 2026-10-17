import { t } from '../i18n/index.ts';
import { parseMiniappRequest } from './json.ts';

type Params = Record<string, unknown>;
export type HostOptions = {
  send: (message: unknown) => void;
  initialize: () => Params;
  state: () => Params;
  context: () => Params;
  tools: ReadonlySet<string>;
  call: (name: string, args: Params) => Promise<unknown>;
  resourceUri: string;
  resource: () => Promise<unknown>;
  openLink: (url: unknown) => Promise<unknown>;
  share: (text: string) => Promise<unknown>;
  native: (capability: string, params: Params) => Promise<unknown>;
  haptic: (kind: string) => void;
  resize: (height: number) => void;
  close: () => void;
};
function record(value: unknown): Params {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Params) : {};
}

/** One authority/lifetime per opaque frame; transport validates its actual source. */
export class MiniappSession {
  private active = true;
  private initialized = false;
  private handshake = false;
  private pending = new Set<string>();
  private readonly host: HostOptions;
  constructor(host: HostOptions) {
    this.host = host;
  }
  async receive(encoded: string) {
    if (!this.active || encoded.length > 1_500_000) return;
    let message: Params;
    try {
      message = record(parseMiniappRequest(encoded));
    } catch {
      return;
    }
    if (message.jsonrpc !== '2.0' || typeof message.method !== 'string') return;
    const params = record(message.params);
    const method = message.method;
    if (message.id === undefined || message.id === null) {
      if (method === 'ui/notifications/initialized' && this.handshake) {
        this.initialized = true;
        this.sync();
      } else if (
        this.initialized &&
        method === 'ui/notifications/size-changed' &&
        typeof params.height === 'number' &&
        Number.isFinite(params.height)
      )
        this.host.resize(Math.min(1600, Math.max(200, params.height)));
      else if (this.initialized && method === 'zoen/haptics' && typeof params.kind === 'string')
        this.host.haptic(params.kind);
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
    const key = `${typeof id}:${id}`;
    if (this.pending.has(key)) return;
    if (this.pending.size >= 16) {
      this.error(id, -32000, t('miniapps.anActionIsInProgress'));
      return;
    }
    this.pending.add(key);
    try {
      let result: unknown;
      if (method === 'ui/initialize') {
        result = this.host.initialize();
        this.handshake = true;
      } else if (!this.initialized) throw new Error(t('miniapps.theInterfaceHasNotInitializedYet'));
      else if (method === 'ping') result = {};
      else if (method === 'tools/call') {
        if (typeof params.name !== 'string' || !this.host.tools.has(params.name))
          throw new Error(t('miniapps.thisToolIsNotAvailableToThisMiniapp'));
        result = await this.host.call(params.name, record(params.arguments));
      } else if (method === 'resources/read') {
        if (params.uri !== this.host.resourceUri)
          throw new Error(t('miniapps.thisResourceDoesNotBelongToTheMiniapp'));
        result = await this.host.resource();
      } else if (method === 'ui/open-link') result = await this.host.openLink(params.url);
      else if (method === 'ui/message') {
        const content = record(params.content);
        if (
          typeof content.text !== 'string' ||
          !content.text.trim() ||
          content.text.length > 32_000
        )
          throw new Error(t('miniapps.thisMiniappSMessageIsInvalid'));
        result = await this.host.share(content.text);
      } else if (method === 'ui/request-display-mode') {
        if (params.mode === 'inline') this.host.close();
        result = { mode: params.mode === 'inline' ? 'inline' : 'fullscreen' };
      } else if (method.startsWith('zoen/native/'))
        result = await this.host.native(method.slice('zoen/native/'.length), params);
      else {
        this.error(id, -32601, t('miniapps.thisFeatureIsNotAvailableOnThisHost'));
        return;
      }
      if (this.active) this.host.send({ jsonrpc: '2.0', id, result });
    } catch (reason) {
      this.error(
        id,
        -32000,
        reason instanceof Error ? reason.message : t('miniapps.theActionCouldNotBeCompleted'),
      );
    } finally {
      this.pending.delete(key);
    }
  }
  sync() {
    if (!this.active || !this.initialized) return;
    this.host.send({
      jsonrpc: '2.0',
      method: 'ui/notifications/tool-input',
      params: { arguments: {} },
    });
    this.host.send({
      jsonrpc: '2.0',
      method: 'ui/notifications/tool-result',
      params: this.host.state(),
    });
    this.host.send({
      jsonrpc: '2.0',
      method: 'ui/notifications/host-context-changed',
      params: this.host.context(),
    });
  }
  dispose() {
    if (!this.active) return;
    this.host.send({
      jsonrpc: '2.0',
      id: 1_000_000,
      method: 'ui/resource-teardown',
      params: { reason: t('miniapps.theInterfaceHasClosed') },
    });
    this.active = false;
    this.pending.clear();
  }
  private error(id: unknown, code: number, message: string) {
    if (this.active) this.host.send({ jsonrpc: '2.0', id, error: { code, message } });
  }
}
