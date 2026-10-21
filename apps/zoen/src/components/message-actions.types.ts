import type { ReactNode } from 'react';
import type { TimelineEntry } from '../core/types';
import type { MessageContextRequest } from './message-context-ports';
export type MessageAnchor = Readonly<{ x: number; y: number; width: number; height: number }>;
export type MeasureMessageAnchor = () => Promise<MessageAnchor | null>;
export type MessageActionsProps = Readonly<{
  entry: TimelineEntry | null;
  mode: 'reactions' | 'actions';
  anchor?: MessageAnchor | null;
  nativeSource?: MessageContextRequest;
  /** Existing canonical account/focus/access/message check, never native authority. */
  isCurrent?: () => boolean;
  preview?: ReactNode;
  pending?: boolean;
  error?: string | null;
  onClose: () => void;
  onReact: (emoji: string | null) => void;
  onMore?: () => void;
  onReply: (entry: TimelineEntry) => void;
  onThread: (entry: TimelineEntry) => void;
  onForward?: (entry: TimelineEntry) => void;
  onCopy?: (entry: TimelineEntry) => void;
  onEdit?: (entry: TimelineEntry) => void;
  onDelete?: (entry: TimelineEntry) => void;
  onShare?: (entry: TimelineEntry) => void;
}>;
