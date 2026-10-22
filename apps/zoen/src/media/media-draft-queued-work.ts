import type { MediaDraftChannel } from './media-draft-channel.ts';
import type { Intent, MediaDraftState, Stage, Write } from './media-draft-contract.ts';
import {
  assetValue,
  boundedText,
  dtoValue,
  invalid,
  MediaDraftFailure,
  saveResultValue,
} from './media-draft-contract.ts';
import type {
  MaterializedMediaDraftAsset,
  MediaDraftContent,
  MediaDraftDto,
  MediaDraftIngest,
  MediaDraftPort,
  MediaDraftScope,
  MediaDraftSendResult,
} from './media-draft-types.ts';

/** One stable work object belongs to each channel; these operations never create a second ledger. */
export type MediaDraftWorkState = {
  state: MediaDraftState;
  retirement: 'preserve' | 'discard' | null;
  tail: Promise<void>;
  loading: Promise<void> | null;
  pumping: Promise<void> | null;
  sendTask: Promise<MediaDraftSendResult> | null;
  intent: Intent | null;
  readonly writes: Write[];
  writeError: Error | null;
  readonly leases: Set<string>;
  readonly leaseReleases: Map<string, Promise<void>>;
  readonly deferredReleases: (() => Promise<void>)[];
  deferredDrain: Promise<void> | null;
  readonly stages: Set<Stage>;
  retirementTask: Promise<void> | null;
  releasing: Promise<void> | null;
  editing: { token: string; task: Promise<void> } | null;
  clearing: boolean;
  clearTask: Promise<MediaDraftDto> | null;
  recovering: boolean;
  needsReload: boolean;
};

export function createMediaDraftWorkState(): MediaDraftWorkState {
  return {
    state: Object.freeze({
      phase: 'loading',
      value: null,
      content: null,
      saving: false,
      sending: false,
      reviewing: false,
      frozen: false,
      error: null,
    }),
    retirement: null,
    tail: Promise.resolve(),
    loading: null,
    pumping: null,
    sendTask: null,
    intent: null,
    writes: [],
    writeError: null,
    leases: new Set<string>(),
    leaseReleases: new Map<string, Promise<void>>(),
    deferredReleases: [],
    deferredDrain: null,
    stages: new Set<Stage>(),
    retirementTask: null,
    releasing: null,
    editing: null,
    clearing: false,
    clearTask: null,
    recovering: false,
    needsReload: false,
  };
}

export type MediaDraftWorkOwner = {
  readonly work: MediaDraftWorkState;
  readonly scope: MediaDraftScope;
  readonly port: MediaDraftPort;

  accept(value: MediaDraftDto, preserveLocal?: boolean): MediaDraftDto;
  cancelEdit(): Promise<void>;
  drainStages(retry: boolean): Promise<void>;
  draining(): void;
  enqueue<T>(work: () => Promise<T>): Promise<T>;
  flush(): Promise<void>;
  flushWrites(): Promise<void>;
  load(): Promise<void>;
  loaded(): void;
  notify(): void;
  open(): void;
  publish(patch: Partial<MediaDraftState>): void;
  pump(): Promise<void>;
  reconcile(write: Write): Promise<void>;
  release(uri: string): Promise<void>;
  releaseDeferred(): Promise<void>;
  saveAdmitted(
    content: MediaDraftContent | null,
    assets?: readonly MediaDraftIngest[],
    stage?: Stage,
    clearing?: boolean,
  ): Promise<MediaDraftDto>;
  scheduleRelease(): void;
  send(): Promise<MediaDraftSendResult>;
  startStage(stage: Stage): Promise<void>;
};

export function stage(
  owner: MediaDraftWorkOwner,
  work: (save: MediaDraftChannel['save']) => Promise<void>,
): Promise<void> {
  try {
    owner.open();
  } catch (error) {
    return Promise.reject(error);
  }
  if (
    owner.work.clearing ||
    owner.work.recovering ||
    owner.work.state.sending ||
    owner.work.editing ||
    owner.work.intent ||
    owner.work.state.value?.attempted
  )
    return Promise.reject(new MediaDraftFailure('busy'));
  const stage: Stage = {
    work,
    task: null,
    active: false,
    cancelled: false,
    wrote: false,
    error: null,
  };
  owner.work.stages.add(stage);
  return owner.startStage(stage);
}

export function startStage(owner: MediaDraftWorkOwner, stage: Stage): Promise<void> {
  if (stage.task) return stage.task;
  stage.active = true;
  stage.wrote = false;
  stage.error = null;
  const task = Promise.resolve().then(() => {
    owner.draining();
    return stage.work((content, assets) => owner.saveAdmitted(content, assets, stage));
  });
  stage.task = task;
  void task.then(
    () => {
      stage.active = false;
      stage.task = null;
      owner.work.stages.delete(stage);
      owner.scheduleRelease();
    },
    (cause: unknown) => {
      stage.active = false;
      stage.task = null;
      if (stage.wrote || stage.cancelled || owner.work.retirement === 'discard')
        owner.work.stages.delete(stage);
      else stage.error = cause instanceof Error ? cause : new Error('Media preparation failed');
      owner.publish({ error: stage.error ?? owner.work.state.error });
    },
  );
  return task;
}

export async function drainStages(owner: MediaDraftWorkOwner, retry: boolean) {
  for (const stage of owner.work.stages) {
    if (stage.task) await stage.task;
    else if (stage.error) {
      if (!retry) throw stage.error;
      await owner.startStage(stage);
    }
  }
}

export async function reconcile(owner: MediaDraftWorkOwner, write: Write) {
  if (!write.baseline) return;
  // Rust's sealed save receipt authenticates the original revision, normalized
  // content and actual ingress byte hashes. JSON equality cannot attest bytes.
  const remote = saveResultValue(
    await owner.port.save(owner.scope, write.baseline.revision, write.json, write.assets),
  );
  owner.draining();
  write.acceptedRevision = remote.acceptedRevision;
  write.result = owner.accept(remote.value, true);
}

export function pump(owner: MediaDraftWorkOwner): Promise<void> {
  if (owner.work.pumping) return owner.work.pumping;
  const task = owner.enqueue(async () => {
    owner.draining();
    owner.publish({ saving: true });
    try {
      while (owner.work.writes.length) {
        owner.draining();
        const write = owner.work.writes[0]!;
        await owner.reconcile(write);
        if (!write.result) {
          const baseline = owner.work.state.value!;
          write.baseline = baseline;
          const value = saveResultValue(
            await owner.port.save(owner.scope, baseline.revision, write.json, write.assets),
          );
          owner.draining();
          write.acceptedRevision = value.acceptedRevision;
          write.result = owner.accept(value.value, true);
        }
        owner.work.writes.shift();
        if (write.acceptedRevision !== write.result.revision) {
          if (owner.work.writes.length) {
            // An old receipt may return independently newer durable content.
            // Pending local edits were admitted against the older intent.
            owner.work.writes[0]!.baseline = write.baseline;
            throw new MediaDraftFailure('conflict');
          }
        }
        // Rust's accepted revision authenticates this save even when serde
        // normalizes waveform floats. Adopt its canonical projection only when
        // there is no later optimistic local write to preserve.
        if (!owner.work.writes.length) owner.publish({ content: dtoValue(write.result).content });
      }
      owner.work.writeError = null;
      owner.publish({ error: null });
    } catch (cause) {
      const error = cause instanceof Error ? cause : new Error('Media draft write failed');
      if (owner.work.retirement !== 'discard') owner.work.writeError = error;
      owner.publish({ error });
      throw error;
    } finally {
      owner.publish({ saving: false });
    }
  });
  owner.work.pumping = task;
  const settled = () => {
    if (owner.work.pumping === task) owner.work.pumping = null;
    owner.scheduleRelease();
  };
  void task.then(settled, settled);
  return task;
}

export async function flushWrites(owner: MediaDraftWorkOwner) {
  owner.draining();
  if (owner.work.loading) await owner.work.loading;
  owner.loaded();
  if (owner.work.pumping) await owner.work.pumping;
  if (owner.work.writes.length) await owner.pump();
  owner.draining();
  if (owner.work.writeError) throw owner.work.writeError;
}

export async function flush(owner: MediaDraftWorkOwner): Promise<void> {
  if (owner.work.clearTask) await owner.work.clearTask;
  await owner.drainStages(false);
  await owner.flushWrites();
  if (owner.work.sendTask) await owner.work.sendTask;
  if (owner.work.intent) throw owner.work.state.error ?? new MediaDraftFailure('attempted');
  await owner.releaseDeferred();
}

export async function retry(owner: MediaDraftWorkOwner): Promise<void> {
  owner.open();
  await owner.drainStages(true);
  if (owner.work.intent) {
    await owner.send();
    return;
  }
  if (!owner.work.state.value || owner.work.needsReload) await owner.load();
  await owner.flush();
}

export async function materialize(
  owner: MediaDraftWorkOwner,
  assetId: string,
): Promise<MaterializedMediaDraftAsset> {
  owner.open();
  if (owner.work.clearTask) await owner.work.clearTask;
  await owner.flushWrites();
  const value = owner.work.state.value!;
  const expected = value.assets.find((asset) => asset.id === assetId);
  if (!expected) throw new MediaDraftFailure('invalid');
  return owner.enqueue(async () => {
    owner.draining();
    const result = await owner.port.materialize(owner.scope, value.revision, assetId);
    try {
      owner.open();
      const { uri, ...metadata } = result;
      if (
        !boundedText(uri, 16_384) ||
        !uri ||
        JSON.stringify(assetValue(metadata)) !== JSON.stringify(expected)
      )
        invalid();
      owner.work.leases.add(uri);
      return Object.freeze({ ...expected, uri });
    } catch (error) {
      if (typeof result.uri === 'string') await owner.port.release(result.uri);
      throw error;
    }
  });
}

export function release(owner: MediaDraftWorkOwner, uri: string): Promise<void> {
  const releasing = owner.work.leaseReleases.get(uri);
  if (releasing) return releasing;
  if (!owner.work.leases.has(uri)) return Promise.resolve();
  const task = Promise.resolve().then(async () => {
    await owner.port.release(uri);
    owner.work.leases.delete(uri);
  });
  owner.work.leaseReleases.set(uri, task);
  const settled = () => {
    if (owner.work.leaseReleases.get(uri) === task) owner.work.leaseReleases.delete(uri);
  };
  void task.then(settled, settled);
  return task;
}

export function deferRelease(owner: MediaDraftWorkOwner, work: () => Promise<void>): void {
  owner.work.deferredReleases.push(work);
  owner.scheduleRelease();
}

export function scheduleRelease(owner: MediaDraftWorkOwner) {
  if (
    owner.work.releasing ||
    !owner.work.deferredReleases.length ||
    owner.work.retirement ||
    owner.work.writeError ||
    [...owner.work.stages].some((stage) => stage.error)
  )
    return;
  const task = Promise.resolve().then(async () => {
    await owner.drainStages(false);
    await owner.flushWrites();
    if (owner.work.sendTask) await owner.work.sendTask;
    if (!owner.work.intent) await owner.releaseDeferred();
  });
  owner.work.releasing = task;
  const settled = () => {
    if (owner.work.releasing === task) owner.work.releasing = null;
  };
  void task.then(settled, (cause: unknown) => {
    settled();
    owner.publish({ error: cause instanceof Error ? cause : new Error('Media release failed') });
  });
}

export function releaseDeferred(owner: MediaDraftWorkOwner): Promise<void> {
  if (owner.work.deferredDrain) return owner.work.deferredDrain;
  const task = Promise.resolve().then(async () => {
    while (owner.work.deferredReleases.length) {
      if (
        owner.work.retirement !== 'discard' &&
        (owner.work.writes.length ||
          owner.work.writeError ||
          owner.work.sendTask ||
          owner.work.intent ||
          owner.work.stages.size ||
          owner.work.clearing)
      )
        return;
      await owner.work.deferredReleases[0]!();
      owner.work.deferredReleases.shift();
    }
  });
  owner.work.deferredDrain = task;
  const settled = () => {
    if (owner.work.deferredDrain === task) owner.work.deferredDrain = null;
  };
  void task.then(settled, settled);
  return task;
}

export function retire(owner: MediaDraftWorkOwner, discard: boolean): Promise<void> {
  if (discard) owner.work.retirement = 'discard';
  else if (!owner.work.retirement) owner.work.retirement = 'preserve';
  owner.work.state = Object.freeze({
    ...owner.work.state,
    phase: 'closed',
    frozen: true,
    ...(discard ? { value: null, content: null, saving: false, sending: false, error: null } : {}),
  });
  owner.notify();
  if (discard) {
    owner.work.writes.length = 0;
    owner.work.writeError = null;
    owner.work.intent = null;
  }
  if (owner.work.retirementTask) return owner.work.retirementTask;
  const task = (async () => {
    await owner.cancelEdit();
    if (owner.work.retirement === 'discard') {
      await Promise.allSettled(
        [...owner.work.stages].flatMap((stage) => (stage.task ? [stage.task] : [])),
      );
      owner.work.stages.clear();
    }
    if (owner.work.retirement !== 'discard') {
      await owner.drainStages(true);
      if (owner.work.state.value || owner.work.loading) await owner.flush();
    }
    await owner.work.tail;
    if (owner.work.sendTask)
      await owner.work.sendTask.catch((error: unknown) => {
        if (owner.work.retirement !== 'discard') throw error;
      });
    if (owner.work.retirement === 'discard') {
      owner.work.writes.length = 0;
      owner.work.writeError = null;
      owner.work.intent = null;
    }
    await owner.releaseDeferred();
    await Promise.all([...owner.work.leases].map((uri) => owner.release(uri)));
    owner.work.state = Object.freeze({
      phase: 'closed',
      value: null,
      content: null,
      saving: false,
      sending: false,
      reviewing: false,
      frozen: true,
      error: null,
    });
    owner.notify();
  })();
  owner.work.retirementTask = task;
  const settled = () => {
    if (owner.work.retirementTask === task) owner.work.retirementTask = null;
  };
  void task.then(settled, settled);
  return task;
}
