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
  editing: TimelineEntry | null;
  editText: string;
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
  | { type: 'cancel' }
  | { type: 'media-accepted'; contextRevision: number }
  | { type: 'accepted'; attempt: MessageDraftAttempt };

export function initialMessageDraft(): MessageDraft {
  return {
    text: '',
    reply: null,
    editing: null,
    editText: '',
    contextRevision: 0,
    textRevision: 0,
    editRevision: 0,
  };
}

export function captureMessageDraft(state: MessageDraft): MessageDraftAttempt {
  return {
    text: state.editing ? state.editText : state.text,
    editingId: state.editing?.id ?? null,
    editingRevision: state.editing?.text_revision ?? state.editing?.id ?? null,
    replyId: state.reply?.id ?? null,
    contextRevision: state.contextRevision,
    textRevision: state.textRevision,
    editRevision: state.editRevision,
  };
}

/** Completion only consumes the exact draft it sent; newer typing and context survive. */
export function messageDraftReducer(state: MessageDraft, action: MessageDraftAction): MessageDraft {
  switch (action.type) {
    case 'text':
      return state.editing
        ? { ...state, editText: action.value, editRevision: state.editRevision + 1 }
        : { ...state, text: action.value, textRevision: state.textRevision + 1 };
    case 'reply':
      return {
        ...state,
        reply: action.entry,
        editing: null,
        editText: '',
        contextRevision: state.contextRevision + 1,
      };
    case 'edit':
      return {
        ...state,
        reply: null,
        editing: action.entry,
        editText: action.text,
        contextRevision: state.contextRevision + 1,
        editRevision: state.editRevision + 1,
      };
    case 'cancel':
      return {
        ...state,
        reply: null,
        editing: null,
        editText: '',
        contextRevision: state.contextRevision + 1,
      };
    case 'media-accepted':
      return state.contextRevision === action.contextRevision && !state.editing
        ? { ...state, reply: null, contextRevision: state.contextRevision + 1 }
        : state;
    case 'accepted': {
      const sent = action.attempt;
      if (sent.editingId)
        return state.editing?.id === sent.editingId &&
          state.contextRevision === sent.contextRevision &&
          state.editRevision === sent.editRevision
          ? { ...state, editing: null, editText: '', contextRevision: state.contextRevision + 1 }
          : state;
      return {
        ...state,
        text: state.textRevision === sent.textRevision ? '' : state.text,
        reply: state.contextRevision === sent.contextRevision ? null : state.reply,
      };
    }
  }
}
