import type { CursorReconciliation } from './cursor-reconciliation.ts';
// Borrowed execution port. Every mutable field accessor delegates to Messages;
// this contains no state, snapshot, queue, timer or transport of its own.
import { type StoredSession } from './session.ts';
import { type CoreWorker } from './worker-client.ts';
import { type BrowserVault } from './vault.ts';
import { type Conversation } from './message-model.ts';
import { type ConnectionState, type AuthenticatedTransport, type RelayFrame } from './transport.ts';
import { type HistoryResult } from './history.ts';
import { type AttachmentIntent, type FileDoc } from './message-file-model.ts';

export type LinkedSession = Extract<StoredSession, { kind: 'linked' }>;
export type MessageCore = Pick<CoreWorker, 'text' | 'bytes' | 'close'>;
export type MessageVault = Pick<BrowserVault, 'save' | 'sign'>;
export interface MessageOperations {
  readonly cursorRepair: CursorReconciliation;
  readonly core: MessageCore;
  readonly closed: boolean;
  conversations: Conversation[];
  readonly generation: number;
  attachmentProducer: string | null;
  readonly connection: ConnectionState;
  notice: string | null;
  readonly transport: AuthenticatedTransport;
  readonly vault: MessageVault;
  readonly aborted: AbortController;
  readonly session: LinkedSession;
  handleChanges: Promise<void>;
  historyResult: HistoryResult | null;
  historyRunning: boolean;
  historyTimer: ReturnType<typeof setTimeout> | undefined;
  readonly historyWaiters: Set<{
    resolve: (result: HistoryResult) => void;
    reject: (error: Error) => void;
    timer: ReturnType<typeof setTimeout>;
  }>;
  historyError: string;
  readonly ephemeral:
    | ((frame: Extract<RelayFrame, { kind: 'ephemeral' | 'presence' }>) => void)
    | undefined;
  maintenanceTimer: ReturnType<typeof setTimeout> | undefined;
  maintenanceDue: number;
  sendAttachment(
    space: string,
    intent: AttachmentIntent,
    framed: Uint8Array,
  ): Promise<{ itemId: string; messageId: string }>;
  upload(blob: string, sealed: Uint8Array): Promise<void>;
  queueFileRecord(space: string, doc: FileDoc): Promise<{ itemId: string; messageId: string }>;
  enqueue(
    operation: () => Promise<void>,
    refusal?: () => string | null,
    fatal?: boolean,
  ): Promise<void>;
  apply(json: string, generation: number): Promise<void>;
  sendAttachmentRequest<T>(
    kind: 'queue_attachment' | 'queue_attachment_batch',
    request: string,
    framed: Uint8Array,
    parse: (value: unknown) => T,
  ): Promise<T>;
  resumeDocumentUploads(generation: number): Promise<void>;
  scheduleMaintenance(delay: number | null, generation: number, onlyEarlier?: boolean): void;
  publish(): void;
  documentRequest(command: unknown): Promise<unknown>;
  act(
    space: string,
    operation: Parameters<MessageCore['text']>[0],
    allowOffline?: boolean,
  ): Promise<void>;
  base(): string;
  rpc(kind: string, value?: string): Promise<unknown>;
  restoreHistory(): Promise<void>;
  scheduleHistory(delay?: number): void;
  linkWork(command: unknown, generation: number): Promise<Record<string, unknown>>;
  linkRequest(command: unknown): Promise<unknown>;
  resumeOutgoingHistory(generation: number): Promise<void>;
  openFile(input: FileDoc, thumbnail?: boolean, pagePayload?: boolean): Promise<Blob>;
  membershipRequest(command: unknown): Promise<unknown>;
}
