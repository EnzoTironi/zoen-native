import type { AudioClipPlayback, AudioClipPlaybackState } from './audio-clip-controller.ts';
import type { MessageAudioChannel } from './message-audio-channel.ts';
import type { PlaybackCoordinator, PlaybackIntent, PlaybackOwner } from './playback-owner.ts';
import type { LocalMediaFile } from './types.ts';

export type MessageAudioScope = Readonly<{
  identityId: string;
  deviceId: string;
  epoch: number;
}>;
export type MessageAudioSource = Readonly<{
  scope: MessageAudioScope;
  spaceId: string;
  messageId: string;
  itemId: string;
  sha256: string;
  durationMs: number;
}>;
export type MessageAudioRuntime = Readonly<{
  playback: AudioClipPlayback;
  /** Setup failures still return an owned runtime so disposal can be retried. */
  failure?: Error;
  /** The controller reached native Play; buffering need not be audible yet. */
  hasPlayRequest(): boolean;
  /** Fence callbacks immediately; drain seeks before releasing the SharedObject. */
  dispose(): Promise<void>;
}>;
export type MessageAudioPort = Readonly<{
  coordinator: PlaybackCoordinator;
  scope(): MessageAudioScope | null;
  authorized(source: MessageAudioSource): boolean;
  /** Read the canonical nondeleted message AND current file item; return its pinned version. */
  verify(source: MessageAudioSource): Promise<number>;
  materialize(source: MessageAudioSource, version: number): Promise<LocalMediaFile | null>;
  release(file: LocalMediaFile): Promise<void>;
  create(
    source: MessageAudioSource,
    file: LocalMediaFile,
    current: () => boolean,
  ): MessageAudioRuntime;
}>;
export type Metadata = Readonly<{ seconds: number; duration: number; rate: 1 | 1.5 | 2 }>;
export type Focus = Readonly<{ scope: string; spaceId: string }>;
export type Intent = {
  owner: PlaybackOwner;
  token: PlaybackIntent | null;
  handoff: boolean;
  cancelled: boolean;
};
export type Preparation = { channel: MessageAudioChannel; serial: number; intent: Intent };
export type Active = {
  channel: MessageAudioChannel;
  file: LocalMediaFile;
  runtime: MessageAudioRuntime;
  serial: number;
  unsubscribe: () => void;
  firstPlay: Intent | null;
  initial: boolean;
  startSeconds: number;
  validating: boolean;
  validationAgain: boolean;
  validationIntent: Intent | null;
};
export type Retired = {
  file: LocalMediaFile;
  runtime: MessageAudioRuntime | null;
  runtimeDisposed: boolean;
  task: Promise<void> | null;
};
export const scopeKey = (scope: MessageAudioScope | null) =>
  scope ? JSON.stringify([scope.identityId, scope.deviceId, scope.epoch]) : null;
export const sourceKey = (source: MessageAudioSource) =>
  JSON.stringify([
    source.scope.identityId,
    source.scope.deviceId,
    source.scope.epoch,
    source.spaceId,
    source.messageId,
    source.itemId,
    source.sha256,
  ]);
export const metadata = (source: MessageAudioSource): Metadata => ({
  seconds: 0,
  duration:
    Number.isFinite(source.durationMs) && source.durationMs > 0 ? source.durationMs / 1000 : 0,
  rate: 1,
});
export const snapshot = (value: Metadata, admitted = true): AudioClipPlaybackState =>
  Object.freeze({
    loaded: admitted,
    playing: false,
    buffering: false,
    currentSeconds: value.seconds,
    durationSeconds: value.duration,
    rate: value.rate,
    error: null,
    loadFailed: false,
    busy: false,
  });
