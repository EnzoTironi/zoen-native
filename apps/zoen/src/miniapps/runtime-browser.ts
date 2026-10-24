import {
  createRuntimePackageLoader,
  parseRuntimePackage,
  type RuntimePackage,
} from './runtime-package.ts';

type RuntimeTarget = Pick<Window, 'postMessage'>;
type RuntimeEvent = Pick<MessageEvent, 'source' | 'origin' | 'data'>;
const object = (value: unknown): value is Record<string, unknown> =>
  Boolean(value && typeof value === 'object' && !Array.isArray(value));
const request = (value: unknown) =>
  object(value) &&
  Object.keys(value).sort().join(',') === 'kind,zoenRuntime' &&
  value.zoenRuntime === 1 &&
  value.kind === 'load';

/** App may request its one public code package, never a URL or arbitrary network operation. */
export function createRuntimeHost(
  frame: () => RuntimeTarget | null,
  input: RuntimePackage,
  check: () => void,
) {
  const runtime = parseRuntimePackage(input);
  const loader = createRuntimePackageLoader(runtime, check);
  let active = true;
  let requested = false;
  return {
    receive(event: RuntimeEvent): boolean {
      if (!request(event.data)) return false;
      const target = frame();
      if (!active || !target || event.source !== target || event.origin !== 'null') return true;
      if (requested) return true;
      requested = true;
      void loader.load().then(
        (assets) => {
          if (!active || frame() !== target) return;
          try {
            check();
            target.postMessage(
              { zoenRuntime: 1, kind: 'ready', revision: runtime.revision, ...assets },
              '*',
              [assets.wasm.buffer],
            );
          } catch {
            // A retired owner must never publish bytes or errors into a replacement frame.
          }
        },
        () => {
          if (!active || frame() !== target) return;
          try {
            check();
            target.postMessage({ zoenRuntime: 1, kind: 'error' }, '*');
          } catch {}
        },
      );
      return true;
    },
    dispose() {
      active = false;
      loader.dispose();
    },
  };
}

type Bindings = Record<string, unknown> & {
  default(input: { module_or_path: Uint8Array }): Promise<unknown>;
  zoenHostStart(): unknown;
  zoenHostRetire(): unknown;
};
export type RuntimeClient = {
  load(): Promise<Bindings>;
  onBeforeDispose(cleanup: () => void): void;
  dispose(): void;
};

/** Runs inside the opaque App. Its CSP permits no network; only parent transfers code. */
export function installRuntimeClient(input: RuntimePackage, target: Window = window) {
  const runtime = parseRuntimePackage(input);
  let active = true;
  let bindings: Bindings | undefined;
  let moduleUrl: string | undefined;
  let pending: Promise<Bindings> | undefined;
  let rejectPending: ((error: Error) => void) | undefined;
  let admitted = false;
  let ownerCleanup: (() => void) | undefined;
  const current = () => {
    if (!active) throw new Error('App runtime retired');
  };
  function dispose() {
    if (!active) return;
    active = false;
    target.removeEventListener('message', receive);
    target.removeEventListener('pagehide', dispose);
    rejectPending?.(new Error('App runtime retired'));
    rejectPending = undefined;
    const cleanup = ownerCleanup;
    ownerCleanup = undefined;
    try {
      // The loader registers pagehide first. Retire document callbacks while
      // bindings are still live, independent of later pagehide listener order.
      cleanup?.();
    } finally {
      try {
        bindings?.zoenHostRetire();
      } finally {
        if (moduleUrl) URL.revokeObjectURL(moduleUrl);
        moduleUrl = undefined;
      }
    }
  }
  let accept: ((value: Bindings) => void) | undefined;
  async function receive(event: MessageEvent) {
    if (
      !active ||
      !pending ||
      admitted ||
      event.source !== target.parent ||
      !object(event.data) ||
      event.data.zoenRuntime !== 1
    )
      return;
    if (event.data.kind === 'error') {
      admitted = true;
      rejectPending?.(
        Object.assign(new Error('App runtime could not be loaded'), {
          zoenStartupCode: 'resource_fetch_failed',
        }),
      );
      return;
    }
    const data = event.data;
    if (
      Object.keys(data).sort().join(',') !== 'kind,module,revision,wasm,zoenRuntime' ||
      data.kind !== 'ready' ||
      data.revision !== runtime.revision ||
      typeof data.module !== 'string' ||
      new TextEncoder().encode(data.module).length !== runtime.module.bytes ||
      !(data.wasm instanceof Uint8Array) ||
      data.wasm.length !== runtime.wasm.bytes
    )
      return;
    admitted = true;
    let stage = 'module_init_failed';
    try {
      current();
      moduleUrl = URL.createObjectURL(new Blob([data.module], { type: 'text/javascript' }));
      const module = (await import(/* @metro-ignore */ moduleUrl)) as Bindings;
      current();
      if (
        typeof module.default !== 'function' ||
        typeof module.zoenHostStart !== 'function' ||
        typeof module.zoenHostRetire !== 'function'
      )
        throw new Error('App runtime does not implement the managed host ABI');
      bindings = module;
      await bindings.default({ module_or_path: data.wasm });
      current();
      stage = 'runtime_start_failed';
      await bindings.zoenHostStart();
      current();
      accept?.(bindings);
      rejectPending = undefined;
    } catch (error) {
      try {
        bindings?.zoenHostRetire();
      } finally {
        rejectPending?.(
          Object.assign(new Error('App runtime failed', { cause: error }), {
            zoenStartupCode: stage,
          }),
        );
        rejectPending = undefined;
      }
    } finally {
      if (moduleUrl) URL.revokeObjectURL(moduleUrl);
      moduleUrl = undefined;
    }
  }
  const client: RuntimeClient = Object.freeze({
    onBeforeDispose(cleanup: () => void) {
      current();
      if (ownerCleanup) throw new Error('App runtime already has an owner');
      ownerCleanup = cleanup;
    },
    load() {
      current();
      if (!pending) {
        pending = new Promise<Bindings>((resolve, reject) => {
          accept = resolve;
          rejectPending = reject;
        });
        target.parent.postMessage({ zoenRuntime: 1, kind: 'load' }, '*');
      }
      return pending;
    },
    dispose,
  });
  target.addEventListener('message', receive);
  target.addEventListener('pagehide', dispose, { once: true });
  Object.defineProperty(target, '__zoenRuntime', {
    value: client,
    writable: false,
    configurable: false,
  });
  return client;
}
