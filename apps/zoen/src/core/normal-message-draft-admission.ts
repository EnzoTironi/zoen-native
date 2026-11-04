import { NormalDraftFailure, type DraftRecord } from './normal-message-draft-types.ts';

/** A read-only barrier; it never resumes an ambiguous Send. */
export function withEmptyDraftRecord<T>(
  record: DraftRecord,
  retired: () => boolean,
  work: (revision: string, current: () => boolean) => Promise<T>,
): Promise<T> {
  if (
    record.intent ||
    record.failedSave ||
    record.state.sending ||
    record.state.saving ||
    record.state.error ||
    record.state.edit ||
    record.persisted.edit ||
    record.state.text ||
    record.state.replyTo ||
    record.persisted.text ||
    record.persisted.reply_to
  )
    throw new NormalDraftFailure('conflict');
  const textRevision = record.state.textRevision,
    replyRevision = record.state.replyRevision;
  const revision = record.persisted.revision;
  const current = () =>
    !retired() &&
    !record.intent &&
    record.state.textRevision === textRevision &&
    record.state.replyRevision === replyRevision &&
    record.persisted.revision === revision &&
    !record.state.edit &&
    !record.state.text &&
    !record.state.replyTo;
  return work(revision, current);
}
