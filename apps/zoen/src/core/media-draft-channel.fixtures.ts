import { createHash } from 'node:crypto';
import { MediaDraftChannel, MediaDraftFailure } from '../media/media-draft-channel.ts';
import type {
  MediaDraftAttachments,
  MediaDraftContent,
  MediaDraftDto,
  MediaDraftIngest,
  MediaDraftPort,
  MediaDraftScope,
} from '../media/media-draft-types.ts';
import type { TimelineEntry } from './types.ts';

export const scope: MediaDraftScope = {
  expected_identity_id: 'owner',
  expected_device_id: 'device',
  space_id: 'chat',
  thread_root_id: null,
};
export const empty = (revision = '0'): MediaDraftDto => ({
  revision,
  content_json: 'null',
  assets: [],
  attempted: false,
});
export const content = (caption = 'one', asset = 'a'): MediaDraftAttachments => ({
  kind: 'attachments',
  reply_to: null,
  items: [{ asset_id: asset, caption, mutation_id: `post_${asset}` }],
});
export const multipleContent = (count = 2): MediaDraftAttachments => ({
  kind: 'attachments',
  reply_to: null,
  items: Array.from({ length: count }, (_, index) => {
    const asset = String.fromCharCode(97 + index);
    return { asset_id: asset, caption: '', mutation_id: `post_${asset}` };
  }),
});
export const ingest = (id = 'a'): MediaDraftIngest => ({
  id,
  uri: `file:///${id}`,
  name: `${id}.jpg`,
  mime: 'image/jpeg',
});
export function deferred<T = void>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}
export const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
export const kind = (name: MediaDraftFailure['kind']) => (error: unknown) =>
  error instanceof MediaDraftFailure && error.kind === name;
export function fixture() {
  let current = empty();
  let revision = 0;
  let copies = 0;
  let lease = 0;
  const sources = new Map([
    ['file:///a', 'bytes-a'],
    ['file:///b', 'bytes-b'],
  ]);
  const receipts = new Map<string, string>();
  const sends = new Map<string, readonly TimelineEntry[]>();
  const calls: {
    op: string;
    revision?: string;
    json?: string;
    assets?: readonly MediaDraftIngest[];
  }[] = [];
  const released: string[] = [];
  const hooks: {
    load?: () => Promise<void>;
    save?: () => Promise<void>;
    saved?: () => Promise<void>;
    normalize?: (json: string) => string;
    send?: () => Promise<void>;
    sent?: () => Promise<void>;
    materialize?: () => Promise<void>;
    release?: () => Promise<void>;
  } = {};
  const port: MediaDraftPort = {
    async load() {
      calls.push({ op: 'load' });
      await hooks.load?.();
      return current;
    },
    async save(_scope, expected, json, assets) {
      calls.push({ op: 'save', revision: expected, json, assets });
      await hooks.save?.();
      const supplied = assets.map((asset) => {
        const bytes = sources.get(asset.uri);
        if (!bytes) throw new Error('Source missing');
        return { ...asset, bytes };
      });
      const fingerprint = JSON.stringify([expected, json, supplied]);
      const accepted = receipts.get(fingerprint);
      if (accepted) return { accepted_revision: accepted, draft: current };
      if (expected !== current.revision) throw new Error('Stale');
      if (current.attempted && json !== 'null') throw new Error('Attempted');
      const parsed = JSON.parse(json) as MediaDraftContent | null;
      const refs =
        parsed === null
          ? []
          : parsed.kind === 'attachments'
            ? parsed.items.map((item) => item.asset_id)
            : [parsed.source_asset_id, ...(parsed.render ? [parsed.render.asset_id] : [])];
      const metadata = refs.map((id) => {
        const input = supplied.find((asset) => asset.id === id);
        if (input)
          return {
            id,
            name: input.name,
            mime: input.mime,
            size: Buffer.byteLength(input.bytes),
            sha256: createHash('sha256').update(input.bytes).digest('hex'),
          };
        const old = current.assets.find((asset) => asset.id === id);
        if (!old) throw new Error('Asset missing');
        return old;
      });
      current = {
        revision: `r${++revision}`,
        content_json: hooks.normalize?.(json) ?? json,
        assets: metadata,
        attempted: false,
      };
      const acceptedRevision = current.revision;
      receipts.set(fingerprint, acceptedRevision);
      await hooks.saved?.();
      return { accepted_revision: acceptedRevision, draft: current };
    },
    async send(_scope, expected) {
      calls.push({ op: 'send', revision: expected });
      await hooks.send?.();
      const prior = sends.get(expected);
      if (prior) return { entries: prior, draft: current };
      if (current.revision !== expected) throw new Error('Stale');
      current = { ...current, attempted: true };
      const entries = [{ id: `post-${++copies}` } as TimelineEntry];
      sends.set(expected, entries);
      current = empty(`r${++revision}`);
      await hooks.sent?.();
      return { entries, draft: current };
    },
    async materialize(_scope, expected, id) {
      calls.push({ op: 'materialize', revision: expected });
      const asset = current.assets.find((entry) => entry.id === id);
      if (!asset || current.revision !== expected) throw new Error('Stale');
      await hooks.materialize?.();
      return { ...asset, uri: `file:///lease-${++lease}` };
    },
    async release(uri) {
      released.push(uri);
      await hooks.release?.();
    },
  };
  return {
    port,
    hooks,
    calls,
    sources,
    released,
    channel: new MediaDraftChannel(scope, port),
    current: () => current,
    set: (value: MediaDraftDto) => {
      current = value;
    },
    copies: () => copies,
  };
}
