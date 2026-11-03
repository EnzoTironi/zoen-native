import type { ReplySelection } from '../core/generated/ReplySelection';
import type { ReactNode } from 'react';
import type { TimelineEntry } from '../core/types';
import type { MessageContextRequest } from './message-context-ports';
import type { ComponentProps } from 'react';
import type { Button } from '@expo/ui/swift-ui';
export type MessageActionRow = Readonly<{
  label: string;
  image: ComponentProps<typeof Button>['systemImage'];
  run: () => void;
  destructive?: boolean;
  disabled?: boolean;
  keepOpen?: boolean;
}>;
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
  /** Reply options share the same presentation, without message reaction controls. */
  options?: readonly MessageActionRow[];
  pending?: boolean;
  error?: string | null;
  onClose: () => void;
  onReact: (emoji: string | null) => void;
  onMore?: () => void;
  onReply: (entry: TimelineEntry) => void;
  onThread: (entry: TimelineEntry) => void;
  onInfo?: (entry: TimelineEntry) => void;
  onForward?: (entry: TimelineEntry) => void;
  onSelect?: (entry: TimelineEntry) => void;
  onSelectText?: (entry: TimelineEntry) => void | (() => void);
  onQuoteReview?: (entry: TimelineEntry, text: string, ranges: string) => Promise<ReplySelection>;
  onQuote?: (entry: TimelineEntry, selection: ReplySelection) => void;
  onCopy?: (entry: TimelineEntry) => void;
  onCopyLink?: (entry: TimelineEntry) => void;
  onEdit?: (entry: TimelineEntry) => void;
  onRecover?: (entry: TimelineEntry) => void;
  onDelete?: (entry: TimelineEntry) => void;
  onShare?: (entry: TimelineEntry) => void;
}>;
