#import <AppKit/AppKit.h>
#define CGRect NSRect
#define CGRectMake NSMakeRect
#define UIUserInterfaceLayoutDirectionRightToLeft 1
#define UIControlStateNormal 0
#define CGAffineTransformMakeRotation(x) (x)
static CGFloat ENRMQuoteGeometryProgress(CGFloat current, CGFloat initial, CGFloat target) { return fabs(target-initial)<0.5 ? 1 : (current-initial)/(target-initial); }
static CGFloat RCTScreenScale(void) { return 1; }
static CGFloat ENRMQuotePreviewHeight(id text, CGFloat width, CGFloat scale) { return text && width < 300 ? 60 : 0; }
@interface UIView : NSObject
@property NSRect frame, bounds;
@property NSPoint center;
@property CGFloat transform;
@property BOOL accessibilityElementsHidden, clipsToBounds, hidden;
@property NSInteger effectiveUserInterfaceLayoutDirection;
@property NSArray<UIView *> *subviews;
- (void)layoutSubviews;
- (void)bringSubviewToFront:(id)view;
@end
@implementation UIView
- (void)layoutSubviews { for (UIView *child in self.subviews) child.frame = NSMakeRect(0,0,200,180); }
- (void)bringSubviewToFront:(id)view {}
@end
@interface UIButton : UIView
@property id tintColor;
@property NSString *accessibilityLabel, *accessibilityValue;
- (void)setImage:(id)image forState:(NSInteger)state;
@end
@implementation UIButton
- (void)setImage:(id)image forState:(NSInteger)state {}
@end
@interface UIImage : NSObject
+ (id)systemImageNamed:(NSString *)name;
@end
@implementation UIImage
+ (id)systemImageNamed:(NSString *)name { return nil; }
@end
@interface StyleConfig : NSObject
@property id blockquoteBorderColor;
@end
@implementation StyleConfig @end
@interface Quote : UIView
@property NSEdgeInsets contentInsets;
@property id quotePreviewText;
@property BOOL quoteExpanded;
@property UIButton *quoteToggleButton;
@property StyleConfig *config;
@property NSString *quoteCollapseLabel, *quoteExpandLabel, *quoteExpandedLabel, *quoteCollapsedLabel;
@end
@implementation Quote
- (CGFloat)computeContentHeightForWidth:(CGFloat)width { return 180; }
#include "quote-layout.inc"
@end
int main() { @autoreleasepool {
 Quote *quote = [Quote new]; UIView *text = [UIView new];
 quote.quoteToggleButton = [UIButton new]; quote.subviews = @[text, quote.quoteToggleButton];
 quote.quotePreviewText = @"original attributed text"; quote.bounds = NSMakeRect(0,0,200,104);
 [quote layoutSubviews];
 NSCAssert(text.accessibilityElementsHidden && text.clipsToBounds && text.frame.size.height == 60, @"long collapsed");
 quote.bounds = NSMakeRect(0,0,400,180); [quote layoutSubviews];
 NSCAssert(!text.accessibilityElementsHidden && !text.clipsToBounds && text.frame.size.height == 180 && quote.quoteToggleButton.hidden, @"retained reflow short");
 quote.bounds = NSMakeRect(0,0,200,104); [quote layoutSubviews];
 NSCAssert(text.accessibilityElementsHidden && !quote.quoteToggleButton.hidden, @"long again");
 quote.quotePreviewText = nil; [quote layoutSubviews];
 NSCAssert(!text.accessibilityElementsHidden && !text.clipsToBounds && text.frame.size.height == 180 && quote.quoteToggleButton.hidden, @"selection full");
 quote.quotePreviewText = @"original attributed text"; quote.quoteExpanded = YES; quote.bounds = NSMakeRect(0,0,200,224); [quote layoutSubviews];
 NSCAssert(!text.accessibilityElementsHidden && !text.clipsToBounds && text.frame.size.height == 180, @"expanded viewport");
 puts("Retained quote reflow PASS");
} }
