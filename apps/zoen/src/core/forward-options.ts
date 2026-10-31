import type { ForwardOptions } from './generated/ForwardOptions';
import type { TimelineEntry } from './types';

export const DEFAULT_FORWARD_OPTIONS: Readonly<ForwardOptions> = Object.freeze({
  hide_author: false,
  hide_caption: false,
});

/** Missing metadata is the legacy default; supplied flags must have their exact type. */
export function ownForwardOptions(value?: ForwardOptions): Readonly<ForwardOptions> {
  const options = value === undefined ? DEFAULT_FORWARD_OPTIONS : value;
  if (
    !options ||
    typeof options.hide_author !== 'boolean' ||
    typeof options.hide_caption !== 'boolean'
  )
    throw new Error('Invalid forwarding options');
  return Object.freeze({ hide_author: options.hide_author, hide_caption: options.hide_caption });
}

export function forwardHasCaption(entry: TimelineEntry): boolean {
  return (
    !entry.deleted &&
    !entry.voice &&
    'Message' in entry.kind &&
    entry.kind.Message.card?.kind_id === 'file' &&
    Boolean(entry.kind.Message.text.trim())
  );
}

/** Read-only content projection. Canonical IDs, actors and blob versions stay untouched. */
export function forwardPreviewEntry(entry: TimelineEntry, options: ForwardOptions): TimelineEntry {
  if (!options.hide_caption || !forwardHasCaption(entry) || !('Message' in entry.kind))
    return entry;
  return { ...entry, kind: { Message: { ...entry.kind.Message, text: '' } } };
}
