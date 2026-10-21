const path = require('node:path');
const { isDeepStrictEqual } = require('node:util');
const {
  createRunOncePlugin,
  IOSConfig,
  withPlugins,
  withXcodeProject,
} = require('expo/config-plugins');

const unquote = IOSConfig.XcodeUtils.unquote;

function referenceValue(reference) {
  const value = typeof reference === 'string' ? reference : reference?.value;
  return typeof value === 'string' ? unquote(value) : value;
}

function semanticValue(value) {
  if (typeof value === 'string') return unquote(value);
  if (Array.isArray(value)) return value.map(semanticValue);
  if (value && typeof value === 'object')
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [unquote(key), semanticValue(item)]),
    );
  return value;
}

/** Repair SDK 58 aliases already emitted before the pinned inline-modules fix. */
function reconcileWatchedGroups(config) {
  const watched = config.experiments?.inlineModules?.watchedDirectories ?? [];
  if (watched.length === 0) return config;
  const project = config.modResults;
  const objects = project.hash.project.objects;
  const groups = objects.PBXFileSystemSynchronizedRootGroup ?? {};
  const mainGroup = project.getFirstProject().firstProject.mainGroup;
  const ordered = [
    ...(objects.PBXGroup?.[mainGroup]?.children ?? []).map(referenceValue),
    ...Object.keys(groups).filter((key) => !key.endsWith('_comment')),
  ];
  const aliases = new Map();
  const canonicalIds = new Set();
  const replacements = [];
  const conflict = (detail) => {
    throw new Error(`Cannot reconcile watched Xcode groups: ${detail}`);
  };
  for (const directory of new Set(watched)) {
    const expectedPath = path.join('..', directory);
    const ids = [...new Set(ordered)].filter((id) => {
      const group = groups[id];
      return (
        group &&
        typeof group.path === 'string' &&
        typeof group.sourceTree === 'string' &&
        unquote(group.path) === expectedPath &&
        unquote(group.sourceTree) === 'SOURCE_ROOT'
      );
    });
    if (ids.length === 0) continue;
    const canonical = ids[0];
    const merged = structuredClone(groups[canonical]);
    merged.path = expectedPath;
    merged.sourceTree = 'SOURCE_ROOT';
    canonicalIds.add(canonical);
    for (const id of ids.slice(1)) {
      aliases.set(id, canonical);
      for (const [key, value] of Object.entries(groups[id])) {
        if (key === 'path' || key === 'sourceTree') continue;
        if (!(key in merged)) {
          merged[key] = structuredClone(value);
        } else if (key === 'exceptions' || key === 'explicitFolders') {
          if (!Array.isArray(merged[key]) || !Array.isArray(value)) conflict(`${directory}.${key}`);
          const identity = key === 'exceptions' ? referenceValue : semanticValue;
          const seen = new Set(merged[key].map(identity));
          for (const item of value) {
            const itemId = identity(item);
            if (typeof itemId !== 'string') conflict(`${directory}.${key}`);
            if (!seen.has(itemId)) {
              merged[key].push(structuredClone(item));
              seen.add(itemId);
            }
          }
        } else if (key === 'explicitFileTypes') {
          if (
            !merged[key] ||
            !value ||
            Array.isArray(merged[key]) ||
            Array.isArray(value) ||
            typeof merged[key] !== 'object' ||
            typeof value !== 'object'
          )
            conflict(`${directory}.${key}`);
          for (const [file, type] of Object.entries(value)) {
            const existing = Object.keys(merged[key]).find(
              (candidate) => unquote(candidate) === unquote(file),
            );
            if (existing === undefined) merged[key][file] = type;
            else if (!isDeepStrictEqual(semanticValue(merged[key][existing]), semanticValue(type)))
              conflict(`${directory}.${key}.${file}`);
          }
        } else if (!isDeepStrictEqual(semanticValue(merged[key]), semanticValue(value))) {
          conflict(`${directory}.${key}`);
        }
      }
    }
    replacements.push([groups[canonical], merged]);
  }
  const updates = [];
  const hasAlias = (value) =>
    typeof value === 'string'
      ? aliases.has(unquote(value))
      : Array.isArray(value)
        ? value.some(hasAlias)
        : value && typeof value === 'object'
          ? Object.values(value).some(hasAlias)
          : false;
  for (const [, merged] of replacements)
    if (hasAlias(merged)) conflict('merged group references an alias');
  for (const [section, records] of Object.entries(objects)) {
    for (const [id, record] of Object.entries(records)) {
      if (id.endsWith('_comment') || !record || typeof record !== 'object' || aliases.has(id))
        continue;
      const field =
        section === 'PBXGroup'
          ? 'children'
          : section === 'PBXNativeTarget'
            ? 'fileSystemSynchronizedGroups'
            : section === 'XCBuildConfiguration'
              ? 'baseConfigurationReferenceAnchor'
              : undefined;
      if (field && field in record) {
        if (Array.isArray(record[field])) {
          const seen = new Set();
          const next = record[field].flatMap((reference) => {
            const original = referenceValue(reference);
            const target = aliases.get(original) ?? original;
            if (canonicalIds.has(target) && seen.has(target)) return [];
            seen.add(target);
            if (target === original) return [reference];
            return [typeof reference === 'string' ? target : { ...reference, value: target }];
          });
          updates.push([record, field, next]);
        } else if (aliases.has(referenceValue(record[field]))) {
          const target = aliases.get(referenceValue(record[field]));
          updates.push([
            record,
            field,
            typeof record[field] === 'string' ? target : { ...record[field], value: target },
          ]);
        }
      }
      // New Xcode reference kinds must be handled explicitly before deleting an alias.
      for (const [key, value] of Object.entries(record))
        if (key !== field && hasAlias(value))
          conflict(`${section}.${id}.${key} references an alias`);
    }
  }
  // All metadata and reference validation precedes the first mutation.
  for (const [group, merged] of replacements) Object.assign(group, merged);
  for (const [record, field, value] of updates) record[field] = value;
  for (const alias of aliases.keys()) {
    delete groups[alias];
    delete groups[`${alias}_comment`];
  }
  return config;
}

function applyWidgetReleaseBuildSettings(config) {
  reconcileWatchedGroups(config);
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
module.exports.reconcileWatchedGroups = reconcileWatchedGroups;
