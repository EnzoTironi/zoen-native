import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { preparePackage } from './message-markdown-native.fixtures.ts';

test(
  'Fabric accessibility targets the editor once, preserving native value and selection',
  { skip: process.platform !== 'darwin', timeout: 60_000 },
  async (t) => {
    const app = fileURLToPath(new URL('../../', import.meta.url));
    const directory = mkdtempSync(join(tmpdir(), 'zoen-composer-accessibility-'));
    try {
      const pkg = preparePackage(app, directory);
      const source = readFileSync(join(pkg, 'ios/input/EnrichedMarkdownTextInput.mm'), 'utf8');
      const methods = source.match(/#pragma mark - Accessibility\n([\s\S]*?)#endif/)?.[1];
      assert.ok(methods);
      assert.match(source, /_textView\.isAccessibilityElement = YES;/);
      assert.match(source, /_placeholderLabel\.accessibilityElementsHidden = YES;/);
      assert.doesNotMatch(
        methods,
        /accessibilityValue|accessibilityActivate|accessibilityElements/,
      );
      // Compile the actual production overrides with UIKit, then exercise their
      // dispatch contract on macOS without launching an application or Simulator.
      const harness = join(directory, 'accessibility.m');
      writeFileSync(
        harness,
        `
#if defined(ENRM_UIKIT_CHECK)
#import <UIKit/UIKit.h>
#define Editor UITextView
#define Container UIView
#else
#import <Foundation/Foundation.h>
@interface Editor : NSObject
@property BOOL isAccessibilityElement;
@property BOOL editable;
@property BOOL selectable;
@property NSRange selectedRange;
@property(copy) NSString *accessibilityLabel;
@property(copy) NSString *accessibilityIdentifier;
@property(copy) NSString *accessibilityValue;
@end
@implementation Editor
@end
@interface Container : NSObject
@property BOOL isAccessibilityElement;
@end
@implementation Container
@end
#endif
@interface Input : Container {
@public Editor *_textView;
}
- (NSObject *)accessibilityElement;
@end
@implementation Input
${methods}
@end
int main(void) {
  @autoreleasepool {
    Input *host = [Input new];
    Editor *editor = [Editor new];
    host->_textView = editor;
    editor.isAccessibilityElement = YES;
    editor.editable = YES;
    editor.selectable = YES;
    editor.selectedRange = NSMakeRange(3, 2);
    editor.accessibilityValue = @"Native text 🪷";
    // Fabric uses this target for accessible, label, identifier, hint and value.
    Editor *target = (Editor *)host.accessibilityElement;
    NSCAssert(target == editor, @"Fabric must address the actual text input");
    target.accessibilityLabel = @"Mensagem";
    target.accessibilityIdentifier = @"message-input";
    NSCAssert([editor.accessibilityLabel isEqual:@"Mensagem"], @"label forwarding");
    NSCAssert([editor.accessibilityIdentifier isEqual:@"message-input"], @"identifier forwarding");
    NSCAssert([target.accessibilityValue isEqual:@"Native text 🪷"], @"native value");
    NSCAssert(NSEqualRanges(editor.selectedRange, NSMakeRange(3, 2)), @"native selection unchanged");
    NSCAssert(editor.editable && editor.selectable, @"native editing unchanged");
    host.isAccessibilityElement = YES;
    NSCAssert(!host.isAccessibilityElement && target.isAccessibilityElement, @"one accessible editor, no host duplicate");
    target.isAccessibilityElement = NO;
    NSCAssert(!editor.isAccessibilityElement, @"explicit accessible=false reaches editor");
    target.isAccessibilityElement = YES;
    target.accessibilityValue = @"Updated override";
    NSCAssert([editor.accessibilityValue isEqual:@"Updated override"], @"explicit value forwarding");
    target.accessibilityLabel = nil;
    target.accessibilityIdentifier = nil;
    NSCAssert(editor.accessibilityLabel == nil && editor.accessibilityIdentifier == nil, @"metadata reset");
    puts("Composer accessibility delegation PASS");
  }
}
`,
      );
      const sdk = spawnSync('xcrun', ['--sdk', 'iphonesimulator', '--show-sdk-path']);
      await t.test(
        'production accessibility overrides compile against UIKit',
        { skip: sdk.status !== 0 ? 'iOS SDK requires Xcode DEVELOPER_DIR' : false },
        () => {
          execFileSync(
            'xcrun',
            [
              '--sdk',
              'iphonesimulator',
              'clang',
              '-target',
              'arm64-apple-ios16.0-simulator',
              '-fobjc-arc',
              '-DENRM_UIKIT_CHECK',
              '-fsyntax-only',
              '-Werror',
              harness,
            ],
            { timeout: 30_000 },
          );
        },
      );
      const binary = join(directory, 'accessibility');
      execFileSync(
        'xcrun',
        [
          '--sdk',
          'macosx',
          'clang',
          '-fobjc-arc',
          '-Werror',
          harness,
          '-framework',
          'Foundation',
          '-o',
          binary,
        ],
        { timeout: 30_000 },
      );
      assert.match(
        execFileSync(binary, { encoding: 'utf8' }),
        /Composer accessibility delegation PASS/,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
