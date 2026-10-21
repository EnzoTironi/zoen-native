// SPDX-License-Identifier: GPL-2.0-or-later
// Translated presentation/gesture values and waveform binning from Telegram-iOS contributors.
// Revision f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838; upstream paths/hashes in the packet.
// The GPLv3 option is selected for integration; see LICENSES/GPL-3.0.txt.
/** Source-observed UI contracts, Telegram iOS f1dd7a2 (see the frozen reference receipt).
 * These are presentation/gesture values, never recording, retention or send policies. */
export const telegramAudio = Object.freeze({
  easeInOut: [0.42, 0, 0.58, 1] as const,
  accessoryHeight: 45,
  accessoryLeading: 55,
  accessoryLineWidth: 2,
  accessoryLineTop: 8,
  accessoryLineHeight: 35,
  accessoryTitleTop: 7,
  accessoryTextTop: 25,
  accessoryFont: 15,
  recordActivationMs: 190,
  moveCancel: -150,
  moveLock: -110,
  releaseCancel: -100,
  releaseLock: -60,
  releaseVelocity: -400,
  lockProgressDistance: 105,
  lockWidth: 40,
  lockHeight: 72,
  previewHeight: 40,
  previewWaveformHeight: 13,
  voiceControl: 44,
  voiceControlArea: 55,
  waveformBar: 2,
  waveformGap: 1,
  waveformPeak: 12,
  iconTransitionMs: 150,
  previewFadeMs: 200,
  cancelTransitionMs: 250,
  scrubbingFadeMs: 300,
});
export type RecordingGestureEnd = 'cancel' | 'lock' | 'review';
export function recordingMove(x: number, y: number): RecordingGestureEnd | null {
  if (x < telegramAudio.moveCancel) return 'cancel';
  if (y < telegramAudio.moveLock) return 'lock';
  return null;
}
export function recordingRelease(
  x: number,
  y: number,
  velocityX: number,
  velocityY: number,
): RecordingGestureEnd {
  // The release decision uses the dominant axis, exactly as the pinned mic recognizer.
  if (Math.abs(x) > Math.abs(y)) y = 0;
  else x = 0;
  if (velocityX < telegramAudio.releaseVelocity || x < telegramAudio.releaseCancel) return 'cancel';
  if (velocityY < telegramAudio.releaseVelocity || y < telegramAudio.releaseLock) return 'lock';
  // Zoen keeps explicit review → Send; releasing a held recording never posts it.
  return 'review';
}
export function recordingLockProgress(y: number): number {
  return Math.max(0, Math.min(1, -y / telegramAudio.lockProgressDistance));
}

/** One real sample contributes to one pixel bin. No resampling invented peaks or random fallback. */
export function audioWaveformBars(width: number, levels: readonly number[]): readonly number[] {
  if (!Number.isFinite(width) || width < 3) return [];
  const count = Math.min(2048, Math.floor(width / 3));
  const peaks = Array<number>(count).fill(0);
  let maximum = 0;
  for (let i = 0; i < levels.length; i++) {
    const input = levels[i];
    const sample = typeof input === 'number' && Number.isFinite(input) ? Math.max(0, input) : 0;
    const bin = Math.min(count - 1, Math.floor((i * count) / Math.max(1, levels.length)));
    peaks[bin] = Math.max(peaks[bin]!, sample);
    maximum = Math.max(maximum, sample);
  }
  return peaks.map((peak) => Math.max(2, (maximum ? (peak / maximum) * 12 : 0) - 1));
}

/** Pinned font metrics include the internal5pt/4pt text-view insets. */
export function composerVerticalInsets(
  fontSize: number,
): Readonly<{ top: number; bottom: number }> {
  if (fontSize <= 14) return { top: 11.5, bottom: 10.5 };
  if (fontSize <= 15) return { top: 10.5, bottom: 10.5 };
  if (fontSize <= 16) return { top: 10, bottom: 9.5 };
  return { top: 9.5, bottom: 9.5 };
}
