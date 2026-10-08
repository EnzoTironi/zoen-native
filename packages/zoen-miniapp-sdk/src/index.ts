/**
 * @zoen/miniapp-sdk: the bridge between a mini-app and its host.
 *
 * Transport is plain MCP Apps (SEP-1865, 2026-01-26): JSON-RPC 2.0 over
 * `window.parent.postMessage`. In Zoen the host swaps `window.parent` for a WebKit
 * message handler; in an iframe host (ChatGPT, Claude…) it's a real parent window. So the
 * same bundle runs anywhere, and `zoen.capabilities.has()` tells you what this host can do.
 *
 * Zero dependencies. No network: the host's CSP blocks it unless the manifest declares a
 * domain AND you allowed it.
 */

export type Json = null | boolean | number | string | Json[] | { [k: string]: Json };

export interface Member { id: string; name: string; initials: string; color: string; isMe: boolean }
export interface Theme { scheme: 'light' | 'dark'; vars: Record<string, string> }

/** Native capabilities a mini-app can ask for (each declared in its manifest with a purpose). */
export type NativeCapability =
  | 'photos.pick' | 'camera.capture' | 'location' | 'location.approximate'
  | 'calendar.freebusy' | 'calendar.events' | 'calendar.add' | 'contacts.pick' | 'health.steps';

/** Everything `has()` understands: native powers plus the Zoen host services. */
export type Capability = NativeCapability | 'state' | 'members' | 'ai' | 'haptics' | 'share' | 'widget' | 'theme' | `net:${string}`;

export interface WidgetSnapshot {
  template: 'stat' | 'progress' | 'countdown' | 'list' | 'caption';
  title: string;
  eyebrow?: string; value?: string; detail?: string;
  bars?: { label: string; value: number }[];
  rows?: { text: string; done: boolean }[];
  targetMs?: number;
  accentHex: string; symbol: string; art?: string;
}

/** A picked photo. `dataUrl` is a downscaled preview only you see; to put it in the group's
 *  shared state, pass `token` to a tool: the host then asks "Share with group?" first. */
export interface PickedPhoto { token: string; dataUrl: string; width: number; height: number; locationStripped: boolean }
export interface Place { lat: number; lon: number; accuracyM: number; approximate: boolean }
export interface BusyBlock { startMs: number; endMs: number }
export interface NewEvent { title: string; startMs: number; endMs: number; location?: string; notes?: string }
export interface PickedContact { name: string; initials: string }

type Pending = { resolve: (v: any) => void; reject: (e: Error) => void };
type Listener<T> = (v: T) => void;

export class ZoenError extends Error {
  constructor(message: string, readonly code: number = -32000) { super(message); }
}

class Bridge {
  private nextId = 1;
  private pending = new Map<number, Pending>();
  private handlers = new Map<string, Set<(p: any) => void>>();

  constructor() {
    window.addEventListener('message', (e: MessageEvent) => {
      const m = typeof e.data === 'string' ? safeParse(e.data) : e.data;
      if (!m || m.jsonrpc !== '2.0') return;
      if ('id' in m && (m.result !== undefined || m.error !== undefined) && this.pending.has(m.id)) {
        const p = this.pending.get(m.id)!; this.pending.delete(m.id);
        m.error ? p.reject(new ZoenError(m.error.message, m.error.code)) : p.resolve(m.result);
      } else if (m.method) {
        this.handlers.get(m.method)?.forEach((h) => h(m.params ?? {}));
        // Requests from the host (e.g. teardown) get an empty ack.
        if ('id' in m && m.id != null) this.post({ jsonrpc: '2.0', id: m.id, result: {} });
      }
    });
  }

  request<T = any>(method: string, params: Record<string, unknown> = {}, timeoutMs = 120_000): Promise<T> {
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      const t = setTimeout(() => { this.pending.delete(id); reject(new ZoenError(`${method} timed out`, -32001)); }, timeoutMs);
      this.pending.set(id, { resolve: (v) => { clearTimeout(t); resolve(v); }, reject: (e) => { clearTimeout(t); reject(e); } });
      this.post({ jsonrpc: '2.0', id, method, params });
    });
  }

  notify(method: string, params: Record<string, unknown> = {}) { this.post({ jsonrpc: '2.0', method, params }); }

  on(method: string, h: (p: any) => void) {
    if (!this.handlers.has(method)) this.handlers.set(method, new Set());
    this.handlers.get(method)!.add(h);
    return () => this.handlers.get(method)!.delete(h);
  }

  private post(m: unknown) { window.parent.postMessage(m, '*'); }
}

function safeParse(s: string) { try { return JSON.parse(s); } catch { return null; } }

class Store<T> {
  private ls = new Set<Listener<T>>();
  constructor(public value: T) {}
  set(v: T) { this.value = v; this.ls.forEach((l) => l(v)); }
  subscribe(l: Listener<T>) { this.ls.add(l); return () => { this.ls.delete(l); }; }
}

const bridge = new Bridge();
const stateStore = new Store<any>(null);
const membersStore = new Store<Member[]>([]);
const themeStore = new Store<Theme>({ scheme: matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light', vars: {} });
let host: { name: string; zoen?: { native?: string[]; services?: string[]; me?: Member; members?: Member[] } } = { name: 'unknown' };
let readyPromise: Promise<void> | null = null;
let locale = (typeof navigator !== 'undefined' && navigator.language) || 'en';

function applyTheme(ctx: any) {
  const vars = ctx?.styles?.variables ?? {};
  const scheme = ctx?.theme === 'dark' ? 'dark' : 'light';
  const root = document.documentElement;
  root.dataset.theme = scheme;
  for (const [k, v] of Object.entries(vars)) root.style.setProperty(k, String(v));
  themeStore.set({ scheme, vars: { ...themeStore.value.vars, ...vars } });
}

bridge.on('ui/notifications/tool-result', (p) => { if (p.structuredContent !== undefined) stateStore.set(p.structuredContent); });
bridge.on('ui/notifications/host-context-changed', (p) => {
  applyTheme({ ...p, theme: p.theme ?? themeStore.value.scheme });
  if (p.zoen?.members) membersStore.set(p.zoen.members);
});

/** Handshake with the host. Every other call waits for it. */
function ready(): Promise<void> {
  if (readyPromise) return readyPromise;
  readyPromise = (async () => {
    try {
      const r = await bridge.request('ui/initialize', {
        protocolVersion: '2026-01-26',
        appInfo: { name: document.title || 'zoen-miniapp', version: '0.1.0' },
        appCapabilities: { availableDisplayModes: ['inline', 'fullscreen'] },
      }, 4000);
      host = { name: r?.hostInfo?.name ?? 'unknown', zoen: r?.hostCapabilities?.experimental?.zoen };
      applyTheme(r?.hostContext);
      if (typeof r?.hostContext?.locale === 'string') locale = r.hostContext.locale;
      const zc = r?.hostContext?.zoen;
      if (zc?.members) membersStore.set(zc.members);
    } catch {
      host = { name: 'none' }; // Opened outside a host (a browser tab): everything degrades.
    }
    bridge.notify('ui/notifications/initialized');
  })();
  return readyPromise;
}

async function native<T>(cap: NativeCapability, params: Record<string, unknown> = {}): Promise<T> {
  await ready();
  if (!capabilities.has(cap)) throw new ZoenError(`This host can't do ${cap}`, -32601);
  return bridge.request<T>(`zoen/native/${cap}`, params);
}

export const capabilities = {
  /** Feature detection. In ChatGPT/Claude only `state` and `theme` are true. */
  has(c: Capability): boolean {
    if (c === 'state' || c === 'theme') return host.name !== 'none';
    if (c.startsWith('net:')) return (host.zoen?.services ?? []).includes(c);
    return (host.zoen?.native ?? []).includes(c) || (host.zoen?.services ?? []).includes(c);
  },
  get host() { return host.name; },
};

export const zoen = {
  ready,
  capabilities,

  /** Shared state: the mini-app Item. Every member sees the same version, live. */
  state: {
    get<T = any>(): T { return stateStore.value; },
    subscribe<T = any>(l: Listener<T>) { return stateStore.subscribe(l); },
    /** Call one of the mini-app's tools. The core checks the Grant; irreversible or
     *  outbound tools get a native confirmation first. Resolves with the new state. */
    async call<T = any>(tool: string, args: Record<string, Json> = {}): Promise<T> {
      await ready();
      const r = await bridge.request('tools/call', { name: tool, arguments: args });
      if (r?.isError) throw new ZoenError(r?.content?.[0]?.text ?? 'Tool failed');
      if (r?.structuredContent !== undefined) stateStore.set(r.structuredContent);
      return r?.structuredContent as T;
    },
  },

  members: {
    list(): Member[] { return membersStore.value; },
    me(): Member | undefined { return membersStore.value.find((m) => m.isMe) ?? host.zoen?.me; },
    subscribe(l: Listener<Member[]>) { return membersStore.subscribe(l); },
  },

  /** Ask the group's agent. Runs under the agent's Grant and monthly budget; the host may
   *  answer on device or decline. */
  ai: {
    async ask(prompt: string, opts: { maxTokens?: number } = {}): Promise<{ text: string; engine: string; cents: number }> {
      await ready();
      if (!capabilities.has('ai')) throw new ZoenError('No agent in this host', -32601);
      return bridge.request('zoen/ai/ask', { prompt, maxTokens: opts.maxTokens ?? 300 });
    },
  },

  haptics: {
    tap() { if (capabilities.has('haptics')) bridge.notify('zoen/haptics', { kind: 'tap' }); },
    select() { if (capabilities.has('haptics')) bridge.notify('zoen/haptics', { kind: 'select' }); },
    success() { if (capabilities.has('haptics')) bridge.notify('zoen/haptics', { kind: 'success' }); },
    warning() { if (capabilities.has('haptics')) bridge.notify('zoen/haptics', { kind: 'warning' }); },
  },

  /** Post into the chat (MCP `ui/message`), as you, after the host shows it to you. */
  async share(text: string) {
    await ready();
    return bridge.request('ui/message', { role: 'user', content: { type: 'text', text } });
  },

  /** Open a link outside (the host confirms with you first). */
  async openLink(url: string) { await ready(); return bridge.request('ui/open-link', { url }); },

  /** BCP 47 tag from the host (falls back to the browser's). */
  get locale() { return locale; },

  /** Leave full screen (in Zoen this closes the mini-app and returns to the chat). */
  async close() { await ready(); return bridge.request('ui/request-display-mode', { mode: 'inline' }); },

  /** Ask for full screen (cards start inline). */
  async fullscreen() { await ready(); return bridge.request('ui/request-display-mode', { mode: 'fullscreen' }); },

  /** Publish the Home-screen/lock-screen widget for this mini-app (strict schema, no URLs). */
  widget: {
    async setSnapshot(s: WidgetSnapshot) {
      await ready();
      if (!capabilities.has('widget')) return false;
      await bridge.request('zoen/widget/set-snapshot', { snapshot: s as unknown as Json });
      return true;
    },
  },

  theme: {
    get(): Theme { return themeStore.value; },
    subscribe(l: Listener<Theme>) { return themeStore.subscribe(l); },
  },

  /** Native powers through Zoen's two-layer consent (Zoen sheet, then iOS the first time).
   *  Reads and writes are separate; every write gets a native confirmation. */
  native: {
    photos: {
      /** System photo picker: you get only what was picked, downscaled, location stripped. */
      pick: (o: { max?: number; maxSide?: number } = {}) => native<PickedPhoto[]>('photos.pick', { max: o.max ?? 4, maxSide: o.maxSide ?? 1024 }),
    },
    camera: { capture: () => native<PickedPhoto>('camera.capture') },
    location: {
      /** When-in-use only. `approximate` asks for a ~3 km fix. */
      current: (o: { approximate?: boolean } = {}) => native<Place>(o.approximate ? 'location.approximate' : 'location'),
    },
    calendar: {
      /** Busy blocks only (no titles), unless the user granted `calendar.events` too. */
      freeBusy: (startMs: number, endMs: number) => native<BusyBlock[]>('calendar.freebusy', { startMs, endMs }),
      /** Opens the system event editor, prefilled; nothing is saved until you tap Add. */
      add: (e: NewEvent) => native<{ saved: boolean }>('calendar.add', e as unknown as Record<string, unknown>),
    },
    contacts: { pick: () => native<PickedContact[]>('contacts.pick') },
    health: { steps: (days = 7) => native<{ day: string; steps: number }[]>('health.steps', { days }) },
  },
};

export default zoen;
