#import <AppKit/AppKit.h>
#import "ENRMQuotePresentation.h"
#import "ENRMAsyncRenderCoordinator.h"
static BOOL reducedMotion;
static BOOL UIAccessibilityIsReduceMotionEnabled(void) { return reducedMotion; }
#define CGRectValue rectValue
@interface UIView : NSObject @end
@implementation UIView @end
@interface QuoteOwner : NSObject {
  NSMapTable *_frames;
  CGSize _initialSize;
  NSInteger _generation, _motionSequence;
  CGFloat _progress;
}
- (void)captureFixture:(UIView *)view frame:(CGRect)frame sequence:(NSInteger)sequence;
- (CGFloat)progress;
@end
@implementation QuoteOwner
- (void)captureFixture:(UIView *)view frame:(CGRect)frame sequence:(NSInteger)sequence {
  _frames = [NSMapTable weakToStrongObjectsMapTable];
  [_frames setObject:[NSValue valueWithRect:frame] forKey:view];
  _initialSize = CGSizeMake(200, 100); _generation = 12; _motionSequence = sequence; _progress = 1;
}
- (CGFloat)progress { return _progress; }
#include "quote-owner.inc"
@end
int main() { @autoreleasepool {
 UIView *view = [UIView new]; QuoteOwner *owner = [QuoteOwner new];
 CGRect initial = CGRectMake(0,20,200,100), target = CGRectMake(0,120,200,300);
 [owner captureFixture:view frame:initial sequence:3];
 [owner advanceProgress:0 sequence:2 generation:12];
 NSCAssert(owner.progress == 1, @"late prior interaction cannot rewind a new capture");
 [owner advanceProgress:0 sequence:3 generation:12];
 NSCAssert(CGRectEqualToRect([owner frameForSegment:view target:target width:200], initial), @"no jump at capture");
 [owner advanceProgress:0.5 sequence:3 generation:12];
 CGRect half = [owner frameForSegment:view target:target width:200];
 NSCAssert(half.origin.y == 70 && half.size.height == 200, @"same progress owns segment origin and viewport");
 [owner advanceProgress:0 sequence:4 generation:99];
 [owner advanceProgress:NAN sequence:4 generation:12];
 NSCAssert(owner.progress == 0.5, @"wrong revision and invalid progress are inert");
 [owner captureFixture:view frame:half sequence:4];
 [owner advanceProgress:0 sequence:4 generation:12];
 NSCAssert(CGRectEqualToRect([owner frameForSegment:view target:initial width:200], half), @"interruption starts from actual retained geometry");
 reducedMotion = YES;
 NSCAssert(CGRectEqualToRect([owner frameForSegment:view target:initial width:200], initial), @"system Reduce Motion commits canonical geometry");
 reducedMotion = NO;
 [owner captureFixture:view frame:initial sequence:5];
 NSCAssert(CGRectEqualToRect([owner frameForSegment:view target:target width:400], target), @"rotation invalidates width-specific interpolation");
 ENRMAsyncRenderCoordinator *coordinator = [[ENRMAsyncRenderCoordinator alloc] initWithQueueLabel:"test.quote-motion"];
 NSMutableArray *order = [NSMutableArray new];
 [coordinator scheduleRender:^BOOL { [order addObject:@"old parsed"]; return YES; } apply:^{ [order addObject:@"STALE"]; }];
 [coordinator renderSynchronously:^BOOL { [order addObject:@"current parsed"]; return YES; } apply:^{ [order addObject:@"current applied"]; }];
 NSCAssert(([order isEqual:@[@"old parsed", @"current parsed", @"current applied"]]), @"serial native render reaches actual mounting commit and supersedes stale apply");
 coordinator.blockAsyncRender = YES;
 [coordinator renderSynchronously:^BOOL { abort(); } apply:^{ abort(); }];
 puts("Quote motion owner and committed rendering PASS");
} }
