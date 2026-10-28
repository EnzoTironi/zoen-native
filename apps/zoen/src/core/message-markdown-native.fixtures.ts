import { cpSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';

/** Reconstruct the maintained native Markdown package privately from installed dependencies. */
export function preparePackage(app: string, directory: string) {
  const pkg = join(directory, 'node_modules/react-native-enriched-markdown');
  const installed = join(app, 'node_modules/react-native-enriched-markdown');
  const patches = [
    '007+quote-source-map',
    '008+message-context-admission',
    '009+github-quote-pipeline',
    '010+composer-parser',
    '011+composer-input-contract',
    '012+composer-formatting',
    '013+composer-input-codegen',
    '014+composer-input-owner',
    '015+composer-input-lifecycle',
    '016+composer-input-receipt',
    '017+composer-plain-paste-sizing',
    '018+composer-range-normalization',
    '019+composer-inline-variants',
    '020+composer-public-codegen-spec',
    '021+composer-format-availability',
    '022+composer-native-accessibility',
    '023+composer-escaped-inline-import',
    '024+composer-link-accessibility',
    '025+quote-collapse-public-contract',
    '026+quote-collapse-text-layout',
    '027+quote-collapse-native-host',
    '030+semantic-text-projection',
    '031+semantic-projection-android-codegen',
    '034+composer-canonical-sizing',
  ].map((stage) => join(app, `patches/react-native-enriched-markdown+1.1.1+${stage}.patch`));
  const paths = new Set(
    patches.flatMap((patch) =>
      [
        ...readFileSync(patch, 'utf8').matchAll(
          /^diff --git a\/node_modules\/react-native-enriched-markdown\/(.+) b\//gm,
        ),
      ].map((match) => match[1]!),
    ),
  );
  for (const path of paths) {
    if (!existsSync(join(installed, path))) continue;
    mkdirSync(join(pkg, path, '..'), { recursive: true });
    cpSync(join(installed, path), join(pkg, path));
  }
  for (const path of ['cpp/parser', 'cpp/md4c', 'src', 'ios/parser', 'ios/segments', 'ios/utils'])
    cpSync(join(installed, path), join(pkg, path), { recursive: true });
  const markers = [
    () => existsSync(join(pkg, 'ios/utils/ENRMQuoteRanges.m')),
    () =>
      existsSync(join(pkg, 'ios/utils/ENRMMessageContextAdmission.h')) ||
      readFileSync(join(pkg, 'ios/EnrichedMarkdownText.mm'), 'utf8').includes(
        'zoenMessageContextBodyAtPoint:',
      ),
    () => existsSync(join(pkg, 'ios/utils/ENRMMessageContextAdmission.h')),
    () =>
      readFileSync(join(pkg, 'ios/input/ENRMInputParser.h'), 'utf8').includes(
        'parseComposerMarkdown:',
      ),
    () =>
      readFileSync(join(pkg, 'src/EnrichedMarkdownTextInputNativeComponent.ts'), 'utf8').includes(
        'composerBinding',
      ),
    () => existsSync(join(pkg, 'ios/input/ENRMMarkdownSerializer+Composer.mm')),
    () =>
      readFileSync(
        join(pkg, 'ios/generated/ReactCodegen/EnrichedMarkdownTextSpec/Props.h'),
        'utf8',
      ).includes('composerBinding'),
    () =>
      readFileSync(join(pkg, 'ios/input/EnrichedMarkdownTextInput.mm'), 'utf8').includes(
        'composerMutationWillBegin',
      ),
    () =>
      readFileSync(join(pkg, 'ios/input/ENRMClipboardCoordinator.h'), 'utf8').includes(
        'composerMode',
      ),
    () => existsSync(join(pkg, 'ios/input/ENRMComposerInputState.m')),
    () =>
      readFileSync(join(pkg, 'ios/input/EnrichedMarkdownTextInput.mm'), 'utf8').includes(
        'zoenComposerBaseFontSize',
      ),
    () => existsSync(join(pkg, 'ios/input/ENRMComposerRanges.mm')),
    () => existsSync(join(pkg, 'ios/input/ENRMComposerVariants.mm')),
    () =>
      readFileSync(
        join(pkg, 'lib/module/EnrichedMarkdownTextInputNativeComponent.ts'),
        'utf8',
      ).includes('composerBinding') &&
      !existsSync(join(pkg, 'lib/module/EnrichedMarkdownTextInputNativeComponent.js')),
    () => existsSync(join(pkg, 'ios/input/ENRMComposerAvailability.mm')),
    () =>
      readFileSync(join(pkg, 'ios/input/EnrichedMarkdownTextInput.mm'), 'utf8').includes(
        '- (NSObject *)accessibilityElement',
      ),
    () =>
      readFileSync(join(pkg, 'ios/input/ENRMInputParser.mm'), 'utf8').includes(
        'size_t cursor=textStart - (escaped ? 1 : 0);',
      ),
    () =>
      readFileSync(join(pkg, 'ios/input/ENRMInputLinkPrompt.mm'), 'utf8').includes(
        'enriched-markdown-link-url',
      ),
    () =>
      readFileSync(join(pkg, 'lib/module/EnrichedMarkdownNativeComponent.ts'), 'utf8').includes(
        'quoteCollapse?:',
      ),
    () => existsSync(join(pkg, 'ios/utils/ENRMQuotePresentation.mm')),
    () =>
      readFileSync(join(pkg, 'ios/EnrichedMarkdown.mm'), 'utf8').includes('view.onQuoteToggle ='),
    () => existsSync(join(pkg, 'ios/utils/ENRMPlainTextProjection.mm')),
    () => existsSync(join(pkg, 'android/generated/jni/EnrichedMarkdownTextSpec-generated.cpp')),
    () =>
      readFileSync(join(pkg, 'ios/input/EnrichedMarkdownTextInput.mm'), 'utf8').includes(
        '- (NSNumber *)zoenComposerFittingHeight',
      ),
  ];
  for (const [index, patch] of patches.entries())
    if (!markers[index]!())
      execFileSync('patch', ['--batch', '--forward', '-E', '-p1', '-i', patch], { cwd: directory });
  return pkg;
}
