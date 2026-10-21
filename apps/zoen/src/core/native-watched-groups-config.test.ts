import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { after, before, test } from 'node:test';

const require = createRequire(import.meta.url);
const xcode = require('xcode');
const { reconcileWatchedGroups } = require('../../plugins/with-native-surfaces.cjs');
const root = mkdtempSync(path.join(tmpdir(), 'zoen-watched-groups-'));
const upstream = path.dirname(require.resolve('@expo/inline-modules/package.json'));
const installedSourceBefore = readFileSync(path.join(upstream, 'build/xcodeProjectUpdates.js'));
const patch = new URL('../../patches/@expo+inline-modules+0.2.7.patch', import.meta.url);
const projectId = 'AAAAAAAAAAAAAAAAAAAAAAAA';
const groupId = 'BBBBBBBBBBBBBBBBBBBBBBBB';
const originalId = 'CCCCCCCCCCCCCCCCCCCCCCCC';
const aliasId = 'DDDDDDDDDDDDDDDDDDDDDDDD';
const targetId = 'EEEEEEEEEEEEEEEEEEEEEEEE';
const otherTarget = 'FFFFFFFFFFFFFFFFFFFFFFFF';
const otherGroup = '111111111111111111111111';
const configId = '222222222222222222222222';
const nestedGroup = '333333333333333333333333';
const firstException = '444444444444444444444444';
const secondException = '555555555555555555555555';
let originalUpdate: (root: string, options: object) => Promise<void>;
let patchedUpdate: typeof originalUpdate;

function packageCopy(name: string) {
  const directory = path.join(root, name);
  const destination = path.join(directory, 'node_modules/@expo/inline-modules');
  mkdirSync(path.join(destination, 'build'), { recursive: true });
  for (const file of ['package.json', 'build/xcodeProjectUpdates.js'])
    writeFileSync(path.join(destination, file), readFileSync(path.join(upstream, file)));
  symlinkSync(
    path.dirname(require.resolve('@expo/config-plugins/package.json')),
    path.join(directory, 'node_modules/@expo/config-plugins'),
  );
  writeFileSync(
    path.join(directory, 'package.json'),
    JSON.stringify({
      name: `watched-groups-${name}`,
      private: true,
      dependencies: { '@expo/inline-modules': '0.2.7' },
    }),
  );
  return directory;
}

function applyPatch(directory: string, reverse = false) {
  const patches = path.join(directory, 'patches');
  mkdirSync(patches, { recursive: true });
  writeFileSync(path.join(patches, path.basename(patch.pathname)), readFileSync(patch));
  const result = spawnSync(
    process.execPath,
    [
      require.resolve('patch-package/dist/index.js'),
      '--error-on-fail',
      ...(reverse ? ['--reverse'] : []),
    ],
    { cwd: directory, encoding: 'utf8', env: { ...process.env, CI: '1' } },
  );
  assert.equal(result.status, 0, result.stdout + result.stderr);
}

before(() => {
  assert.equal(require('@expo/inline-modules/package.json').version, '0.2.7');
  const original = packageCopy('original');
  const patched = packageCopy('patched');
  applyPatch(original, true);
  applyPatch(patched);
  const target = path.join(
    patched,
    'node_modules/@expo/inline-modules/build/xcodeProjectUpdates.js',
  );
  assert.equal(
    createHash('sha256')
      .update(
        readFileSync(
          path.join(original, 'node_modules/@expo/inline-modules/build/xcodeProjectUpdates.js'),
        ),
      )
      .digest('hex'),
    '1f8b6b02c574d6f32b82c43a942bce0b1a183b59d94fc897dd11fc49684d4db8',
  );
  assert.equal(
    createHash('sha256').update(readFileSync(target)).digest('hex'),
    'b78d656073f53a8ff116cc5d8a71799a6d6372be7088713d56a99515ba2ad12f',
  );
  const once = readFileSync(target);
  applyPatch(patched);
  assert.deepEqual(readFileSync(target), once, 'postinstall reapplication must be idempotent');
  originalUpdate = require(
    path.join(original, 'node_modules/@expo/inline-modules/build/xcodeProjectUpdates.js'),
  ).updateXcodeProject;
  patchedUpdate = require(target).updateXcodeProject;
});
after(() => rmSync(root, { recursive: true, force: true }));

function fixture() {
  const project = xcode.project('fixture.pbxproj');
  project.hash = {
    project: {
      archiveVersion: 1,
      classes: {},
      objectVersion: 77,
      rootObject: projectId,
      objects: {
        PBXProject: {
          [projectId]: {
            isa: 'PBXProject',
            mainGroup: groupId,
            targets: [{ value: targetId }, { value: otherTarget }],
          },
        },
        PBXGroup: {
          [groupId]: {
            isa: 'PBXGroup',
            children: [
              { value: originalId, comment: 'original' },
              { value: otherGroup },
              { value: aliasId, comment: 'alias' },
            ],
            sourceTree: '"<group>"',
          },
          [nestedGroup]: {
            isa: 'PBXGroup',
            children: [{ value: aliasId }],
            sourceTree: '"<group>"',
          },
        },
        PBXFileSystemSynchronizedRootGroup: {
          // Deliberately reverse object order: the first existing main-group position wins.
          [aliasId]: {
            isa: 'PBXFileSystemSynchronizedRootGroup',
            path: '../app-intents',
            sourceTree: 'SOURCE_ROOT',
            name: 'app-intents',
            exceptions: [{ value: secondException }],
            explicitFolders: ['More'],
            explicitFileTypes: { 'Other.swift': 'sourcecode.swift' },
          },
          [originalId]: {
            isa: 'PBXFileSystemSynchronizedRootGroup',
            path: '"../app-intents"',
            sourceTree: '"SOURCE_ROOT"',
            exceptions: [{ value: firstException }],
            explicitFolders: ['Existing'],
            explicitFileTypes: { 'Original.swift': 'sourcecode.swift' },
          },
          [`${aliasId}_comment`]: 'alias',
          [otherGroup]: {
            isa: 'PBXFileSystemSynchronizedRootGroup',
            path: '"../other"',
            sourceTree: 'SOURCE_ROOT',
            explicitFolders: [],
            explicitFileTypes: {},
          },
        },
        PBXNativeTarget: {
          [targetId]: {
            isa: 'PBXNativeTarget',
            name: 'Zoen',
            productType: '"com.apple.product-type.application"',
            fileSystemSynchronizedGroups: [{ value: originalId }, { value: aliasId }],
          },
          [otherTarget]: {
            isa: 'PBXNativeTarget',
            name: 'Extension',
            fileSystemSynchronizedGroups: [{ value: aliasId }, { value: otherGroup }],
          },
        },
        XCBuildConfiguration: {
          [configId]: {
            isa: 'XCBuildConfiguration',
            name: 'Debug',
            baseConfigurationReferenceAnchor: aliasId,
            baseConfigurationReferenceRelativePath: 'Config.xcconfig',
            buildSettings: { KEEP: 'YES' },
          },
        },
        PBXFileSystemSynchronizedBuildFileExceptionSet: {
          [firstException]: {
            isa: 'PBXFileSystemSynchronizedBuildFileExceptionSet',
            target: targetId,
            membershipExceptions: ['One.swift'],
          },
          [secondException]: {
            isa: 'PBXFileSystemSynchronizedBuildFileExceptionSet',
            target: otherTarget,
            membershipExceptions: ['Two.swift'],
          },
        },
      },
    },
  };
  return project;
}

function reconcile(project: ReturnType<typeof fixture>, watched = ['app-intents']) {
  return reconcileWatchedGroups({
    modResults: project,
    experiments: { inlineModules: { watchedDirectories: watched } },
  });
}

function writeProject(name: string, project: ReturnType<typeof fixture>) {
  const directory = path.join(root, name);
  const file = path.join(directory, 'ios/Zoen.xcodeproj/project.pbxproj');
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, project.writeSync());
  return { directory, file };
}

function parse(file: string) {
  const project = xcode.project(file);
  project.parseSync();
  return project;
}

function synchronizedIds(project: ReturnType<typeof fixture>) {
  return Object.keys(project.hash.project.objects.PBXFileSystemSynchronizedRootGroup).filter(
    (key) => !key.endsWith('_comment'),
  );
}

function singleQuotedFixture() {
  const project = fixture();
  const objects = project.hash.project.objects;
  delete objects.PBXFileSystemSynchronizedRootGroup[aliasId];
  delete objects.PBXFileSystemSynchronizedRootGroup[`${aliasId}_comment`];
  objects.PBXGroup[groupId].children = [{ value: originalId }, { value: otherGroup }];
  objects.PBXGroup[nestedGroup].children = [];
  objects.PBXNativeTarget[targetId].fileSystemSynchronizedGroups = [{ value: originalId }];
  objects.PBXNativeTarget[otherTarget].fileSystemSynchronizedGroups = [{ value: otherGroup }];
  objects.XCBuildConfiguration[configId].baseConfigurationReferenceAnchor = originalId;
  return project;
}

test('actual node-xcode quoted-path negative adds a duplicate; patched upstream reuses the original UUID', async () => {
  const old = writeProject('negative', singleQuotedFixture());
  assert.equal(
    parse(old.file).hash.project.objects.PBXFileSystemSynchronizedRootGroup[originalId].path,
    '"../app-intents"',
  );
  await originalUpdate(old.directory, { name: 'Zoen', watchedDirectories: ['app-intents'] });
  assert.equal(
    synchronizedIds(parse(old.file)).length,
    3,
    'retained negative reproduces SDK duplicate',
  );
  const fixed = writeProject('positive', singleQuotedFixture());
  await patchedUpdate(fixed.directory, { name: 'Zoen', watchedDirectories: ['app-intents'] });
  const actual = parse(fixed.file);
  assert.deepEqual(synchronizedIds(actual).sort(), [originalId, otherGroup].sort());
  assert.deepEqual(
    actual.hash.project.objects.PBXNativeTarget[targetId].fileSystemSynchronizedGroups.map(
      ({ value }: { value: string }) => value,
    ),
    [originalId],
  );
  const first = readFileSync(fixed.file);
  await patchedUpdate(fixed.directory, { name: 'Zoen', watchedDirectories: ['app-intents'] });
  assert.deepEqual(readFileSync(fixed.file), first);
});

test('patched upstream registers new groups immediately and excludes other source-tree paths', async () => {
  const project = singleQuotedFixture();
  project.hash.project.objects.PBXFileSystemSynchronizedRootGroup[originalId].sourceTree =
    '"<group>"';
  const item = writeProject('different-source-tree', project);
  await patchedUpdate(item.directory, {
    name: 'Zoen',
    watchedDirectories: ['app-intents', 'app-intents'],
  });
  const actual = parse(item.file);
  assert.equal(synchronizedIds(actual).length, 3);
  assert.equal(
    actual.hash.project.objects.PBXFileSystemSynchronizedRootGroup[originalId].sourceTree,
    '"<group>"',
  );
  assert.equal(
    actual.hash.project.objects.PBXNativeTarget[targetId].fileSystemSynchronizedGroups.length,
    2,
  );
});

test('migration keeps first existing UUID and every exception, folder, type, target and configuration anchor', () => {
  const project = fixture();
  const before = structuredClone(project.hash);
  reconcile(project);
  const objects = project.hash.project.objects;
  assert.deepEqual(synchronizedIds(project).sort(), [originalId, otherGroup].sort());
  assert.equal(objects.PBXFileSystemSynchronizedRootGroup[originalId].path, '../app-intents');
  assert.deepEqual(objects.PBXFileSystemSynchronizedRootGroup[originalId].exceptions, [
    { value: firstException },
    { value: secondException },
  ]);
  assert.deepEqual(objects.PBXFileSystemSynchronizedRootGroup[originalId].explicitFolders, [
    'Existing',
    'More',
  ]);
  assert.deepEqual(objects.PBXFileSystemSynchronizedRootGroup[originalId].explicitFileTypes, {
    'Original.swift': 'sourcecode.swift',
    'Other.swift': 'sourcecode.swift',
  });
  assert.deepEqual(objects.PBXGroup[groupId].children, [
    { value: originalId, comment: 'original' },
    { value: otherGroup },
  ]);
  assert.deepEqual(objects.PBXGroup[nestedGroup].children, [{ value: originalId }]);
  assert.deepEqual(objects.PBXNativeTarget[targetId].fileSystemSynchronizedGroups, [
    { value: originalId },
  ]);
  assert.deepEqual(objects.PBXNativeTarget[otherTarget].fileSystemSynchronizedGroups, [
    { value: originalId },
    { value: otherGroup },
  ]);
  assert.equal(objects.XCBuildConfiguration[configId].baseConfigurationReferenceAnchor, originalId);
  assert.deepEqual(
    objects.PBXFileSystemSynchronizedBuildFileExceptionSet,
    before.project.objects.PBXFileSystemSynchronizedBuildFileExceptionSet,
  );
  assert.deepEqual(
    objects.PBXFileSystemSynchronizedRootGroup[otherGroup],
    before.project.objects.PBXFileSystemSynchronizedRootGroup[otherGroup],
  );
  const once = structuredClone(project.hash);
  reconcile(project);
  assert.deepEqual(project.hash, once);
});

test('unwatched paths and other source trees are untouched', () => {
  const project = fixture();
  const before = structuredClone(project.hash);
  reconcile(project, ['not-configured']);
  assert.deepEqual(project.hash, before);
  project.hash.project.objects.PBXFileSystemSynchronizedRootGroup[aliasId].sourceTree = '"<group>"';
  const alias = structuredClone(
    project.hash.project.objects.PBXFileSystemSynchronizedRootGroup[aliasId],
  );
  reconcile(project);
  assert.deepEqual(project.hash.project.objects.PBXFileSystemSynchronizedRootGroup[aliasId], alias);
});

for (const kind of ['metadata', 'file-types', 'new-reference']) {
  test(`conflicting ${kind} refuses atomically, including earlier compatible watched groups`, () => {
    const project = fixture();
    if (kind === 'metadata') {
      project.hash.project.objects.PBXFileSystemSynchronizedRootGroup[originalId].indentWidth = '2';
      project.hash.project.objects.PBXFileSystemSynchronizedRootGroup[aliasId].indentWidth = '4';
    } else if (kind === 'file-types') {
      project.hash.project.objects.PBXFileSystemSynchronizedRootGroup[aliasId].explicitFileTypes = {
        '"Original.swift"': 'text',
      };
    } else project.hash.project.objects.PBXNativeTarget[otherTarget].unknownFutureAnchor = aliasId;
    const before = structuredClone(project.hash);
    assert.throws(
      () => reconcile(project, ['other', 'app-intents']),
      /Cannot reconcile watched Xcode groups/,
    );
    assert.deepEqual(project.hash, before);
  });
}

test('shared installed dependency remains byte-identical while isolated postinstall applies the pinned patch', () => {
  const source = readFileSync(path.join(upstream, 'build/xcodeProjectUpdates.js'));
  assert.equal(
    createHash('sha256').update(source).digest('hex'),
    createHash('sha256').update(installedSourceBefore).digest('hex'),
  );
});
