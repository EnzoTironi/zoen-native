import type { VoiceDraft } from './voice-edit.ts';
import type { VoiceSession } from './voice-session.ts';

/** Ephemeral preview admission only; canonical edits and send bytes stay in VoiceSession. */
export type PendingVoiceTrim = Readonly<{
  session: VoiceSession;
  draft: VoiceDraft;
  owner: object;
}> | null;

export function updatePendingVoiceTrim(
  previous: PendingVoiceTrim,
  session: VoiceSession,
  draft: VoiceDraft,
  pending: boolean,
  owner: object,
): PendingVoiceTrim {
  if (pending) {
    if (session.getSnapshot().draft !== draft) return previous;
    return previous?.session === session && previous.draft === draft && previous.owner === owner
      ? previous
      : { session, draft, owner };
  }
  // A delayed child cleanup can release only its exact immutable draft.
  return previous?.session === session && previous.draft === draft && previous.owner === owner
    ? null
    : previous;
}

export function hasPendingVoiceTrim(pending: PendingVoiceTrim, session: VoiceSession): boolean {
  return pending?.session === session && pending.draft === session.getSnapshot().draft;
}
