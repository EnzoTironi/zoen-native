const { createRunOncePlugin, withPodfile, withInfoPlist } = require('expo/config-plugins');

const begin = '  # @generated begin zoen-native-calls';
const end = '  # @generated end zoen-native-calls';
const stanza = `${begin}\n  pod 'ZoenWebRTC', :podspec => File.join(__dir__, '../modules/zoen-core/ios/ZoenWebRTC.podspec')\n${end}`;

/** :podspec uses the checksum-pinned HTTP source; :path would ignore it. */
function callPodfile(contents) {
  const block =
    /  # @generated begin zoen-native-calls\n[\s\S]*?  # @generated end zoen-native-calls\n?/g;
  const clean = contents.replace(block, '');
  const target = /^target ['"]Zoen['"] do\r?$/m;
  if (!target.test(clean))
    throw new Error('Cannot configure native calls: main Zoen Podfile target was not found.');
  return clean.replace(target, (line) => `${line}\n${stanza}`);
}
function applyCallPodfile(config) {
  config.modResults.contents = callPodfile(config.modResults.contents);
  return config;
}
function applyCallInfoPlist(config) {
  config.modResults.NSMicrophoneUsageDescription =
    'Use your microphone for calls and voice or video messages you choose to send.';
  config.modResults.NSCameraUsageDescription =
    'Use your camera for video calls and photos or videos you choose to send.';
  const modes = config.modResults.UIBackgroundModes ?? [];
  if (!Array.isArray(modes) || modes.some((value) => typeof value !== 'string'))
    throw new Error('Invalid iOS background modes.');
  config.modResults.UIBackgroundModes = [...new Set([...modes, 'audio'])];
  return config;
}
function withNativeCalls(config) {
  return withInfoPlist(withPodfile(config, applyCallPodfile), applyCallInfoPlist);
}
module.exports = createRunOncePlugin(withNativeCalls, 'zoen-native-calls', '1.0.0');
module.exports.callPodfile = callPodfile;
module.exports.applyCallPodfile = applyCallPodfile;
module.exports.applyCallInfoPlist = applyCallInfoPlist;
