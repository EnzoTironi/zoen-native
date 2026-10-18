import type { FileDto } from '../core/generated/FileDto';
import type { VoiceNativePort } from './voice-session';

// The current UniFFI API takes one contiguous payload. Keep its native copies
// within a predictable mobile memory budget until the Rust API supports streams.
export const MAX_SELECTED_BYTES = 64 * 1024 * 1024;

export type FileSelection = {
  uri: string;
  name: string;
  mime: string;
  size?: number;
};

export type SendFile = FileSelection & {
  spaceId: string;
  replyTo: string | null;
  thread: boolean;
  /** One immutable selection intent, retained across failed-send retries. */
  mutationId: string;
};
export type SendVoice = {
  spaceId: string;
  uri: string;
  mime: string;
  durationMs: number;
  levels: number[];
  transcript?: string;
  voiceSourceId?: string;
  replyTo: string | null;
  thread: boolean;
};

/** A disposable, verified local copy of one specific signed file version. */
export type LocalMediaFile = {
  uri: string;
  itemId: string;
  version: number;
  info: FileDto;
};

/** Bytes stay in native storage. The bridge carries local URIs and small metadata only. */
export interface MediaDriver {
  voiceEditor?: VoiceNativePort;
  sendFile(selection: SendFile): Promise<{ itemId: string; messageId: string }>;
  sendVoice(recording: SendVoice): Promise<{ messageId: string; itemId: string }>;
  fileInfo(itemId: string, version?: number): Promise<FileDto>;
  materializeFile(itemId: string, version?: number): Promise<LocalMediaFile | null>;
  /** Resolves after the native viewer closes, while retaining its own preview copy. */
  previewFile?(file: LocalMediaFile): Promise<void>;
  releaseLocalFile(uri: string): Promise<void>;
}
