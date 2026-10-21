import type { ItemDetail, TimelineEntry } from './types';

export type MiniappTimelineActivity = Readonly<{
  /** Canonical events remain individually addressable; this is only a visual run. */
  entries: readonly TimelineEntry[];
  item?: ItemDetail;
}>;
export type ConversationTimelineRow = Readonly<{
  /** The first event anchors a growing run without recycling the row on every save. */
  key: string;
  entry: TimelineEntry;
  activity?: MiniappTimelineActivity;
}>;

function plainUpdate(entry: TimelineEntry) {
  return (
    'ItemEdited' in entry.kind &&
    !entry.kind.ItemEdited.is_undo &&
    !entry.deleted &&
    !entry.forwarded &&
    !entry.reply_to &&
    !entry.voice &&
    entry.reactions.length === 0 &&
    entry.thread_replies === 0 &&
    entry.delivery !== 'Sending' &&
    entry.delivery !== 'Failed'
  );
}

function adjacentSequence(previous: string, next: string) {
  if (!/^[1-9]\d{0,19}$/.test(previous) || !/^[1-9]\d{0,19}$/.test(next)) return false;
  return BigInt(previous) < 18_446_744_073_709_551_615n && BigInt(next) === BigInt(previous) + 1n;
}

/** A projection, never a new event stream. Only typed, adjacent updates to the
 * same scoped miniapp can collapse. Text, explicit shares and unknown items are
 * never inferred from translated notes or discarded from the canonical page. */
export function projectConversationTimeline(
  entries: readonly TimelineEntry[],
  items: readonly ItemDetail[],
  spaceId: string,
  revealId?: string | null,
): ConversationTimelineRow[] {
  const scoped = new Map(
    items.filter((item) => item.space_id === spaceId).map((item) => [item.id, item]),
  );
  const rows: {
    key: string;
    entry: TimelineEntry;
    activity?: { item?: ItemDetail; entries: TimelineEntry[] };
  }[] = [];
  for (const entry of entries) {
    if (
      !('ItemEdited' in entry.kind) ||
      entry.deleted ||
      entry.edited ||
      entry.forwarded ||
      entry.reply_to ||
      entry.voice ||
      entry.reactions.length > 0 ||
      entry.thread_replies > 0
    ) {
      // Keep every per-message affordance on the original renderer. Core currently
      // attaches these marks only to Message, but never erase them if that evolves.
      rows.push({ key: entry.id, entry });
      continue;
    }
    const edit = entry.kind.ItemEdited;
    const candidate = scoped.get(edit.item_id);
    // A removed/unavailable item still has its signed note and history entry.
    // An older or unrelated projection must never expose a current history target.
    const item =
      candidate?.kind_id === 'app' && candidate.app && candidate.version >= edit.version
        ? candidate
        : undefined;
    const previous = rows.at(-1);
    const last = previous?.entry;
    const priorEdit = last && 'ItemEdited' in last.kind ? last.kind.ItemEdited : null;
    const elapsed = last ? Number(entry.at_ms) - Number(last.at_ms) : NaN;
    if (
      item &&
      previous?.activity?.item === item &&
      last &&
      priorEdit &&
      plainUpdate(last) &&
      plainUpdate(entry) &&
      entry.id !== revealId &&
      last.id !== revealId &&
      entry.author.id === last.author.id &&
      entry.in_thread === last.in_thread &&
      entry.delivery === last.delivery &&
      edit.note === priorEdit.note &&
      edit.version === priorEdit.version + 1 &&
      adjacentSequence(last.seq, entry.seq) &&
      elapsed >= 0 &&
      elapsed < 180_000
    ) {
      previous.activity.entries.push(entry);
      previous.entry = entry;
    } else {
      rows.push({ key: entry.id, entry, activity: { item, entries: [entry] } });
    }
  }
  return rows;
}
