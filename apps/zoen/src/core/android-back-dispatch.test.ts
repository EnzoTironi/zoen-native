import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import type { ExpoConfig } from 'expo/config';

const require = createRequire(import.meta.url);
const plugin = require('../../plugins/with-android-back-dispatch.cjs');
const { removeGeneratedContents } = require('@expo/config-plugins/build/utils/generateCode');
const tags = [
  'android-back-dispatch-import',
  'android-back-dispatch-lifecycle',
  'android-back-dispatch-register',
  'android-back-dispatch-default',
];
const originalFallback = `      if (Build.VERSION.SDK_INT <= Build.VERSION_CODES.R) {
          if (!moveTaskToBack(false)) {
              super.invokeDefaultOnBackPressed()
          }
          return
      }
      super.invokeDefaultOnBackPressed()`;
// The SDK 58 Kotlin template's actual extension points, including its existing root fallback.
const template = `package xyz.tironi.zoen
import android.os.Build
import android.os.Bundle
import com.facebook.react.ReactActivity

class MainActivity : ReactActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    SplashScreenManager.registerOnActivity(this)
    super.onCreate(null)
  }

  override fun invokeDefaultOnBackPressed() {
${originalFallback}
  }
}
`;

async function generate(contents = template, language = 'kt') {
  const config = plugin({
    name: 'Zoen',
    slug: 'zoen',
    android: { predictiveBackGestureEnabled: true },
  });
  const result = await config.mods.android.mainActivity({
    ...config,
    modRequest: {},
    modResults: { language, contents },
  });
  return result as ExpoConfig & { modResults: { contents: string } };
}

test('the real MainActivity mod adds one API 33–35 bridge and preserves the original native fallback', async () => {
  const generated = await generate();
  const source = generated.modResults.contents;
  assert.equal(generated.android?.predictiveBackGestureEnabled, true);
  assert.ok(source.includes(originalFallback));
  assert.match(
    source,
    /super\.onCreate\(null\)[\s\S]+onBackPressedDispatcher\.addCallback\(this, reactBackDispatch\)/,
  );
  assert.match(
    source,
    /try \{\s+super@MainActivity\.onBackPressed\(\)\s+\} finally \{\s+isEnabled = !nativeBackFallback/,
  );
  assert.match(
    source,
    /override fun onResume\(\) \{\s+super\.onResume\(\)\s+if \(Build\.VERSION\.SDK_INT in 33\.\.35\) \{\s+nativeBackFallback = false\s+reactBackDispatch\.isEnabled = true/,
  );
  assert.match(
    source,
    /override fun invokeDefaultOnBackPressed\(\) \{[\s\S]+nativeBackFallback = true\s+reactBackDispatch\.isEnabled = false[\s\S]+if \(Build\.VERSION\.SDK_INT <= Build\.VERSION_CODES\.R\)/,
  );
  assert.equal(source.match(/Build\.VERSION\.SDK_INT in 33\.\.35/g)?.length, 3);
  let restored = source;
  for (const tag of tags) restored = removeGeneratedContents(restored, tag) ?? restored;
  assert.equal(restored, template);
});

test('repeated CNG regenerates the same bridge and replaces only its own generated blocks', async () => {
  const once = (await generate()).modResults.contents;
  const twice = (await generate(once)).modResults.contents;
  assert.equal(twice, once);
  const stale = once.replace('isEnabled = !nativeBackFallback', 'isEnabled = false');
  assert.equal((await generate(stale)).modResults.contents, once);
  for (const tag of tags) assert.equal(once.split(`@generated begin ${tag} -`).length, 2);
});

test('unsupported or ambiguous templates fail before modifying the MainActivity', async () => {
  await assert.rejects(generate(template, 'java'), /Kotlin Expo MainActivity/);
  await assert.rejects(
    generate(template.replace('super.onCreate(null)', 'super.onCreate(savedInstanceState)')),
    /exactly one MainActivity anchor/,
  );
  await assert.rejects(
    generate(`${template}\nclass MainActivity : ReactActivity() {`),
    /exactly one MainActivity anchor/,
  );
  await assert.rejects(
    generate(
      template.replace(
        '  override fun onCreate',
        '  override fun onResume() {}\n  override fun onCreate',
      ),
    ),
    /existing onResume override/,
  );
});
