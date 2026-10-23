import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

// Source wiring guard only. Runtime capture/camera/CallKit acceptance still needs
// the integrated iOS target; this does not present a model as device execution.
const source = (file: string) =>
  readFileSync(new URL(`../../modules/zoen-core/ios/${file}`, import.meta.url), 'utf8');
const session = source('ZoenSession.swift');

test('close, forced escalation and erase wire media retirement before fallible or suspended editor cleanup', () => {
  const erase = session.slice(
    session.indexOf('  func eraseDevice('),
    session.indexOf('  func close('),
  );
  const eraseTask = erase.slice(erase.indexOf('    let task = Task {'));
  assert.ok(
    eraseTask.indexOf('await stopCallsForClose()') < eraseTask.indexOf('await pageDrain.value'),
  );
  assert.ok(
    eraseTask.indexOf('await stopCallsForClose()') <
      eraseTask.indexOf('try await filePreview.clear'),
  );
  const close = session.slice(
    session.indexOf('  func close('),
    session.indexOf('  private func finishClose('),
  );
  const escalation = close.slice(
    close.indexOf('    if let closeTask {'),
    close.indexOf('    // Fence new work'),
  );
  assert.ok(
    escalation.indexOf('await stopCallsForClose()') <
      escalation.indexOf('try await filePreview.clear'),
  );
  const closeTask = close.slice(close.indexOf('    let task = Task {'));
  assert.ok(
    closeTask.indexOf('await self.stopCallsForClose()') <
      closeTask.indexOf('await ZoenKeyboardPanelHost.retireAll()'),
  );
  assert.ok(
    closeTask.indexOf('await self.stopCallsForClose()') <
      closeTask.indexOf('try await self.filePreview.clear'),
  );
});

test('native RTC retirement disables capture and closes the peer before waiting for camera release', () => {
  const stop = source('Calls/ZoenCallRTC.swift')
    .split('  func stop() async {')[1]!
    .split('\nextension ')[0]!;
  const drain = stop.indexOf('await withCheckedContinuation');
  for (const command of [
    'live = false',
    'microphone?.isEnabled = false',
    'cameraTrack?.isEnabled = false',
    'isAudioEnabled = false',
    'peer?.close()',
  ]) {
    const position = stop.indexOf(command);
    assert.ok(position >= 0 && position < drain, command);
  }
});
