#import <Foundation/Foundation.h>
#import <CoreGraphics/CoreGraphics.h>
#import <assert.h>

// Execute the actual patched renderer method and existing attribute classifier.
// The point-to-index adapter below is deterministic; this is not a TextKit/gesture runtime proof.
@interface NSValue (PointBridge)
- (CGPoint)CGPointValue;
@end
@implementation NSValue (PointBridge)
- (CGPoint)CGPointValue { CGPoint p; [self getValue:&p size:sizeof(p)]; return p; }
@end
@interface FixtureText : NSObject
@property BOOL editable, selectable, hidden;
@property CGRect bounds;
@property NSAttributedString *attributedText;
@end
@implementation FixtureText
@end
#define ENRMPlatformTextView FixtureText
static NSString *SpoilerAttributeName = @"spoiler";
static NSAttributedString *ENRMGetAttributedText(FixtureText *text) { return text.attributedText; }
static NSUInteger ENRMCharacterIndexAtPoint(FixtureText *text, CGPoint point) {
  return point.x >= 0 && point.x < text.attributedText.length ? (NSUInteger)point.x : NSNotFound;
}
#import "classifier.inc"
@interface Renderer : NSObject {
@public FixtureText *_textView;
}
@property CGRect bounds;
- (CGPoint)convertPoint:(CGPoint)point toView:(FixtureText *)text;
@end
@implementation Renderer
- (CGPoint)convertPoint:(CGPoint)point toView:(FixtureText *)text { return point; }
#import "admission.inc"
@end
int main(void) {
  @autoreleasepool {
    Renderer *renderer = [Renderer new]; renderer.bounds = CGRectMake(0, 0, 100, 30);
    FixtureText *text = [FixtureText new]; renderer->_textView = text;
    text.bounds = renderer.bounds;
    NSMutableAttributedString *body = [[NSMutableAttributedString alloc] initWithString:@"café 🦊 verde fim"];
    text.attributedText = body;
    CGPoint p = CGPointMake(1, 4);
    NSValue *point = [NSValue value:&p withObjCType:@encode(CGPoint)];
    assert([renderer zoenMessageContextBodyAtPoint:point].boolValue);
    // Existing renderer-owned semantic attributes must all retain native input.
    for (NSString *key in @[@"linkURL", @"TaskItem", @"spoiler", @"imageURL", @"codeBlockText"]) {
      [body addAttribute:key value:@YES range:NSMakeRange(1, 1)];
      assert(![renderer zoenMessageContextBodyAtPoint:point].boolValue);
      [body removeAttribute:key range:NSMakeRange(1, 1)];
    }
    text.selectable = YES; assert(![renderer zoenMessageContextBodyAtPoint:point].boolValue);
    text.selectable = NO; text.editable = YES; assert(![renderer zoenMessageContextBodyAtPoint:point].boolValue);
    text.editable = NO; text.hidden = YES; assert(![renderer zoenMessageContextBodyAtPoint:point].boolValue);
    text.hidden = NO;
    p.x = 101; assert(![renderer zoenMessageContextBodyAtPoint:[NSValue value:&p withObjCType:@encode(CGPoint)]].boolValue);
    assert(![renderer zoenMessageContextBodyAtPoint:(NSValue *)@"invalid"].boolValue);
    assert(![renderer zoenMessageContextBodyAtPoint:[NSValue valueWithRange:NSMakeRange(0, 1)]].boolValue);
    puts("Actual renderer admission + interactive attributes PASS");
  }
}
