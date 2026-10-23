import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

// Source wiring guard only. Runtime capture/camera/CallKit acceptance still needs
// the integrated iOS target; this does not present a model as device execution.
const source = (file: string) =>
  readFileSync(new URL(`../../modules/zoen-core/ios/${file}`, import.meta.url), 'utf8');
const session = source('ZoenSession.swift');
const before = (body: string, first: string, later: string) => {
  const start = body.indexOf(first);
  const end = body.indexOf(later);
  assert.ok(start >= 0 && end >= 0 && start < end, `${first} must precede ${later}`);
};

test('close, forced escalation and erase wire media retirement before fallible or suspended editor cleanup', () => {
  const erase = session.slice(
    session.indexOf('  func eraseDevice('),
    session.indexOf('  func close('),
  );
  const eraseTask = erase.slice(erase.indexOf('    let task = Task {'));
  before(eraseTask, 'await stopCallsForClose()', 'await pageDrain.value');
  before(eraseTask, 'await stopCallsForClose()', 'try await filePreview.clear');
  const close = session.slice(
    session.indexOf('  func close('),
    session.indexOf('  private func finishClose('),
  );
  const escalation = close.slice(
    close.indexOf('    if let closeTask {'),
    close.indexOf('    // Fence new work'),
  );
  before(escalation, 'await stopCallsForClose()', 'try await filePreview.clear');
  const closeTask = close.slice(close.indexOf('    let task = Task {'));
  before(closeTask, 'await self.stopCallsForClose()', 'await ZoenKeyboardPanelHost.retireAll()');
  before(closeTask, 'await self.stopCallsForClose()', 'try await self.filePreview.clear');
});

test('native RTC retirement disables capture and closes the peer before waiting for camera release', () => {
  const stop = source('Calls/ZoenCallRTC.swift')
    .split('  func stop() async {')[1]!
    .split('\nextension ')[0]!;
  for (const command of [
    'live = false',
    'microphone?.isEnabled = false',
    'cameraTrack?.isEnabled = false',
    'isAudioEnabled = false',
    'peer?.close()',
  ]) {
    before(stop, command, 'await withCheckedContinuation');
  }
});

test('CallKit presentation revalidates canonical contact policy after actor and audio handoff awaits', () => {
  const begin = source('Calls/ZoenCallCoordinator.swift')
    .split('  func begin(outgoing: Bool) async throws {')[1]!
    .split('  private func watchCanonicalLease()')[0]!;
  before(begin, 'port.presentationRecord()', 'ZoenCallSystem.shared.install(self)');
  before(begin, 'port.presentationRecord()', 'provider.reportNewIncomingCall');
  const outgoing = begin.split('try await port.prepareAudio()')[1]!;
  before(outgoing, 'port.presentationRecord()', 'controller.request(');
});

test('failure before system installation projects terminal presentation and clears the scoped session slot', () => {
  const coordinator = source('Calls/ZoenCallCoordinator.swift');
  const stop = coordinator.split('  func stop(writeEnd:')[1]!.split('    let releaseAudio')[0]!;
  assert.match(
    stop,
    /guard installed else \{ retired = true; presentation\.retire\(\); status\("ended"\); return \}/,
  );
  const snapshot = coordinator
    .split('  func presentationSnapshot()')[1]!
    .split('  private func phase')[0]!;
  assert.match(snapshot, /state: presentation\.snapshotState\(currentPhase\)/);
  const sessionCalls = source('ZoenSession+Calls.swift');
  const changed = sessionCalls
    .split('  private func callPresentationChanged(')[1]!
    .split('  func currentCallPresentation(')[0]!;
  assert.match(
    changed,
    /if snapshot\.state == "ended" \{\s*callCoordinator = nil; callsScope = nil/,
  );
  const begin = sessionCalls
    .split('  private func presentCall(')[1]!
    .split('  private func callPresentationChanged(')[0]!;
  assert.match(
    begin,
    /catch \{ await coordinator\.stop\(writeEnd: true, reason: \.failed\); throw error \}/,
  );
});

test('camera selection serializes capture and hides preview during switching, off and retirement', () => {
  const rtc = source('Calls/ZoenCallRTC.swift');
  const selection = rtc
    .split('  func setCameraFacing(')[1]!
    .split('  private func publishLocalPreview')[0]!;
  before(selection, 'cameraTrack?.isEnabled = false', 'enqueue {');
  before(selection, 'localVideo(nil, capturingFront)', 'enqueue {');
  before(selection, 'await rtc.stopCamera()', 'try await rtc.startCamera()');
  assert.equal(selection.match(/revision == rtc.cameraSwitch/g)?.length, 2);
  const publish = rtc
    .split('  private func publishLocalPreview()')[1]!
    .split('  func setMuted(')[0]!;
  assert.match(
    publish,
    /intent.cameraLive\(allowed: cameraAllowed, started: cameraStarted, live: live\)/,
  );
  assert.match(publish, /localVideo\(enabled \? cameraTrack : nil, capturingFront\)/);
  const capture = rtc.split('  private func startCamera()')[1]!.split('  func stop()')[0]!;
  before(capture, 'capturingFront = device.position == .front', 'camera.startCapture');
  before(capture, '_ = try await current()', 'publishLocalPreview()');
  const screen = source('Calls/ZoenCallViewController.swift');
  assert.match(
    screen,
    /localPreview.transform = CGAffineTransform\(scaleX: front \? -1 : 1, y: 1\)/,
  );
  const hide = screen
    .split('override func viewWillDisappear(')[1]!
    .split('  func presentIfNeeded(')[0]!;
  assert.match(hide, /localTrack\?\.remove\(localPreview\)/);
  const stop = rtc.split('  func stop() async')[1]!.split('\nextension ')[0]!;
  assert.match(stop, /localVideo\(nil, intent.frontCamera\)/);
});
