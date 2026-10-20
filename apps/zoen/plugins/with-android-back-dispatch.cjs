const { createRunOncePlugin, withMainActivity } = require('expo/config-plugins');
const {
  mergeContents,
  removeGeneratedContents,
} = require('@expo/config-plugins/build/utils/generateCode');

const blocks = [
  {
    tag: 'android-back-dispatch-import',
    anchor: /^import android\.os\.Bundle$/,
    comment: '//',
    source: 'import androidx.activity.OnBackPressedCallback',
  },
  {
    tag: 'android-back-dispatch-lifecycle',
    anchor: /^class MainActivity : ReactActivity\(\) \{$/,
    comment: '  //',
    source: `  // RN 0.88 installs its dispatcher bridge only on API 36+. Keep predictive Back on 33–35.
  private var nativeBackFallback = false
  private val reactBackDispatch = object : OnBackPressedCallback(false) {
    override fun handleOnBackPressed() {
      isEnabled = false
      try {
        super@MainActivity.onBackPressed()
      } finally {
        isEnabled = !nativeBackFallback
      }
    }
  }

  override fun onResume() {
    super.onResume()
    if (Build.VERSION.SDK_INT in 33..35) {
      nativeBackFallback = false
      reactBackDispatch.isEnabled = true
    }
  }`,
  },
  {
    tag: 'android-back-dispatch-register',
    anchor: /^    super\.onCreate\(null\)$/,
    comment: '    //',
    source: `    if (Build.VERSION.SDK_INT in 33..35) {
      reactBackDispatch.isEnabled = true
      onBackPressedDispatcher.addCallback(this, reactBackDispatch)
    }`,
  },
  {
    tag: 'android-back-dispatch-default',
    anchor: /^  override fun invokeDefaultOnBackPressed\(\) \{$/,
    comment: '    //',
    source: `    if (Build.VERSION.SDK_INT in 33..35) {
      // The original fallback re-enters the dispatcher. Leave this bridge disabled until resume.
      nativeBackFallback = true
      reactBackDispatch.isEnabled = false
    }`,
  },
];

function withAndroidBackDispatch(config) {
  return withMainActivity(config, (config) => {
    if (config.modResults.language !== 'kt') {
      throw new Error('Android Back dispatch requires the Kotlin Expo MainActivity template.');
    }
    let source = config.modResults.contents;
    for (const { tag } of blocks) source = removeGeneratedContents(source, tag) ?? source;
    if (/override\s+fun\s+onResume\s*\(/.test(source)) {
      throw new Error('Android Back dispatch requires review of the existing onResume override.');
    }
    for (const { tag, anchor, comment, source: newSrc } of blocks) {
      if (source.split('\n').filter((line) => anchor.test(line)).length !== 1) {
        throw new Error(`Android Back dispatch requires exactly one MainActivity anchor: ${tag}.`);
      }
      source = mergeContents({ src: source, newSrc, tag, anchor, offset: 1, comment }).contents;
    }
    config.modResults.contents = source;
    return config;
  });
}

module.exports = createRunOncePlugin(
  withAndroidBackDispatch,
  'zoen-android-back-dispatch',
  '1.0.0',
);
