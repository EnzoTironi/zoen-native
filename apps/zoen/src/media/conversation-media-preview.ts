import type { ClientStore } from '../core/store';
import type { TimelineEntry } from '../core/types';
import { mediaPlayback } from './playback-owner';
import type { ConversationMediaRequest } from './types';
import { t } from '../i18n/index.ts';

const fingerprint = (entry: TimelineEntry) =>
  JSON.stringify([
    entry.id,
    entry.author.id,
    entry.kind,
    entry.media,
    entry.in_thread,
    entry.reply_to?.id,
    entry.deleted,
    entry.voice,
    entry.forwarded,
  ]);

/** A viewer lease outlives its source row, but never its canonical authority.
 * No bytes, account secrets, or mutable document state are retained here. */
export async function previewConversationMedia(
  store: ClientStore,
  spaceId: string,
  selectedId: string,
  entries: readonly TimelineEntry[],
): Promise<void> {
  const preview = store.driver.media?.previewConversationMedia;
  if (!preview) throw new Error(t('media.thisFileCouldNotBeOpened'));
  if (
    entries.length < 1 ||
    entries.length > 10 ||
    new Set(entries.map((entry) => entry.id)).size !== entries.length ||
    !entries.some((entry) => entry.id === selectedId)
  )
    throw new Error(t('media.thisFileCouldNotBeOpened'));
  // Snapshot borrowed projections before the first await.
  const baseline = new Map(entries.map((entry) => [entry.id, fingerprint(entry)]));
  const request: ConversationMediaRequest = {
    spaceId,
    selectedId,
    entries: entries.map((entry) => {
      const card = 'Message' in entry.kind && entry.kind.Message.card;
      if (!card || card.kind_id !== 'file' || entry.deleted || entry.voice || entry.forwarded)
        throw new Error(t('media.thisFileCouldNotBeOpened'));
      return { messageId: entry.id, itemId: card.item_id, version: card.version };
    }),
  };
  const epoch = store.sessionEpoch;
  const signal = new AbortController();
  const current = () =>
    !store.isSessionRetiring &&
    store.sessionEpoch === epoch &&
    store.getSnapshot().phase === 'ready' &&
    !store.getSnapshot().erasePending;
  let checking = false;
  let again = false;
  let finished = false;
  const check = async () => {
    if (checking) {
      again = true;
      return;
    }
    checking = true;
    try {
      do {
        again = false;
        if (!current()) return signal.abort();
        for (const entry of request.entries) {
          const live = await store.driver.request({
            op: 'message',
            space_id: spaceId,
            message_id: entry.messageId,
          });
          if (finished || signal.signal.aborted) return;
          if (!current() || fingerprint(live) !== baseline.get(entry.messageId))
            return signal.abort();
        }
      } while (again && !finished);
    } catch {
      signal.abort();
    } finally {
      checking = false;
    }
  };
  const detachChanges = store.subscribeCanonicalChanges((spaces) => {
    if (!spaces || spaces.includes(spaceId)) void check();
  });
  const detachState = store.subscribe(() => {
    if (!current()) signal.abort();
  });
  let complete!: () => void;
  const closed = new Promise<void>((resolve) => {
    complete = resolve;
  });
  const detachCleanup = store.registerSessionCleanup(async () => {
    signal.abort();
    await closed;
  });
  try {
    await check();
    if (!current() || signal.signal.aborted) return;
    mediaPlayback.pauseAll();
    await preview(request, signal.signal);
  } finally {
    finished = true;
    signal.abort();
    detachChanges();
    detachState();
    detachCleanup();
    complete();
  }
}
