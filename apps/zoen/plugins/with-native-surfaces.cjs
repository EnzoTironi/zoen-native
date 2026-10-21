const { createRunOncePlugin, withPlugins, withXcodeProject } = require('expo/config-plugins');

function applyWidgetReleaseBuildSettings(config) {
  const project = config.modResults;
  const target = project.pbxTargetByName('ExpoWidgetsTarget');
  if (
    !target ||
    !['com.apple.product-type.app-extension', '"com.apple.product-type.app-extension"'].includes(
      target.productType,
    )
  )
    throw new Error('Widget Release settings require the generated widget extension target.');
  const list = project.pbxXCConfigurationList()[target.buildConfigurationList];
  const configurations = project.pbxXCBuildConfigurationSection();
  const releases = (list?.buildConfigurations ?? [])
    .map(({ value }) => configurations[value])
    .filter((configuration) => ['Release', '"Release"'].includes(configuration?.name));
  if (releases.length !== 1)
    throw new Error('Widget Release settings require exactly one Release configuration.');
  // Keep the widget executable inspectable by AppIntents metadata extraction.
  // Debug retains Xcode previews and every other upstream build setting.
  releases[0].buildSettings.ENABLE_DEBUG_DYLIB = 'NO';
  return config;
}

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
  // SDK 58 Xcode mods run newest first: register before the widget creator so
  // this action runs after its target/configurations exist.
  return withPlugins(withXcodeProject(config, applyWidgetReleaseBuildSettings), [
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
            displayName: 'Zoen app',
            description: 'A preview you choose. Content hidden by default.',
            ios: {
              supportedFamilies: [
                'systemSmall',
                'systemMedium',
                'systemLarge',
                'accessoryRectangular',
              ],
              initialLayout: 'src/native-surfaces/miniapp-widget.ios.tsx',
              configuration: {
                title: 'App',
                description: 'Choose a different app for each widget.',
                parameters: {
                  miniapp: {
                    title: 'App',
                    type: 'enum',
                    default: 'none',
                    values: [{ name: 'Choose an app', value: 'none' }],
                    dynamic: true,
                  },
                },
              },
            },
            android: null,
          },
        ],
      },
    ],
  ]);
}

module.exports = createRunOncePlugin(withNativeSurfaces, 'zoen-native-surfaces', '1.0.0');

module.exports.applyWidgetReleaseBuildSettings = applyWidgetReleaseBuildSettings;
