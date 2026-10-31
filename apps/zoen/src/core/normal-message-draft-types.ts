import { immutableReplySelection, immutableSharedReply } from './reply-context.ts';
export { immutableReplySelection, immutableSharedReply } from './reply-context.ts';
import type { MessageDraftEdit } from './generated/MessageDraftEdit';
import type { SharedReplyDraft } from './generated/SharedReplyDraft';
import type { ReplySelection } from './generated/ReplySelection';
import type { TimelineEntry } from './types';

export type NormalDraftScope = Readonly<{
  owner: Readonly<{
    identityId: string;
    deviceId: string;
    sessionEpoch: number;
    mode: 'account' | 'demo';
  }>;
  spaceId: string;
  threadRootId: string | null;
}>;
export type NormalDraftSnapshot = Readonly<{
  revision: string;
  text: string;
  reply_to: string | null;
  reply_quote?: ReplySelection | null;
  shared_reply?: SharedReplyDraft | null;
  reply_intent?: string | null;
  edit?: MessageDraftEdit | null;
}>;
export type DraftMessageReference = Readonly<{ id: string; text_revision: string | null }>;
export type DraftAcceptance = Readonly<{
  message_id: string;
  text_revision: string | null;
  entry: TimelineEntry | null;
  draft: NormalDraftSnapshot;
}>;
export type NormalDraftPort = {
  replyIntent?(scope: NormalDraftScope): string | null;
  load(scope: NormalDraftScope): Promise<NormalDraftSnapshot>;
  save(
    scope: NormalDraftScope,
    expectedRevision: string,
    text: string,
    replyTo: string | null,
    replyQuote?: ReplySelection | null,
    sharedReply?: SharedReplyDraft | null,
    edit?: MessageDraftEdit | null,
    replyIntent?: string | null,
  ): Promise<NormalDraftSnapshot>;
  sendEdit?(scope: NormalDraftScope, expectedRevision: string): Promise<DraftAcceptance>;
  send(scope: NormalDraftScope, expectedRevision: string): Promise<DraftAcceptance>;
};
export class NormalDraftFailure extends Error {
  readonly kind: 'closed' | 'notLoaded' | 'empty' | 'load' | 'save' | 'send' | 'conflict';
  constructor(kind: NormalDraftFailure['kind'], cause?: unknown) {
    super(cause instanceof Error ? cause.message : `Message draft ${kind}`, { cause });
    this.name = 'NormalDraftFailure';
    this.kind = kind;
  }
}
export type NormalDraftState = Readonly<{
  scope: NormalDraftScope;
  text: string;
  replyTo: string | null;
  replyQuote: ReplySelection | null;
  sharedReply: SharedReplyDraft | null;
  replyIntent: string | null;
  edit: MessageDraftEdit | null;
  editRevision: number;
  editContextRevision: number;
  textRevision: number;
  replyRevision: number;
  persistedRevision: string;
  loaded: boolean;
  loading: boolean;
  saving: boolean;
  sending: boolean;
  error: NormalDraftFailure | null;
}>;
export type Content = Pick<
  NormalDraftSnapshot,
  'text' | 'reply_to' | 'reply_quote' | 'shared_reply' | 'reply_intent' | 'edit'
>;
export type SendIntent = Content & {
  kind: 'message' | 'edit';
  editRevision: number;
  editContextRevision: number;
  textRevision: number;
  replyRevision: number;
  persistedRevision: string | null;
  work: Promise<DraftMessageReference> | null;
};
export type DraftRecord = {
  state: NormalDraftState;
  persisted: NormalDraftSnapshot;
  load: Promise<void> | null;
  autosave: Promise<void> | null;
  failedSave: { baseline: NormalDraftSnapshot; content: Content } | null;
  intent: SendIntent | null;
};
export const emptySnapshot = (): NormalDraftSnapshot => ({
  revision: '0',
  text: '',
  reply_to: null,
});
export function sameEdit(
  a: MessageDraftEdit | null | undefined,
  b: MessageDraftEdit | null | undefined,
): boolean {
  return a == null
    ? b == null
    : b != null &&
        a.message_id === b.message_id &&
        a.source_revision === b.source_revision &&
        a.base_text === b.base_text &&
        a.text === b.text &&
        a.is_caption === b.is_caption;
}
export const sameContent = (a: Content, b: Content) =>
  (a.reply_intent ?? null) === (b.reply_intent ?? null) &&
  sameEdit(a.edit, b.edit) &&
  a.text === b.text &&
  a.reply_to === b.reply_to &&
  JSON.stringify(a.reply_quote ?? null) === JSON.stringify(b.reply_quote ?? null) &&
  JSON.stringify(a.shared_reply ?? null) === JSON.stringify(b.shared_reply ?? null);
export const content = (state: NormalDraftState): Content => ({
  ...(state.edit ? { edit: state.edit } : {}),
  text: state.text,
  reply_to: state.replyTo,
  ...(state.replyQuote ? { reply_quote: state.replyQuote } : {}),
  ...(state.sharedReply ? { shared_reply: state.sharedReply } : {}),
  ...(state.replyIntent ? { reply_intent: state.replyIntent } : {}),
});
export const keyFor = (scope: NormalDraftScope) =>
  JSON.stringify([
    scope.owner.mode,
    scope.owner.identityId,
    scope.owner.deviceId,
    scope.owner.sessionEpoch,
    scope.spaceId,
    scope.threadRootId,
  ]);
export function checked(value: NormalDraftSnapshot): NormalDraftSnapshot {
  if (
    typeof value.revision !== 'string' ||
    !value.revision.length ||
    value.revision.length > 256 ||
    typeof value.text !== 'string' ||
    (value.reply_intent != null &&
      (value.reply_intent === '0' || !/^[A-Za-z0-9_-]{1,128}$/.test(value.reply_intent))) ||
    (value.edit != null &&
      (typeof value.edit.message_id !== 'string' ||
        !value.edit.message_id ||
        typeof value.edit.source_revision !== 'string' ||
        !value.edit.source_revision ||
        typeof value.edit.base_text !== 'string' ||
        typeof value.edit.text !== 'string' ||
        typeof value.edit.is_caption !== 'boolean')) ||
    (value.reply_to !== null && typeof value.reply_to !== 'string')
  )
    throw new Error('Invalid message draft response');
  return {
    ...(value.edit ? { edit: Object.freeze({ ...value.edit }) } : {}),
    revision: value.revision,
    text: value.text,
    reply_to: value.reply_to,
    ...(value.reply_quote ? { reply_quote: immutableReplySelection(value.reply_quote) } : {}),
    ...(value.shared_reply ? { shared_reply: immutableSharedReply(value.shared_reply) } : {}),
    ...(value.reply_intent ? { reply_intent: value.reply_intent } : {}),
  };
}
