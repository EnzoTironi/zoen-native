const { createRunOncePlugin, withInfoPlist } = require('expo/config-plugins');

// Native-stack and the admitted UIKit owner control system traits together.
// AppDelegate/SceneDelegate remain the SDK-generated Expo factory lifecycle.
function withNativeWindowPolicy(config) {
  return withInfoPlist(config, (mod) => {
    mod.modResults.UIViewControllerBasedStatusBarAppearance = true;
    return mod;
  });
}
module.exports = createRunOncePlugin(withNativeWindowPolicy, 'zoen-native-window-policy', '1.0.0');
