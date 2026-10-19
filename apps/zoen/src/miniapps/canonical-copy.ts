import type { CanonicalSceneDoc } from './canonical-scene.ts';

type Input = Readonly<{ item_id: string; scene_json: string; mutation_id: string }>;
type Options = {
  itemId: string;
  check: () => void;
  id: () => string;
  prepare: () => Promise<{ isError?: boolean; structuredContent?: unknown }>;
  staged: (
    uploadId: string,
    bytes: number,
    sha256: string,
  ) => Readonly<{ scene_json: string; mutation_id: string }>;
  save: (input: Input) => Promise<CanonicalSceneDoc>;
  blocked: (value: boolean) => void;
};

/** An ambiguous copy keeps its exact bytes and mutation until ACK, while editing stays paused. */
export function canonicalCopy(options: Options) {
  let input: Input | null = null;
  let sha: string | null = null;
  let task: Promise<CanonicalSceneDoc> | null = null;
  let active = true;
  const check = () => {
    if (!active) throw new Error('This drawing has closed.');
    options.check();
  };
  return {
    run(): Promise<CanonicalSceneDoc> {
      check();
      if (task) return task;
      const pending = (async () => {
        options.blocked(true);
        if (!input) {
          try {
            const result = await options.prepare();
            check();
            const payload = result.structuredContent;
            const metadata =
              payload && typeof payload === 'object' && !Array.isArray(payload)
                ? (payload as Record<string, unknown>)
                : null;
            if (
              result.isError ||
              !metadata ||
              typeof metadata.uploadId !== 'string' ||
              !Number.isSafeInteger(metadata.bytes) ||
              typeof metadata.sha256 !== 'string'
            )
              throw new Error('The drawing did not prepare its current scene.');
            const current = options.staged(
              metadata.uploadId,
              metadata.bytes as number,
              metadata.sha256,
            );
            check();
            sha = metadata.sha256;
            input = Object.freeze({
              item_id: options.itemId,
              scene_json: current.scene_json,
              mutation_id: options.id(),
            });
          } catch (error) {
            check();
            options.blocked(false);
            throw error;
          }
        }
        check();
        const accepted = await options.save(input);
        check();
        if (
          accepted.item_id === options.itemId ||
          !/^[A-Za-z0-9_-]{1,128}$/.test(accepted.item_id) ||
          accepted.version !== 1 ||
          accepted.current_version < 1 ||
          accepted.sha256 !== sha ||
          accepted.scene_json !== input.scene_json
        )
          throw new Error('The copy did not acknowledge this exact drawing.');
        return accepted;
      })();
      task = pending;
      void pending
        .finally(() => {
          if (task === pending) task = null;
        })
        .catch(() => {});
      return pending;
    },
    get unresolved() {
      return input !== null;
    },
    get busy() {
      return task !== null;
    },
    dispose() {
      active = false;
      input = null;
      sha = null;
    },
  };
}
