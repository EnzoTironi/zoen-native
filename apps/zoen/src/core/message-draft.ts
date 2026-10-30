import type { ReplySelection } from './generated/ReplySelection';
import type { TimelineEntry } from './types';

export function canConsumeMediaReply(
  current: { replyRevision: number; editContextRevision: number; editing: boolean },
  sent: { replyRevision: number; editContextRevision: number },
): boolean {
  return (
    !current.editing &&
    current.replyRevision === sent.replyRevision &&
    current.editContextRevision === sent.editContextRevision
  );
}

export type MessageDraft = {
  text: string;
  reply: TimelineEntry | null;
  editing: (Pick<TimelineEntry, 'id' | 'text_revision'> & { isCaption?: boolean }) | null;
  editText: string;
  editBaseText: string;
  contextRevision: number;
  textRevision: number;
  editRevision: number;
};
export type MessageDraftAttempt = {
  text: string;
  editingId: string | null;
  editingRevision: string | null;
  replyId: string | null;
  contextRevision: number;
  textRevision: number;
  editRevision: number;
};

/** Normal reply revisions never enter the transient edit reducer's fence. */
export function projectMessageDraft(
  edit: MessageDraft,
  normal: Pick<MessageDraft, 'text' | 'textRevision' | 'reply'>,
): MessageDraft {
  return { ...edit, ...normal };
}
export type MessageDraftAction =
  | { type: 'text'; value: string }
  | { type: 'reply'; entry: TimelineEntry; selection?: ReplySelection }
  | { type: 'edit'; entry: TimelineEntry; text: string }
  | { type: 'cancel' };

export function initialMessageDraft(): MessageDraft {
  return {
    text: '',
    reply: null,
    editing: null,
    editText: '',
    editBaseText: '',
    contextRevision: 0,
    textRevision: 0,
    editRevision: 0,
  };
}
