import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, realpathSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import { preparePackage } from './message-markdown-native.fixtures.ts';

const require = createRequire(import.meta.url);
test('Metro resolves the actual public input spec and Expo generates its complete static view config', () => {
  const app = fileURLToPath(new URL('../../', import.meta.url));
  const directory = mkdtempSync(join(tmpdir(), 'zoen-composer-public-spec-'));
  try {
    const pkg = preparePackage(app, directory);
    const resolve = require('metro-resolver').resolve;
    const defaults = require(join(app, 'node_modules/metro-config/src/defaults/defaults.js'));
    const facadePath = join(pkg, 'lib/module/EnrichedMarkdownTextInput.js');
    const specifier = readFileSync(facadePath, 'utf8').match(
      /from\s+['"]([^'"]*EnrichedMarkdownTextInputNativeComponent[^'"]*)['"]/,
    )?.[1];
    assert.ok(specifier, 'resolve the import used by the actual public facade');
    const resolution = resolve(
      {
        originModulePath: facadePath,
        sourceExts: defaults.sourceExts,
        assetExts: new Set(defaults.assetExts),
        preferNativePlatform: true,
        mainFields: ['react-native', 'browser', 'main'],
        getPackageForModule: () => null,
        fileSystemLookup: (path: string) => {
          try {
            const stat = statSync(path);
            return {
              exists: true,
              type: stat.isDirectory() ? 'd' : 'f',
              realPath: realpathSync(path),
            };
          } catch {
            return { exists: false };
          }
        },
      },
      specifier,
      'ios',
    );
    assert.equal(resolution.type, 'sourceFile');
    const source = readFileSync(resolution.filePath, 'utf8');
    const compiled = require('@babel/core').transformSync(source, {
      filename: resolution.filePath,
      configFile: false,
      babelrc: false,
      presets: [require.resolve('babel-preset-expo')],
      caller: {
        name: 'metro',
        platform: 'ios',
        engine: 'hermes',
        isDev: false,
        supportsReactCompiler: true,
      },
    }).code;
    assert.match(resolution.filePath, /NativeComponent\.ts$/);
    assert.equal(
      source,
      readFileSync(join(pkg, 'src/EnrichedMarkdownTextInputNativeComponent.ts'), 'utf8'),
    );
    const module = { exports: {} as Record<string, any> };
    const ports: Record<string, any> = {
      'react-native': {},
      'react-native/Libraries/NativeComponent/NativeComponentRegistry': {
        get: (_name: string, read: () => unknown) => read(),
      },
      'react-native/Libraries/NativeComponent/ViewConfigIgnore': {
        ConditionallyIgnoredEventHandlers: (value: unknown) => value,
      },
      'react-native/Libraries/ReactNative/RendererProxy': { dispatchCommand() {} },
      'react-native/Libraries/Components/View/ReactNativeStyleAttributes': { colorAttribute: {} },
    };
    runInNewContext(compiled, {
      module,
      exports: module.exports,
      require: (name: string) => {
        assert.ok(name in ports, name);
        return ports[name];
      },
    });
    const config = module.exports.__INTERNAL_VIEW_CONFIG;
    assert.equal(config.uiViewClassName, 'EnrichedMarkdownTextInput');
    for (const prop of [
      'composerBinding',
      'composerValue',
      'composerMostRecentEventCount',
      'composerRichText',
      'composerSubmitOnReturn',
      'composerCodeLabel',
      'composerQuoteLabel',
    ])
      assert.equal(config.validAttributes[prop], true, prop);
    assert.equal(config.directEventTypes.topChangeMarkdown.registrationName, 'onChangeMarkdown');
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
