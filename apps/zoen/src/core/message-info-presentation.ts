import type { MessageReadParticipant } from './generated/MessageReadParticipant';

/** The explicit peer clock is the only read-date source; observation is never a fallback. */
export function messageReadDate(participant: MessageReadParticipant): string | null {
  return participant.read ? participant.read_reported_at_ms : null;
}
export function messageInfoDate(value: string | null, locale: string): string | null {
  if (value === null || !/^-?\d+$/.test(value)) return null;
  const at = Number(value);
  if (!Number.isSafeInteger(at) || !Number.isFinite(new Date(at).getTime())) return null;
  return new Date(at).toLocaleString(locale, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}
