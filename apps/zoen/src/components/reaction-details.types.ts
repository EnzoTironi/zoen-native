import type { TimelineEntry } from '../core/types';

export type ReactionDetailsProps = Readonly<{
  entry: TimelineEntry | null;
  mode: 'participants' | 'emoji';
  selectedEmoji?: string;
  canGoBack?: boolean;
  pending?: boolean;
  error?: string | null;
  onClose: () => void;
  onSelectEmoji: (emoji: string) => void;
  onChooseEmoji: () => void;
  onBack: () => void;
  onReact: (emoji: string | null) => void;
}>;
