#import <AppKit/AppKit.h>

// Run the maintained measurement methods with real attributed-string/font
// layout on macOS. This is not a UIKit view or Simulator interaction proof.
@interface Editor : NSView
@property(copy) NSAttributedString *attributedText;
@property(copy) NSDictionary *typingAttributes;
@end
@implementation Editor
@end
@interface Composer : NSObject
@property(copy) NSString *binding;
@end
@implementation Composer
@end
static NSAttributedString *ENRMGetAttributedText(Editor *editor) { return editor.attributedText; }
@interface Input : NSObject {
@public Editor *_textView;
@public Composer *_composer;
@public BOOL _composerConfigurationRejected;
}
- (CGSize)measureSize:(CGFloat)width;
- (NSNumber *)zoenComposerFittingHeight;
@end
@implementation Input
#include "SizingMethods.inc"
@end

int main(void) {
  @autoreleasepool {
    Input *input = [Input new];
    input->_textView = [[Editor alloc] initWithFrame:NSMakeRect(0, 0, 240, 21)];
    input->_composer = [Composer new];
    input->_composer.binding = @"account/device/conversation";
    input->_textView.attributedText = [[NSAttributedString alloc] initWithString:@""];
    double regular = 0;
    for (NSNumber *size in @[@17, @51]) {
      NSDictionary *attributes = @{NSFontAttributeName: [NSFont systemFontOfSize:size.doubleValue]};
      input->_textView.typingAttributes = attributes;
      input->_textView.attributedText = [[NSAttributedString alloc] initWithString:@""];
      double empty = input.zoenComposerFittingHeight.doubleValue;
      NSCAssert(empty > 0, @"empty editor requires a real line");
      if (size.intValue == 17) regular = empty;
      else NSCAssert(empty > regular * 2, @"large typing font controls empty line geometry");
      NSCAssert(input->_textView.attributedText.length == 0, @"measurement cannot insert text");

      input->_textView.attributedText = [[NSAttributedString alloc] initWithString:@"I" attributes:attributes];
      NSCAssert(fabs(input.zoenComposerFittingHeight.doubleValue - empty) < 1, @"empty matches the next typed line");
      input->_textView.attributedText = [[NSAttributedString alloc] initWithString:@"I\n" attributes:attributes];
      double trailing = input.zoenComposerFittingHeight.doubleValue;
      NSCAssert(trailing > empty * 1.8, @"trailing newline retains the next empty line");
      NSCAssert([input->_textView.attributedText.string isEqual:@"I\n"], @"measurement preserves newlines");
      NSCAssert([input->_textView.typingAttributes isEqual:attributes], @"measurement preserves typing style");
    }
    NSDictionary *attrs = @{NSFontAttributeName: [NSFont systemFontOfSize:17]};
    input->_textView.attributedText = [[NSAttributedString alloc]
      initWithString:@"Uma mensagem longa com Unicode 🪷 e várias palavras para quebrar a linha." attributes:attrs];
    double wide = input.zoenComposerFittingHeight.doubleValue;
    input->_textView.frame = NSMakeRect(0, 0, 80, 21);
    NSCAssert(input.zoenComposerFittingHeight.doubleValue > wide, @"actual editor width owns wrapping");
    input->_textView.frame = NSMakeRect(0, 0, 0, 21);
    NSCAssert(input.zoenComposerFittingHeight == nil, @"unlaid-out editor has no fit");
    input->_textView.frame = NSMakeRect(0, 0, 240, 21);
    input->_composerConfigurationRejected = YES;
    NSCAssert(input.zoenComposerFittingHeight == nil, @"rejected owner has no fit");
    input->_composerConfigurationRejected = NO;
    input->_composer.binding = @"";
    NSCAssert(input.zoenComposerFittingHeight == nil, @"unbound editor has no fit");
    puts("Maintained composer sizing PASS");
  }
}
