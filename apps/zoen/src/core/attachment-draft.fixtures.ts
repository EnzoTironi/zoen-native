import {
  AttachmentDraft,
  AttachmentDraftFailure,
  type AttachmentAsset,
  type AttachmentDraftItem,
  type AttachmentDraftPort,
  type AttachmentDraftScope,
  type AttachmentReceipt,
} from '../media/attachment-draft.ts';

export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}
export const scope: AttachmentDraftScope = {
  mode: 'account',
  identityId: 'identity',
  deviceId: 'device',
  spaceId: 'chat',
  replyTo: null,
  thread: false,
};
export const asset = (id: number, size = 100): AttachmentAsset => ({
  uri: `file:///owned/selection-${id}/qa.pdf`,
  name: `qa-${id}.pdf`,
  mime: 'application/pdf',
  size,
});
export const receipt = (id: string): AttachmentReceipt => ({
  itemId: `item-${id}`,
  messageId: `msg-${id}`,
});
export function fixture(capturedScope = scope) {
  const calls: { scope: AttachmentDraftScope; item: AttachmentDraftItem }[] = [];
  const discarded: string[] = [];
  let available = true;
  let liveScope = { ...capturedScope };
  let sequence = 0;
  const port: AttachmentDraftPort = {
    current: (captured) =>
      available &&
      captured.mode === liveScope.mode &&
      captured.identityId === liveScope.identityId &&
      captured.deviceId === liveScope.deviceId &&
      captured.spaceId === liveScope.spaceId,
    mutationId: () => `intent-${++sequence}`,
    send: async (scope, item) => {
      calls.push({ scope, item });
      return receipt(item.mutationId);
    },
    discard: async (uri) => {
      discarded.push(uri);
    },
  };
  const draft = new AttachmentDraft(capturedScope, port);
  return {
    draft,
    port,
    calls,
    discarded,
    setAvailable: (value: boolean) => {
      available = value;
    },
    setScope: (value: AttachmentDraftScope) => {
      liveScope = value;
    },
    pick: (...items: AttachmentAsset[]) => draft.pick(async () => items),
  };
}
export const kind = (expected: string) => (error: unknown) =>
  error instanceof AttachmentDraftFailure && error.kind === expected;
