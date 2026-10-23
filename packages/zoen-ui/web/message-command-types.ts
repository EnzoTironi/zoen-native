// Exact inline public command/result contracts, shared without a new state owner.
import type { MessageCore, MessageVault, LinkedSession } from './message-operations.ts';
import type { MessagesState } from './message-model.ts';
import type { RelayFrame } from './transport.ts';

export type MessageControllerOptions = {
  core: MessageCore;
  vault: MessageVault;
  session: LinkedSession;
  changed: (state: MessagesState) => void;
  ephemeral?: (frame: Extract<RelayFrame, { kind: 'ephemeral' | 'presence' }>) => void;
};

export type PostInput = {
  space: string;
  text: string;
  replyTo?: string;
  thread?: boolean;
  forwarded?: boolean;
};

export type HistoryStatus = {
  initial_pending: boolean;
  manifest_received: boolean;
  chunks_received: number;
  chunks_total: number;
  receive_active: boolean;
};
