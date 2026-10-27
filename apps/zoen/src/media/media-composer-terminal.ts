import type { RecordingStatus } from 'expo-audio';
import type { MediaComposerRuntime } from './media-composer-contract';
import { failureMessage } from './media-composer-shared';

type Owner = Pick<
  MediaComposerRuntime,
  | 'recording'
  | 'stoppedCapture'
  | 'stopTask'
  | 'recorder'
  | 'mounted'
  | 'captureControl'
  | 'voiceControlNow'
  | 'getCaptureLease'
  | 'captureOwner'
  | 'setFailure'
>;

/** SDK58 clears duration before terminal events, and can replace its native URL on reset. */
export function rememberCaptureDuration(
  owner: Pick<Owner, 'recording' | 'stoppedCapture' | 'stopTask'>,
  duration: number,
) {
  const capture = owner.stoppedCapture.current;
  if (
    owner.recording.current &&
    !owner.stopTask.current &&
    capture &&
    !capture.stopped &&
    Number.isFinite(duration) &&
    duration > 0
  )
    capture.durationMs = Math.max(capture.durationMs, duration);
}

export function finishNativeCapture(owner: Owner, status: RecordingStatus) {
  const capture = owner.stoppedCapture.current;
  const lease = owner.getCaptureLease();
  if (
    !status.isFinished ||
    status.id !== owner.recorder.id ||
    !owner.mounted.current ||
    !owner.recording.current ||
    owner.stopTask.current ||
    !capture ||
    capture.stopped ||
    owner.captureControl.current !== owner.voiceControlNow.current ||
    lease?.owner !== owner.captureOwner
  )
    return;
  try {
    const current = owner.recorder.getStatus();
    if (current.isRecording) return;
    // A completed URL identifies this capture. A nil-URL reset is admissible
    // only when the SDK actually replaced its source; encoding failure must
    // leave this source unprepared. Never borrow the replacement recording.
    const uri = owner.recorder.uri ?? undefined;
    if (
      status.url
        ? status.url !== capture.uri
        : status.mediaServicesDidReset
          ? uri === capture.uri
          : !status.hasError || current.canRecord
    )
      return;
    capture.stopped = true;
    capture.terminal = true;
    if (uri && uri !== capture.uri) capture.replacementUri = uri;
    // This lease captured the initiating route's Stop. A newer render's Stop
    // could target a different channel; terminal delivery cannot rebind it.
    void lease.recover().catch(() => {});
    if (status.hasError && status.error) owner.setFailure(status.error);
  } catch (reason) {
    owner.setFailure(failureMessage(reason));
  }
}
