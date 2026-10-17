import type { TimelineEntry } from './types';

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
  replyId: string | null;
  contextRevision: number;
  textRevision: number;
  editRevision: number;
};
export type MessageDraftAction =
  | { type: 'text'; value: string }
  | { type: 'reply'; entry: TimelineEntry }
  | { type: 'edit'; entry: TimelineEntry; text: string }
  | { type: 'cancel' }
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
