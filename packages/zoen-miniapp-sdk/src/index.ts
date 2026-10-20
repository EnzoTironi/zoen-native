/** The official MCP Apps client plus feature-detected Zoen native services. */
import { App } from '@modelcontextprotocol/ext-apps/app-with-deps';

export type Json = null | boolean | number | string | Json[] | { [k: string]: Json };

export interface Member {
  id: string;
  name: string;
  initials: string;
  color: string;
  isMe: boolean;
}
export interface Theme {
  scheme: 'light' | 'dark';
  vars: Record<string, string>;
}

/** Native capabilities a mini-app can ask for (each declared in its manifest with a purpose). */
export type NativeCapability =
  | 'photos.pick'
  | 'camera.capture'
  | 'location'
  | 'location.approximate'
  | 'calendar.freebusy'
  | 'calendar.events'
  | 'calendar.add'
  | 'contacts.pick'
  | 'health.steps';

/** Everything `has()` understands: native powers plus the Zoen host services. */
export type Capability =
  | NativeCapability
  | 'state'
  | 'members'
  | 'ai'
  | 'haptics'
  | 'share'
  | 'widget'
  | 'theme'
  | `net:${string}`;

export interface WidgetSnapshot {
  template: 'stat' | 'progress' | 'countdown' | 'list' | 'caption';
  title: string;
  eyebrow?: string;
  value?: string;
  detail?: string;
  bars?: { label: string; value: number }[];
  rows?: { text: string; done: boolean }[];
  targetMs?: number;
  accentHex: string;
  symbol: string;
  art?: string;
}

/** A picked photo. `dataUrl` is a downscaled preview only you see; to put it in the group's
 *  shared state, pass `token` to a tool: the host then asks "Share with group?" first. */
export interface PickedPhoto {
  token: string;
  dataUrl: string;
  width: number;
  height: number;
  locationStripped: boolean;
}
export interface Place {
  lat: number;
  lon: number;
  accuracyM: number;
  approximate: boolean;
}
export interface BusyBlock {
  startMs: number;
  endMs: number;
}
export interface NewEvent {
  title: string;
  startMs: number;
  endMs: number;
  location?: string;
  notes?: string;
}
export interface PickedContact {
  name: string;
  initials: string;
}

type Listener<T> = (v: T) => void;

export class ZoenError extends Error {
  constructor(
    message: string,
    readonly code: number = -32000,
  ) {
    super(message);
  }
}

const extensionResult = {
  '~standard': { version: 1 as const, vendor: 'zoen', validate: (value: unknown) => ({ value }) },
};
class Bridge {
  readonly app = new App(
    { name: document.title || 'zoen-miniapp', version: '0.1.0' },
    { availableDisplayModes: ['inline', 'fullscreen'] },
  );
  request<T = any>(
    method: string,
    params: Record<string, unknown> = {},
    timeout = 120_000,
  ): Promise<T> {
    return this.app.request({ method, params }, extensionResult, { timeout }) as Promise<T>;
  }
  notify(method: string, params: Record<string, unknown> = {}) {
    void this.app.notification({ method, params }).catch(() => {});
  }
}

class Store<T> {
  private ls = new Set<Listener<T>>();
  constructor(public value: T) {}
  set(v: T) {
    this.value = v;
    this.ls.forEach((l) => l(v));
  }
  subscribe(l: Listener<T>) {
    this.ls.add(l);
    return () => {
      this.ls.delete(l);
    };
  }
}

const bridge = new Bridge();
const stateStore = new Store<any>(null);
const membersStore = new Store<Member[]>([]);
const themeStore = new Store<Theme>({
  scheme:
    typeof matchMedia === 'function' && matchMedia('(prefers-color-scheme: dark)').matches
      ? 'dark'
      : 'light',
  vars: {},
});
let host: {
  name: string;
  zoen?: { native?: string[]; services?: string[]; me?: Member; members?: Member[] };
} = { name: 'unknown' };
let readyPromise: Promise<void> | null = null;
let locale = (typeof navigator !== 'undefined' && navigator.language) || 'en';

function applyHostContext(ctx: any) {
  const vars = ctx?.styles?.variables ?? {};
  const scheme = ctx?.theme === 'dark' ? 'dark' : ctx?.theme === 'light' ? 'light' : themeStore.value.scheme;
  const root = document.documentElement;
  root.dataset.theme = scheme;
  const insets = ctx?.displayMode === 'fullscreen' ? ctx.safeAreaInsets : undefined;
  for (const edge of ['top', 'right', 'bottom', 'left'] as const) {
    const value = insets?.[edge];
    const pixels = typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : 0;
    root.style.setProperty(`--zoen-host-safe-${edge}`, `${pixels}px`);
  }
  for (const [k, v] of Object.entries(vars)) root.style.setProperty(k, String(v));
  themeStore.set({ scheme, vars: { ...themeStore.value.vars, ...vars } });
}

bridge.app.ontoolresult = (p) => {
  if (p.structuredContent !== undefined) stateStore.set(p.structuredContent);
};
bridge.app.onhostcontextchanged = (p: any) => {
  // The official App merges partial notifications before invoking this callback.
  applyHostContext(bridge.app.getHostContext());
  if (p.zoen?.members) membersStore.set(p.zoen.members);
};
bridge.app.onteardown = async () => ({});

/** Handshake with the host. Every other call waits for it. */
function ready(): Promise<void> {
  if (readyPromise) return readyPromise;
  readyPromise = (async () => {
    try {
      await bridge.app.connect(undefined, { timeout: 4000 });
      const r = {
        hostInfo: bridge.app.getHostVersion(),
        hostCapabilities: bridge.app.getHostCapabilities(),
        hostContext: bridge.app.getHostContext() as any,
      };
      host = { name: r.hostInfo?.name ?? 'unknown', zoen: r.hostCapabilities?.experimental?.zoen };
      applyHostContext(r?.hostContext);
      if (typeof r?.hostContext?.locale === 'string') locale = r.hostContext.locale;
      const zc = r?.hostContext?.zoen;
      if (zc?.members) membersStore.set(zc.members);
    } catch {
      host = { name: 'none' }; // Opened outside a host (a browser tab): everything degrades.
    }
  })();
  return readyPromise;
}

async function native<T>(cap: NativeCapability, params: Record<string, unknown> = {}): Promise<T> {
  await ready();
  if (!capabilities.has(cap)) throw new ZoenError(`This host can't do ${cap}`, -32601);
  const result = await bridge.request<{ data: T }>(`zoen/native/${cap}`, params);
  return result.data;
}

export const capabilities = {
  /** Feature detection from the connected host's advertised capabilities. */
  has(c: Capability): boolean {
    if (c === 'state' || c === 'theme') return host.name !== 'none' && host.name !== 'unknown';
    if (c.startsWith('net:')) return (host.zoen?.services ?? []).includes(c);
    return (host.zoen?.native ?? []).includes(c) || (host.zoen?.services ?? []).includes(c);
  },
  get host() {
    return host.name;
  },
};

export const zoen = {
  ready,
  capabilities,

  /** Shared state: the mini-app Item. Every member sees the same version, live. */
  state: {
    get<T = any>(): T {
      return stateStore.value;
    },
    subscribe<T = any>(l: Listener<T>) {
      return stateStore.subscribe(l);
    },
    /** Call one of the mini-app's tools. The core checks the Grant; irreversible or
     *  outbound tools get a native confirmation first. Resolves with the new state. */
    async call<T = any>(tool: string, args: Record<string, Json> = {}): Promise<T> {
      await ready();
      const r = await bridge.app.callServerTool({ name: tool, arguments: args });
      if (r?.isError) {
        const first = r.content?.[0];
        throw new ZoenError(first?.type === 'text' ? first.text : 'Tool failed');
      }
      if (r?.structuredContent !== undefined) stateStore.set(r.structuredContent);
      return r?.structuredContent as T;
    },
  },

  members: {
    list(): Member[] {
      return membersStore.value;
    },
    me(): Member | undefined {
      return membersStore.value.find((m) => m.isMe) ?? host.zoen?.me;
    },
    subscribe(l: Listener<Member[]>) {
      return membersStore.subscribe(l);
    },
  },

  /** Ask the group's agent. Runs under the agent's Grant and monthly budget; the host may
   *  answer on device or decline. */
  ai: {
    async ask(
      prompt: string,
      opts: { maxTokens?: number } = {},
    ): Promise<{ text: string; engine: string; cents: number }> {
      await ready();
      if (!capabilities.has('ai')) throw new ZoenError('No agent in this host', -32601);
      return bridge.request('zoen/ai/ask', { prompt, maxTokens: opts.maxTokens ?? 300 });
    },
  },

  haptics: {
    tap() {
      if (capabilities.has('haptics')) bridge.notify('zoen/haptics', { kind: 'tap' });
    },
    select() {
      if (capabilities.has('haptics')) bridge.notify('zoen/haptics', { kind: 'select' });
    },
    success() {
      if (capabilities.has('haptics')) bridge.notify('zoen/haptics', { kind: 'success' });
    },
    warning() {
      if (capabilities.has('haptics')) bridge.notify('zoen/haptics', { kind: 'warning' });
    },
  },

  /** Post into the chat (MCP `ui/message`), as you, after the host shows it to you. */
  async share(text: string) {
    await ready();
    return bridge.app.sendMessage({ role: 'user', content: [{ type: 'text', text }] });
  },

  /** Open a link outside (the host confirms with you first). */
  async openLink(url: string) {
    await ready();
    return bridge.app.openLink({ url });
  },

  /** BCP 47 tag from the host (falls back to the browser's). */
  get locale() {
    return locale;
  },

  /** Leave full screen (in Zoen this closes the mini-app and returns to the chat). */
  async close() {
    await ready();
    return bridge.app.requestDisplayMode({ mode: 'inline' });
  },

  /** Ask for full screen (cards start inline). */
  async fullscreen() {
    await ready();
    return bridge.app.requestDisplayMode({ mode: 'fullscreen' });
  },

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
    get(): Theme {
      return themeStore.value;
    },
    subscribe(l: Listener<Theme>) {
      return themeStore.subscribe(l);
    },
  },

  /** Native powers through Zoen's two-layer consent (Zoen sheet, then iOS the first time).
   *  Reads and writes are separate; every write gets a native confirmation. */
  native: {
    photos: {
      /** System photo picker: you get only what was picked, downscaled, location stripped. */
      pick: (o: { max?: number; maxSide?: number } = {}) =>
        native<PickedPhoto[]>('photos.pick', { max: o.max ?? 4, maxSide: o.maxSide ?? 1024 }),
    },
    camera: { capture: () => native<PickedPhoto>('camera.capture') },
    location: {
      /** When-in-use only. `approximate` asks for a ~3 km fix. */
      current: (o: { approximate?: boolean } = {}) =>
        native<Place>(o.approximate ? 'location.approximate' : 'location'),
    },
    calendar: {
      /** Busy blocks only (no titles), unless the user granted `calendar.events` too. */
      freeBusy: (startMs: number, endMs: number) =>
        native<BusyBlock[]>('calendar.freebusy', { startMs, endMs }),
      /** Opens the system event editor, prefilled; nothing is saved until you tap Add. */
      add: (e: NewEvent) =>
        native<{ saved: boolean }>('calendar.add', e as unknown as Record<string, unknown>),
    },
    contacts: { pick: () => native<PickedContact[]>('contacts.pick') },
    health: {
      steps: (days = 7) => native<{ day: string; steps: number }[]>('health.steps', { days }),
    },
  },
};

export default zoen;
