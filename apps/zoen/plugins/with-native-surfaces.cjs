const { createRunOncePlugin, withPlugins } = require('expo/config-plugins');

/** Source-controlled CNG only: compact projections in a distinct app group; no shared engine DB. */
function withNativeSurfaces(config) {
  const bundle = config.ios?.bundleIdentifier;
  if (!bundle) throw new Error('Native surfaces require the app bundle identifier.');
  const watched = config.experiments?.inlineModules?.watchedDirectories ?? [];
  config.experiments = {
    ...config.experiments,
    inlineModules: {
      ...config.experiments?.inlineModules,
      watchedDirectories: [...new Set([...watched, 'app-intents'])],
    },
  };
  return withPlugins(config, [
    ['expo-app-intents', { directory: 'app-intents' }],
    [
      'expo-widgets',
      {
        bundleIdentifier: `${bundle}.widgets`,
        groupIdentifier: `group.${bundle}.widgets`,
        enablePushNotifications: false,
        frequentUpdates: false,
        widgets: [
          {
            name: 'ZoenMiniapp',
            displayName: 'Miniapp do Zoen',
            description: 'Uma prévia escolhida por você. Conteúdo oculto por padrão.',
            ios: {
              supportedFamilies: [
                'systemSmall',
                'systemMedium',
                'systemLarge',
                'accessoryRectangular',
              ],
              initialLayout: 'src/native-surfaces/miniapp-widget.ios.tsx',
            },
            android: null,
          },
        ],
      },
    ],
  ]);
}

module.exports = createRunOncePlugin(withNativeSurfaces, 'zoen-native-surfaces', '1.0.0');
