import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { preparePackage } from './message-markdown-native.fixtures.ts';

test(
  'native link input owns a stable identity and caller-localized label without overriding text/value',
  { skip: process.platform !== 'darwin', timeout: 60000 },
  () => {
    const app = fileURLToPath(new URL('../../', import.meta.url));
    const directory = mkdtempSync(join(tmpdir(), 'zoen-link-accessibility-'));
    try {
      const pkg = preparePackage(app, directory);
      const source = readFileSync(join(pkg, 'ios/input/ENRMInputLinkPrompt.mm'), 'utf8');
      const configure = source.match(
        /\[alert addTextFieldWithConfigurationHandler:\^\(UITextField \*textField\) \{([\s\S]*?)\n  \}\]/,
      )?.[1];
      assert.ok(configure);
      const harness = join(directory, 'link.m');
      writeFileSync(
        harness,
        `
#import <Foundation/Foundation.h>
enum {UIKeyboardTypeURL=3, UITextAutocapitalizationTypeNone=0, UITextAutocorrectionTypeNo=1};
@interface UITextField : NSObject
@property(copy) NSString *placeholder;
@property(copy) NSString *text;
@property(copy) NSString *accessibilityIdentifier;
@property(copy) NSString *accessibilityLabel;
@property(copy) NSString *accessibilityValue;
@property NSInteger keyboardType;
@property NSInteger autocapitalizationType;
@property NSInteger autocorrectionType;
@end
@implementation UITextField
@end
static UITextField *field(NSString *existingURL, NSString *title) {
  UITextField *textField=[UITextField new];
  textField.accessibilityValue=@"native value sentinel";
  void (^configure)(UITextField *)=^(UITextField *textField){ ${configure} };
  configure(textField); return textField;
}
int main(){ @autoreleasepool {
  for(NSString *text in @[@"",@"URL",@"https://example.org/a?q=🦊"]) {
    UITextField *input=field(text,@"Adicionar link");
    NSCAssert([input.accessibilityIdentifier isEqual:@"enriched-markdown-link-url"],@"stable native URL identity");
    NSCAssert([input.accessibilityLabel isEqual:@"Adicionar link"],@"caller owns localization");
    NSCAssert([input.text isEqual:text],@"real URL and empty text stay distinct");
    NSCAssert([input.placeholder isEqual:@"URL"],@"placeholder unchanged");
    NSCAssert([input.accessibilityValue isEqual:@"native value sentinel"],@"never fake native accessibility value");
    NSCAssert(input.keyboardType==UIKeyboardTypeURL && input.autocapitalizationType==UITextAutocapitalizationTypeNone && input.autocorrectionType==UITextAutocorrectionTypeNo,@"native URL entry semantics");
  }
  NSCAssert([field(nil,@"").accessibilityLabel isEqual:@"URL"],@"empty generic title fallback");
  puts("Native link accessibility identity PASS");
}}
`,
      );
      const binary = join(directory, 'link');
      execFileSync(
        'xcrun',
        [
          '--sdk',
          'macosx',
          'clang',
          '-fobjc-arc',
          '-fblocks',
          harness,
          '-framework',
          'Foundation',
          '-o',
          binary,
        ],
        { timeout: 30000 },
      );
      assert.match(
        execFileSync(binary, { encoding: 'utf8' }),
        /Native link accessibility identity PASS/,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
