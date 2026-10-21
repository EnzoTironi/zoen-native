import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import test from 'node:test';

const require = createRequire(import.meta.url);
const { withXcodeProject } = require('expo/config-plugins');
const xcode = require('xcode');
const { applyWidgetReleaseBuildSettings } = require('../../plugins/with-native-surfaces.cjs');
const {
  addXCConfigurationList,
} = require('expo-widgets/plugin/build/ios/xcode/addXCConfigurationList.js');
const {
  addToPbxNativeTargetSection,
} = require('expo-widgets/plugin/build/ios/xcode/addToPbxNativeTargetSection.js');

const mainList = 'AAAAAAAAAAAAAAAAAAAAAAAA';
const mainDebug = 'BBBBBBBBBBBBBBBBBBBBBBBB';
const mainRelease = 'CCCCCCCCCCCCCCCCCCCCCCCC';
const globalRelease = 'DDDDDDDDDDDDDDDDDDDDDDDD';
const mainTarget = 'EEEEEEEEEEEEEEEEEEEEEEEE';

function projectFixture() {
  const project = xcode.project('in-memory-only.pbxproj');
  project.hash = {
    project: {
      objects: {
        PBXNativeTarget: {
          [mainTarget]: {
            isa: 'PBXNativeTarget',
            name: 'Zoen',
            productType: '"com.apple.product-type.application"',
            buildConfigurationList: mainList,
          },
        },
        XCConfigurationList: {
          [mainList]: {
            isa: 'XCConfigurationList',
            buildConfigurations: [{ value: mainDebug }, { value: mainRelease }],
          },
        },
        XCBuildConfiguration: {
          [mainDebug]: {
            name: 'Debug',
            buildSettings: { ENABLE_DEBUG_DYLIB: 'YES', SWIFT_OPTIMIZATION_LEVEL: '"-Onone"' },
          },
          [mainRelease]: {
            name: 'Release',
            buildSettings: { ENABLE_DEBUG_DYLIB: 'YES', SWIFT_OPTIMIZATION_LEVEL: '"-O"' },
          },
          [globalRelease]: {
            name: 'Release',
            buildSettings: { ENABLE_DEBUG_DYLIB: 'YES', OTHER_SETTING: 'unchanged' },
          },
        },
      },
    },
  };
  return project;
}

function addActualUpstreamWidget(project: ReturnType<typeof projectFixture>) {
  const list = addXCConfigurationList(project, {
    targetName: 'ExpoWidgetsTarget',
    currentProjectVersion: '1',
    bundleIdentifier: 'xyz.tironi.zoen.widgets',
    deploymentTarget: '26.0',
    marketingVersion: '0.1.0',
  });
  addToPbxNativeTargetSection(project, {
    targetName: 'ExpoWidgetsTarget',
    targetUuid: project.generateUuid(),
    productFile: { fileRef: project.generateUuid() },
    xCConfigurationList: list,
  });
  const configurations = project.pbxXCBuildConfigurationSection();
  const refs = list.xcConfigurationList.buildConfigurations as { value: string }[];
  return Object.fromEntries(
    refs.map(({ value }) => [configurations[value].name, configurations[value]]),
  );
}

test('installed upstream Release keeps its optimization and only opts its widget executable out of debug dylib', () => {
  const project = projectFixture();
  const widget = addActualUpstreamWidget(project);
  assert.equal(widget.Debug.buildSettings.SWIFT_OPTIMIZATION_LEVEL, '"-Onone"');
  assert.equal(widget.Release.buildSettings.SWIFT_OPTIMIZATION_LEVEL, '"-Onone"');
  const before = structuredClone(project.hash);
  applyWidgetReleaseBuildSettings({ modResults: project });
  const expected = structuredClone(before);
  const target = project.pbxTargetByName('ExpoWidgetsTarget');
  const list = project.pbxXCConfigurationList()[target.buildConfigurationList];
  const release = list.buildConfigurations.find(
    ({ value }: { value: string }) =>
      project.pbxXCBuildConfigurationSection()[value].name === 'Release',
  ).value;
  expected.project.objects.XCBuildConfiguration[release].buildSettings.ENABLE_DEBUG_DYLIB = 'NO';
  assert.deepEqual(project.hash, expected);
  assert.equal(widget.Debug.buildSettings.ENABLE_DEBUG_DYLIB, undefined);
  applyWidgetReleaseBuildSettings({ modResults: project });
  assert.deepEqual(project.hash, expected);
});

test('installed SDK58 mod chain must create the target before applying its Release override', async () => {
  const project = projectFixture();
  const config = withXcodeProject({ name: 'Zoen', slug: 'zoen' }, applyWidgetReleaseBuildSettings);
  withXcodeProject(config, (mod: { modResults: ReturnType<typeof projectFixture> }) => {
    addActualUpstreamWidget(mod.modResults);
    return mod;
  });
  await config.mods.ios.xcodeproj({ ...config, modResults: project, modRequest: {} });
  const target = project.pbxTargetByName('ExpoWidgetsTarget');
  const list = project.pbxXCConfigurationList()[target.buildConfigurationList];
  const release = list.buildConfigurations.find(
    ({ value }: { value: string }) =>
      project.pbxXCBuildConfigurationSection()[value].name === 'Release',
  ).value;
  assert.equal(
    project.pbxXCBuildConfigurationSection()[release].buildSettings.ENABLE_DEBUG_DYLIB,
    'NO',
  );

  const wrong = withXcodeProject(
    { name: 'Zoen', slug: 'zoen' },
    (mod: { modResults: ReturnType<typeof projectFixture> }) => {
      addActualUpstreamWidget(mod.modResults);
      return mod;
    },
  );
  withXcodeProject(wrong, applyWidgetReleaseBuildSettings);
  await assert.rejects(
    wrong.mods.ios.xcodeproj({ ...wrong, modResults: projectFixture(), modRequest: {} }),
    /generated widget extension target/,
  );
});

test('missing target or Release configuration exposes generation/order drift without touching the app', () => {
  const project = projectFixture();
  const before = structuredClone(project.hash);
  assert.throws(
    () => applyWidgetReleaseBuildSettings({ modResults: project }),
    /generated widget extension target/,
  );
  assert.deepEqual(project.hash, before);
  const widget = addActualUpstreamWidget(project);
  widget.Release.name = 'Profile';
  const generated = structuredClone(project.hash);
  assert.throws(
    () => applyWidgetReleaseBuildSettings({ modResults: project }),
    /exactly one Release/,
  );
  assert.deepEqual(project.hash, generated);
});
