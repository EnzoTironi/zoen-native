import { setIsAudioActiveAsync, setAudioModeAsync, type AudioMode } from 'expo-audio';
import { callAudioFocus } from './call-audio-focus.ts';

/** Drain old mode changes before CallKit activation; late cleanup cannot steal it. */
export function setMediaAudioMode(mode: Partial<AudioMode>): Promise<void> {
  if (!callAudioFocus.available && mode.allowsRecording === false) return Promise.resolve();
  return callAudioFocus.run(async () => {
    await setIsAudioActiveAsync(true);
    if (!callAudioFocus.available) throw new Error('Audio is in use by a call.');
    await setAudioModeAsync(mode);
  });
}
