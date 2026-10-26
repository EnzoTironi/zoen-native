import type { CallToolResult } from '@modelcontextprotocol/client';
import type { CanonicalSceneDoc, CanonicalSceneHost } from './canonical-scene';

type Head = Readonly<{ version: number; sha256: string }>;
type Observation = Head & { itemId: string; ready: true; clean: boolean; liveRefresh: 1 };
const record = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
function observation(value: unknown, itemId: string): Observation | null {
  if (!record(value) || !record(value.zoen) || !record(value.zoen.sceneSave)) return null;
  const state = value.zoen.sceneSave;
  if (
    state.itemId !== itemId ||
    state.ready !== true ||
    state.liveRefresh !== 1 ||
    typeof state.clean !== 'boolean' ||
    (state.clean && state.phase !== 'saved') ||
    !Number.isSafeInteger(state.version) ||
    Number(state.version) < 1 ||
    typeof state.sha256 !== 'string' ||
    !/^[a-f0-9]{64}$/.test(state.sha256)
  )
    return null;
  return {
    itemId,
    ready: true,
    clean: state.clean,
    liveRefresh: 1,
    version: Number(state.version),
    sha256: state.sha256,
  };
}
const same = (a: Head, b: Head) => a.version === b.version && a.sha256 === b.sha256;

/** One admitted frame. Candidate bytes never change its save base before canvas ACK. */
export function canonicalRefresh(options: {
  itemId: string;
  scene: CanonicalSceneHost;
  check: () => void;
  read: () => Promise<CanonicalSceneDoc>;
  call: (name: string, args: Record<string, unknown>) => Promise<CallToolResult>;
  conflict: () => void;
}) {
  let observed: Observation | null = null;
  let active = true,
    resumePending = false,
    blocked = false,
    busy = false,
    queued = false;
  let task: Promise<void> | null = null;
  let available = false;
  const listeners = new Set<() => void>();
  const check = () => {
    if (!active) throw Error('This drawing has closed.');
    options.check();
  };
  const ready = () =>
    !!observed?.clean &&
    !busy &&
    !resumePending &&
    !blocked &&
    same(observed, options.scene.head());
  const head = async () => {
    check();
    if (!ready()) throw Error('The drawing is not ready for an agent.');
    const current = options.scene.head();
    return { itemId: options.itemId, version: current.version, sha256: current.sha256 };
  };
  const publish = () => {
    let next = false;
    try {
      next = active && ready();
    } catch {
      /* Retired frames publish unavailable, never another head. */
    }
    if (next === available) return;
    available = next;
    for (const listener of listeners) listener();
  };
  const resume = async () => {
    const result = await options.call('resume_scene', {});
    check();
    if (result.isError) throw Error('The drawing could not resume after its shared update.');
    resumePending = false;
  };
  const refresh = (): Promise<void> => {
    check();
    queued = true;
    if (task) return task;
    if ((!observed && !resumePending) || busy) return Promise.resolve();
    const work = (async () => {
      while (queued) {
        queued = false;
        const candidate = await options.read();
        check();
        if (same(candidate, options.scene.head())) {
          if (resumePending) {
            busy = true;
            publish();
            try {
              await resume();
            } finally {
              busy = false;
              publish();
            }
          }
          blocked = false;
          publish();
          continue;
        }
        // A previous application may be visible after its context ACK was lost.
        // It still needs a new exact apply ACK before becoming the host's save base.
        if (!observed?.clean || busy) {
          options.conflict();
          continue;
        }
        busy = true;
        publish();
        let id: string | null = null;
        try {
          id = options.scene.stageRefresh(candidate);
          const result = await options.call('refresh_scene', {
            refreshId: id,
            version: candidate.version,
            sha256: candidate.sha256,
          });
          check();
          const ack = result.structuredContent;
          if (
            result.isError ||
            !record(ack) ||
            ack.refreshId !== id ||
            ack.itemId !== options.itemId ||
            ack.version !== candidate.version ||
            ack.sha256 !== candidate.sha256 ||
            !observed?.clean ||
            !same(observed, candidate)
          )
            throw Error('The drawing did not acknowledge this shared update.');
          options.scene.acceptRefresh(id);
          resumePending = true;
          await resume();
          blocked = false;
        } catch (error) {
          if (id) {
            try {
              options.scene.cancelRefresh(id);
            } catch {
              /* An owner change already retired the scene. */
            }
          }
          options.conflict();
          throw error;
        } finally {
          busy = false;
          publish();
        }
      }
    })();
    task = work
      .catch((error: unknown) => {
        blocked = true;
        publish();
        throw error;
      })
      .finally(() => {
        if (task === tracked) task = null;
      });
    const tracked = task;
    return task;
  };
  return {
    observedHead: head,
    readiness: {
      getSnapshot: () => available,
      subscribe: (listener: () => void) => {
        listeners.add(listener);
        return () => {
          listeners.delete(listener);
        };
      },
    },
    observe(value: unknown) {
      check();
      const next = observation(value, options.itemId);
      if (next) observed = next;
      else observed = null;
      publish();
      if (queued && !busy && observed) void refresh().catch(() => {});
    },
    refresh,
    async barrier<T>(work: () => Promise<T>): Promise<T> {
      check();
      if (task) await task;
      check();
      if (busy) throw Error('The drawing is applying a shared update.');
      busy = true;
      publish();
      try {
        return await work();
      } finally {
        busy = false;
        publish();
        if (queued && active) void refresh().catch(() => {});
      }
    },
    dispose() {
      active = false;
      observed = null;
      publish();
      listeners.clear();
    },
  };
}
