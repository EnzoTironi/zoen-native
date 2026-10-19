import { sceneMutationId, type SceneAck } from "./canonical-transfer.ts";

export type SceneSaveState = Readonly<{
  phase: "saved" | "pending" | "saving" | "error";
  sequence: number;
  error?: string;
}>;
type Attempt = { sequence: number; json: string; mutation: string };
export function createSceneSaves(options: {
  initial: string;
  save: (json: string, mutation: string) => Promise<SceneAck>;
  schedule: (work: () => void) => () => void;
  publish: (state: SceneSaveState) => void;
}) {
  let active = true;
  let paused = false;
  let latest = options.initial;
  let sequence = 0,
    accepted = 0;
  let attempt: Attempt | null = null;
  let running: Promise<void> | null = null;
  let cancel: (() => void) | null = null;
  let state: SceneSaveState = { phase: "saved", sequence: 0 };
  const listeners = new Set<() => void>();
  const publish = (next: SceneSaveState) => {
    if (!active) return;
    state = next;
    options.publish(next);
    for (const listener of listeners) listener();
  };
  const check = () => {
    if (!active) throw new Error("This drawing has closed.");
  };
  const pump = (): Promise<void> => {
    check();
    if (running) return running;
    cancel?.();
    cancel = null;
    const task = (async () => {
      while (active && !paused && (attempt || accepted < sequence)) {
        attempt ??= { sequence, json: latest, mutation: sceneMutationId() };
        const captured = attempt;
        publish({ phase: "saving", sequence });
        try {
          await options.save(captured.json, captured.mutation);
          check();
          accepted = captured.sequence;
          attempt = null;
        } catch (error) {
          publish({
            phase: "error",
            sequence,
            error: error instanceof Error ? error.message : "The drawing could not be saved.",
          });
          throw error;
        }
      }
      publish({ phase: accepted < sequence ? "pending" : "saved", sequence });
    })();
    running = task.finally(() => {
      if (running === tracked) running = null;
    });
    const tracked = running;
    return running;
  };
  return {
    getSnapshot: () => state,
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    baseline(json: string) {
      check();
      if (sequence === 0 && !attempt && !running) latest = json;
    },
    capture(json: string) {
      check();
      if (json === latest) return;
      latest = json;
      sequence++;
      if (state.phase === "error" || running || paused) return;
      publish({ phase: "pending", sequence });
      cancel?.();
      cancel = options.schedule(() => {
        cancel = null;
        void pump().catch(() => {});
      });
    },
    async pause() {
      check();
      paused = true;
      cancel?.();
      cancel = null;
      if (running) await running.catch(() => {});
      check();
    },
    resume() {
      check();
      paused = false;
    },
    async flush() {
      paused = false;
      check();
      cancel?.();
      cancel = null;
      while (active && (running || attempt || accepted < sequence)) await (running ?? pump());
      check();
    },
    dispose() {
      active = false;
      cancel?.();
      cancel = null;
      listeners.clear();
    },
  };
}
