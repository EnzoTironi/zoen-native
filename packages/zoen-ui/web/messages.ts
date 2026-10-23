import type {
  MessageControllerOptions,
  PostInput,
  HistoryStatus,
} from './message-command-types.ts';
import { createReconciliation } from './message-reconciliation.ts';
import type { CursorReconciliation } from './cursor-reconciliation.ts';
// Messages remains the sole session, queue, transport and lifecycle owner.
import { type MessageCore, type MessageVault, type LinkedSession } from './message-operations.ts';
import {
  type MessagesState,
  type Conversation,
  type Invite,
  parseInvite,
} from './message-model.ts';
import { AuthenticatedTransport, type ConnectionState, type RelayFrame } from './transport.ts';
import { type HistoryResult } from './history.ts';
import {
  type VoiceMeta,
  type AttachmentIntent,
  type AttachmentBatchFile,
  type AttachmentBatchIntent,
  type FileDoc,
} from './message-file-model.ts';
import { array, integer, message } from './boundary.ts';
import type { MessageOperations } from './message-operations.ts';
import * as messageAttachments from './message-attachments.ts';
import * as messageBlobIo from './message-blob-io.ts';
import * as messageProfileOperations from './message-profile-operations.ts';
import * as messageHistoryOperations from './message-history-operations.ts';
import * as messageDomainRequests from './message-domain-requests.ts';
import * as messageConversationOperations from './message-conversation-operations.ts';
import * as messageLifecycle from './message-lifecycle.ts';
export type {
  MessageAttachmentMedia,
  FileDoc,
  Chunk,
  VoiceMeta,
  MessageMedia,
  AttachmentSendOptions,
  AttachmentIntent,
  AttachmentBatchIntent,
  AttachmentBatchFile,
} from './message-file-model.ts';
export type { Profile, Conversation, Message, Invite, MessagesState } from './message-model.ts';
export { parseConversation, parseDevices, parseInvite } from './message-model.ts';
export class Messages {
  private readonly core: MessageCore;
  private readonly vault: MessageVault;
  private readonly session: LinkedSession;
  private readonly changed: (state: MessagesState) => void;
  private readonly transport: AuthenticatedTransport;
  private readonly cursorRepair: CursorReconciliation;
  private serial: Promise<void> = Promise.resolve();
  private handleChanges: Promise<void> = Promise.resolve();
  private closed = false;
  private attachmentProducer: string | null = null;
  private connection: ConnectionState = { kind: 'connecting' };
  private generation = 0;
  private conversations: Conversation[] = [];
  private notice: string | null = null;
  private readonly aborted = new AbortController();
  private historyTimer: ReturnType<typeof setTimeout> | undefined;
  private maintenanceTimer: ReturnType<typeof setTimeout> | undefined;
  private maintenanceDue = 0;
  private historyRunning = false;
  private historyResult: HistoryResult | null = null;
  private historyError = 'O histórico aguarda o celular conectar.';
  private readonly historyWaiters = new Set<{
    resolve: (result: HistoryResult) => void;
    reject: (error: Error) => void;
    timer: ReturnType<typeof setTimeout>;
  }>();
  private readonly ephemeral:
    | ((frame: Extract<RelayFrame, { kind: 'ephemeral' | 'presence' }>) => void)
    | undefined;

  constructor({ core, vault, session, changed, ephemeral }: MessageControllerOptions) {
    this.ephemeral = ephemeral;
    this.core = core;
    this.vault = vault;
    this.session = session;
    this.changed = changed;
    this.transport = new AuthenticatedTransport(
      core,
      vault,
      session.account,
      (connection) => {
        if (this.closed) return;
        this.connection = connection;
        if (connection.kind !== 'ready') {
          this.cursorRepair.retire();
          ++this.generation;
          clearTimeout(this.maintenanceTimer);
        }
        if (connection.kind === 'failed') {
          this.fail(connection.message);
          return;
        }
        this.publish();
      },
      (frame, raw) => this.received(frame, raw),
    );
    this.cursorRepair = createReconciliation(this.#operations);
  }

  start(): Promise<void> {
    return messageLifecycle.start.call(this.#operations);
  }

  private publish(): void {
    if (!this.closed)
      this.changed({
        kind: 'active',
        connection: this.connection,
        conversations: this.conversations,
        notice: this.notice,
      });
  }

  private received(frame: RelayFrame, raw: Uint8Array): Promise<void> {
    return messageLifecycle.received.call(this.#operations, frame, raw);
  }

  private async apply(json: string, generation: number): Promise<void> {
    return messageLifecycle.apply.call(this.#operations, json, generation);
  }

  private scheduleMaintenance(delay: number | null, generation: number, onlyEarlier = false): void {
    return messageLifecycle.scheduleMaintenance.call(
      this.#operations,
      delay,
      generation,
      onlyEarlier,
    );
  }

  membershipRequest(command: unknown): Promise<unknown> {
    return this.domainRequest(command, 'membership_request');
  }

  send({ space, text }: { space: string; text: string }): Promise<void> {
    return this.act(space, { kind: 'queue_message', space, text, now: Date.now() }, true);
  }

  /** Post with an optional inline reply, thread reply or forward. Refusals stay non-fatal. */
  post({ space, text, replyTo = '', thread = false, forwarded = false }: PostInput): Promise<void> {
    return this.act(
      space,
      { kind: 'queue_post', space, text, replyTo, thread, forwarded, now: Date.now() },
      true,
    );
  }

  /** Reaction, edit or delete for everyone, in the shared annotation shape. */
  annotate(
    space: string,
    target: string,
    note: { React: { emoji: string } } | 'Unreact' | { Edit: { text: string } } | 'Delete',
  ): Promise<void> {
    return this.act(space, {
      kind: 'queue_annotation',
      space,
      target,
      note: JSON.stringify(note),
      now: Date.now(),
    });
  }

  /** Seal like native, upload every encrypted chunk, then post the file into the chat. */
  async sendFile(
    space: string,
    file: {
      name: string;
      mime: string;
      bytes: Uint8Array;
      thumb?: Uint8Array | null;
      voice?: VoiceMeta;
      attachment?: AttachmentIntent;
    },
    progress?: (done: number, total: number) => void,
  ): Promise<{ itemId: string; messageId: string }> {
    return messageAttachments.sendFile.call(this.#operations, space, file, progress);
  }

  /** All files and their exact ordered intent enter the existing outbox together. */
  async sendFiles(
    space: string,
    files: readonly AttachmentBatchFile[],
    intent: AttachmentBatchIntent,
  ): Promise<readonly { itemId: string; messageId: string }[]> {
    return messageAttachments.sendFiles.call(this.#operations, space, files, intent);
  }

  async newAttachmentMutationId(): Promise<string> {
    return messageAttachments.newAttachmentMutationId.call(this.#operations);
  }

  async releaseAttachmentMutation(mutationId: string, accepted: boolean): Promise<void> {
    return messageAttachments.releaseAttachmentMutation.call(
      this.#operations,
      mutationId,
      accepted,
    );
  }

  private async sendAttachment(
    space: string,
    intent: AttachmentIntent,
    framed: Uint8Array,
  ): Promise<{ itemId: string; messageId: string }> {
    return messageAttachments.sendAttachment.call(this.#operations, space, intent, framed);
  }

  private async sendAttachmentRequest<T>(
    kind: 'queue_attachment' | 'queue_attachment_batch',
    request: string,
    framed: Uint8Array,
    parse: (value: unknown) => T,
  ): Promise<T> {
    return (messageAttachments.sendAttachmentRequest<T>).call(
      this.#operations,
      kind,
      request,
      framed,
      parse,
    );
  }

  private async queueFileRecord(
    space: string,
    doc: FileDoc,
  ): Promise<{ itemId: string; messageId: string }> {
    return messageAttachments.queueFileRecord.call(this.#operations, space, doc);
  }

  async saveProfile(
    request: { name: string; bio: string; photo: 'keep' | 'remove' | 'set'; mime?: string },
    bytes: Uint8Array,
  ): Promise<void> {
    return messageProfileOperations.saveProfile.call(this.#operations, request, bytes);
  }

  async profilePhoto(
    identity: string,
    expected: string,
  ): Promise<{ bytes: Uint8Array; mime: string; sha256: string }> {
    return messageProfileOperations.profilePhoto.call(this.#operations, identity, expected);
  }

  async fileManifest(
    itemId: string,
    version?: number,
  ): Promise<{ itemId: string; version: number; doc: FileDoc; info: unknown }> {
    return messageAttachments.fileManifest.call(this.#operations, itemId, version);
  }

  /** The same encrypted chunks, named again in another chat: nothing to upload. */
  forwardFile(space: string, doc: FileDoc): Promise<void> {
    return messageAttachments.forwardFile.call(this.#operations, space, doc);
  }

  private async upload(blob: string, sealed: Uint8Array): Promise<void> {
    return messageBlobIo.upload.call(this.#operations, blob, sealed);
  }

  private base(): string {
    return new URL(this.session.account.relay).origin;
  }

  /** Fetches, checks and opens a file's chunks. Throws plain Portuguese on failure. */
  async openFile(input: FileDoc, thumbnail = false, pagePayload = false): Promise<Blob> {
    return messageBlobIo.openFile.call(this.#operations, input, thumbnail, pagePayload);
  }

  markRead(space: string): Promise<void> {
    return messageConversationOperations.markRead.call(this.#operations, space);
  }

  private act(
    space: string,
    operation: Parameters<MessageCore['text']>[0],
    allowOffline = false,
  ): Promise<void> {
    return messageConversationOperations.act.call(this.#operations, space, operation, allowOffline);
  }

  /** Shared Page/Plan commands execute inside Rust, then persist before any publication.
   * Downloaded CRDT payloads are verified by the signed descriptor and the Rust cache.
   * A large outgoing page has a durable upload barrier, including across tab reloads.
   */
  async documentRequest(command: unknown): Promise<unknown> {
    return this.domainRequest(command, 'document_request');
  }

  async agentRequest(command: unknown): Promise<unknown> {
    return this.domainRequest(command, 'agent_request');
  }

  async appRequest(command: unknown): Promise<unknown> {
    return this.domainRequest(command, 'app_request');
  }

  async integrityRequest(command: unknown): Promise<unknown> {
    return messageDomainRequests.integrityRequest.call(this.#operations, command);
  }

  async profileRequest(command: unknown): Promise<unknown> {
    return this.domainRequest(command, 'profile_request');
  }

  updateHandle(handle: string): Promise<string> {
    return messageProfileOperations.updateHandle.call(this.#operations, handle);
  }

  private scheduleHistory(delay = 0): void {
    return messageHistoryOperations.scheduleHistory.call(this.#operations, delay);
  }

  private async restoreHistory(): Promise<void> {
    return messageHistoryOperations.restoreHistory.call(this.#operations);
  }

  async historyStatus(): Promise<HistoryStatus> {
    return messageHistoryOperations.historyStatus.call(this.#operations);
  }

  receiveHistory(timeoutMs: number): Promise<HistoryResult> {
    return messageHistoryOperations.receiveHistory.call(this.#operations, timeoutMs);
  }

  private async domainRequest(
    command: unknown,
    domain:
      | 'document_request'
      | 'agent_request'
      | 'app_request'
      | 'profile_request'
      | 'membership_request'
      | 'link_request',
  ): Promise<unknown> {
    return messageDomainRequests.domainRequest.call(this.#operations, command, domain);
  }

  private async resumeDocumentUploads(generation: number): Promise<void> {
    return messageDomainRequests.resumeDocumentUploads.call(this.#operations, generation);
  }

  private async linkWork(command: unknown, generation: number): Promise<Record<string, unknown>> {
    return messageHistoryOperations.linkWork.call(this.#operations, command, generation);
  }

  /** Retained ciphertext is saved before PUT, and the same bytes survive a cold retry. */
  private async resumeOutgoingHistory(generation: number): Promise<void> {
    return messageHistoryOperations.resumeOutgoingHistory.call(this.#operations, generation);
  }

  linkRequest(command: unknown): Promise<unknown> {
    return this.domainRequest(command, 'link_request');
  }

  async linkDevice(code: string): Promise<{ device_id: string; check: string }> {
    return messageHistoryOperations.linkDevice.call(this.#operations, code);
  }

  async linkProgress(device: string): Promise<number[]> {
    return array(await this.linkRequest({ op: 'link_progress', device_id: device })).map(integer);
  }

  async sendHistory(device: string): Promise<number> {
    return messageHistoryOperations.sendHistory.call(this.#operations, device);
  }

  async loadOlder(
    space: string,
    timeoutMs: number,
  ): Promise<{ loaded: number; more: boolean; primary_offline: boolean; message: string | null }> {
    return messageHistoryOperations.loadOlder.call(this.#operations, space, timeoutMs);
  }

  async devices(): Promise<unknown> {
    if (this.closed) throw new Error('Esta sessão está fechada.');
    return this.transport.request('devices');
  }

  async rpc(kind: string, value = ''): Promise<unknown> {
    return messageHistoryOperations.rpc.call(this.#operations, kind, value);
  }

  async directoryProfiles(profiles: unknown[]): Promise<unknown> {
    return messageProfileOperations.directoryProfiles.call(this.#operations, profiles);
  }

  async typing(space: string, typing: boolean): Promise<void> {
    return messageConversationOperations.typing.call(this.#operations, space, typing);
  }

  async unlinkDevice(device: string): Promise<void> {
    return messageConversationOperations.unlinkDevice.call(this.#operations, device);
  }

  async preview(code: string): Promise<Invite> {
    if (this.closed) throw new Error('The browser session is closed.');
    return parseInvite(await this.transport.request('preview_invite', code));
  }

  join({ invite, code }: { invite: Invite; code: string }): Promise<void> {
    return messageConversationOperations.join.call(this.#operations, { invite, code });
  }

  private enqueue(
    operation: () => Promise<void>,
    refusal: () => string | null = () => null,
    fatal = true,
  ): Promise<void> {
    const task = this.serial.then(async () => {
      if (this.closed) throw new Error('The browser session is closed. Reload Zoen to reopen it.');
      const reason = refusal();
      if (reason) throw new Error(reason);
      try {
        await operation();
      } catch (error) {
        if (fatal || !(error as { soft?: boolean }).soft) this.fail(message(error));
        throw error;
      }
    });
    this.serial = task.catch(() => {});
    return task;
  }

  private fail(reason: string): void {
    if (this.closed) return;
    this.stop();
    this.core.close();
    this.changed({ kind: 'failed', message: reason, conversations: this.conversations });
  }

  stop(): void {
    this.closed = true;
    this.aborted.abort();
    clearTimeout(this.historyTimer);
    clearTimeout(this.maintenanceTimer);
    this.cursorRepair.retire();
    for (const waiter of this.historyWaiters) {
      clearTimeout(waiter.timer);
      waiter.reject(new Error('Esta sessão está fechada.'));
    }
    this.historyWaiters.clear();
    ++this.generation;
    this.transport.stop();
  }

  get #operations(): MessageOperations {
    return Messages.#borrow(this);
  }

  static #borrow(owner: Messages): MessageOperations {
    return {
      get cursorRepair() {
        return owner.cursorRepair;
      },
      get core() {
        return owner.core;
      },
      get closed() {
        return owner.closed;
      },
      get conversations() {
        return owner.conversations;
      },
      set conversations(value: Conversation[]) {
        owner.conversations = value;
      },
      get generation() {
        return owner.generation;
      },
      get attachmentProducer() {
        return owner.attachmentProducer;
      },
      set attachmentProducer(value: string | null) {
        owner.attachmentProducer = value;
      },
      get connection() {
        return owner.connection;
      },
      get notice() {
        return owner.notice;
      },
      set notice(value: string | null) {
        owner.notice = value;
      },
      get transport() {
        return owner.transport;
      },
      get vault() {
        return owner.vault;
      },
      get aborted() {
        return owner.aborted;
      },
      get session() {
        return owner.session;
      },
      get handleChanges() {
        return owner.handleChanges;
      },
      set handleChanges(value: Promise<void>) {
        owner.handleChanges = value;
      },
      get historyResult() {
        return owner.historyResult;
      },
      set historyResult(value: HistoryResult | null) {
        owner.historyResult = value;
      },
      get historyRunning() {
        return owner.historyRunning;
      },
      set historyRunning(value: boolean) {
        owner.historyRunning = value;
      },
      get historyTimer() {
        return owner.historyTimer;
      },
      set historyTimer(value: ReturnType<typeof setTimeout> | undefined) {
        owner.historyTimer = value;
      },
      get historyWaiters() {
        return owner.historyWaiters;
      },
      get historyError() {
        return owner.historyError;
      },
      set historyError(value: string) {
        owner.historyError = value;
      },
      get ephemeral() {
        return owner.ephemeral?.bind(owner);
      },
      get maintenanceTimer() {
        return owner.maintenanceTimer;
      },
      set maintenanceTimer(value: ReturnType<typeof setTimeout> | undefined) {
        owner.maintenanceTimer = value;
      },
      get maintenanceDue() {
        return owner.maintenanceDue;
      },
      set maintenanceDue(value: number) {
        owner.maintenanceDue = value;
      },
      get sendAttachment() {
        return owner.sendAttachment.bind(owner);
      },
      get upload() {
        return owner.upload.bind(owner);
      },
      get queueFileRecord() {
        return owner.queueFileRecord.bind(owner);
      },
      get enqueue() {
        return owner.enqueue.bind(owner);
      },
      get apply() {
        return owner.apply.bind(owner);
      },
      get sendAttachmentRequest() {
        return owner.sendAttachmentRequest.bind(owner);
      },
      get resumeDocumentUploads() {
        return owner.resumeDocumentUploads.bind(owner);
      },
      get scheduleMaintenance() {
        return owner.scheduleMaintenance.bind(owner);
      },
      get publish() {
        return owner.publish.bind(owner);
      },
      get documentRequest() {
        return owner.documentRequest.bind(owner);
      },
      get act() {
        return owner.act.bind(owner);
      },
      get base() {
        return owner.base.bind(owner);
      },
      get rpc() {
        return owner.rpc.bind(owner);
      },
      get restoreHistory() {
        return owner.restoreHistory.bind(owner);
      },
      get scheduleHistory() {
        return owner.scheduleHistory.bind(owner);
      },
      get linkWork() {
        return owner.linkWork.bind(owner);
      },
      get linkRequest() {
        return owner.linkRequest.bind(owner);
      },
      get resumeOutgoingHistory() {
        return owner.resumeOutgoingHistory.bind(owner);
      },
      get openFile() {
        return owner.openFile.bind(owner);
      },
      get membershipRequest() {
        return owner.membershipRequest.bind(owner);
      },
    };
  }
}
