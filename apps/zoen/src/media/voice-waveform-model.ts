import { type FrameRange, type VoiceDraft, type VoiceEditCommand } from './voice-edit.ts';

export type Props = {
  draft: VoiceDraft;
  onEdit: (commands: VoiceEditCommand | readonly VoiceEditCommand[]) => void;
  onTrimPreviewChange: (draft: VoiceDraft, pending: boolean, range: FrameRange | null) => void;
  pendingTrim?: FrameRange | null;
  disabled?: boolean;
  playheadOriginalFrame?: number | null;
  onSeekOriginal?: (observedDraft: VoiceDraft, frame: number) => void;
  onPlayFromSelection?: (observedDraft: VoiceDraft, range: FrameRange) => void;
  auditionDisabled?: boolean;
  auditionLabels?: {
    position: string;
    hint: string;
    playFromSelection: string;
    seekBackward: string;
    seekForward: string;
  };
};
export type Preview = { draft: VoiceDraft; range: FrameRange };
export type Drag = {
  draft: VoiceDraft;
  kind: 'start' | 'end' | 'selection';
  width: number;
  origin: number;
  trim: FrameRange;
  selection: Preview | null;
};
export const GUTTER = 22;
export const clamp = (value: number, min: number, max: number) =>
  Math.max(min, Math.min(max, Math.round(value)));
