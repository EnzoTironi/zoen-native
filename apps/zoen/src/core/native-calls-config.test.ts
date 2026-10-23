import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const require = createRequire(import.meta.url);
const {
  callPodfile,
  applyCallPodfile,
  applyCallInfoPlist,
} = require('../../plugins/with-native-calls.cjs');
const original =
  "platform :ios, '26.0'\ntarget 'Zoen' do\n  use_expo_modules!\nend\ntarget 'ExpoWidgetsTarget' do\nend\n";

test('call plugin adds a checksum-pinned HTTP podspec to the main target, preserving other targets and repeat prebuild', () => {
  const configured = callPodfile(original);
  assert.match(configured, /target 'Zoen' do\n  # @generated begin zoen-native-calls/);
  assert.match(configured, /pod 'ZoenWebRTC', :podspec => File.join/);
  assert.equal(configured.split("pod 'ZoenWebRTC'").length - 1, 1);
  assert.match(configured, /target 'ExpoWidgetsTarget' do\nend\n$/);
  assert.equal(callPodfile(configured), configured);
  const config = { modResults: { contents: original, language: 'ruby' } };
  assert.equal(applyCallPodfile(config), config);
  assert.equal(config.modResults.contents, configured);
  assert.throws(() => callPodfile("target 'Other' do\nend\n"), /main Zoen Podfile target/);
});

test('Expo autolinks only the source module; CocoaPods receives the real pinned binary source', () => {
  const module = JSON.parse(
    readFileSync(
      new URL('../../modules/zoen-core/expo-module.config.json', import.meta.url),
      'utf8',
    ),
  );
  assert.equal(module.apple.podspecPath, 'ios/ZoenCore.podspec');
  const pod = readFileSync(
    new URL('../../modules/zoen-core/ios/ZoenWebRTC.podspec', import.meta.url),
    'utf8',
  );
  assert.match(
    pod,
    /:http => 'https:\/\/github.com\/jitsi\/webrtc\/releases\/download\/v155\.0\.1\/WebRTC\.xcframework\.zip'/,
  );
  assert.match(
    pod,
    /:sha256 => '5189cc5864d917400174f5bcee3a62f27c55524a28bccec783575d5efbda2ae5'/,
  );
});

test('calls disclose microphone/camera use and preserve existing background capabilities without enabling VoIP prematurely', () => {
  const config = {
    modResults: { UIBackgroundModes: ['remote-notification', 'audio'] } as Record<string, any>,
  };
  applyCallInfoPlist(config);
  assert.deepEqual(config.modResults.UIBackgroundModes, ['remote-notification', 'audio']);
  assert.match(config.modResults.NSMicrophoneUsageDescription, /calls and voice or video messages/);
  assert.match(config.modResults.NSCameraUsageDescription, /video calls and photos or videos/);
  applyCallInfoPlist(config);
  assert.deepEqual(config.modResults.UIBackgroundModes, ['remote-notification', 'audio']);
  assert.throws(
    () => applyCallInfoPlist({ modResults: { UIBackgroundModes: 'audio' } }),
    /Invalid iOS background modes/,
  );
});
