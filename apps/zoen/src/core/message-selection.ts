import { messageCanInteract } from './message-presentation.ts';
import { copyableMessageText } from './message-copy-text.ts';
import type { TimelineEntry } from './types';

export const MAX_SELECTED_MESSAGES = 100;

export type MessageSelection = Readonly<{ scope: string; ids: readonly string[] }> | null;

/** The UI retains identities only; every action resolves the current signed entries. */
export function selectedMessages(
  selection: MessageSelection,
  scope: string | null,
  entries: readonly TimelineEntry[],
): TimelineEntry[] {
  if (!scope || selection?.scope !== scope) return [];
  const ids = new Set(selection.ids);
  return entries.filter((entry) => ids.has(entry.id) && messageCanInteract(entry));
}

export function toggleMessageSelection(
  selection: MessageSelection,
  scope: string | null,
  entries: readonly TimelineEntry[],
  id: string,
): MessageSelection {
  if (!scope || !entries.some((entry) => entry.id === id && messageCanInteract(entry)))
    return selection;
  const ids = selectedMessages(selection, scope, entries).map((entry) => entry.id);
  if (!ids.includes(id) && ids.length >= MAX_SELECTED_MESSAGES) return selection;
  const next = ids.includes(id) ? ids.filter((value) => value !== id) : [...ids, id];
  return next.length ? { scope, ids: next } : null;
}

export function selectedMessageText(entries: readonly TimelineEntry[]): string {
  return entries.map(copyableMessageText).filter(Boolean).join('\n\n');
}

/** Route data is a frozen identity list, never a send command. */
export function selectionMessageIds(value: string): string[] {
  try {
    const ids: unknown = JSON.parse(value);
    return Array.isArray(ids) &&
      ids.length &&
      ids.length <= MAX_SELECTED_MESSAGES &&
      ids.every((id) => typeof id === 'string' && id.trim())
      ? [...new Set(ids)]
      : [];
  } catch {
    return [];
  }
}
