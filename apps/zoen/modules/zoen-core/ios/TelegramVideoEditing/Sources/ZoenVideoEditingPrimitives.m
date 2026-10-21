#import "ZoenVideoEditingPrimitives.h"

@implementation ZoenVideoHitControl
- (BOOL)pointInside:(CGPoint)point withEvent:(UIEvent *)event {
    return CGRectContainsPoint(UIEdgeInsetsInsetRect(self.bounds, self.hitTestEdgeInsets), point);
}
@end
@implementation ZoenVideoHitButton
- (BOOL)pointInside:(CGPoint)point withEvent:(UIEvent *)event {
    return CGRectContainsPoint(UIEdgeInsetsInsetRect(self.bounds, self.hitTestEdgeInsets), point);
}
@end

@interface ZoenVideoFrameTick : NSObject
@property (nonatomic, weak) ZoenVideoLinearFrameDriver *owner;
- (void)tick:(CADisplayLink *)link;
@end
@interface ZoenVideoLinearFrameDriver ()
@property (nonatomic, weak) UIView *view;
@property (nonatomic) CADisplayLink *link;
@property (nonatomic) ZoenVideoFrameTick *target;
@property (nonatomic) CGRect from;
@property (nonatomic) CGRect to;
@property (nonatomic) CFTimeInterval started;
@property (nonatomic) CFTimeInterval duration;
- (void)tick:(CADisplayLink *)link;
@end
@implementation ZoenVideoFrameTick
- (void)tick:(CADisplayLink *)link { [self.owner tick:link]; }
@end
@implementation ZoenVideoLinearFrameDriver
- (instancetype)initWithView:(UIView *)view {
    self = [super init];
    if (self) { _view = view; }
    return self;
}
- (void)dealloc { [_link invalidate]; }
- (void)startFromFrame:(CGRect)from toFrame:(CGRect)to duration:(NSTimeInterval)duration {
    [self stop];
    self.from = from; self.to = to; self.duration = MAX(0.0, duration);
    self.started = CACurrentMediaTime();
    self.view.frame = from;
    if (!isfinite(duration) || duration <= 0.0) { self.view.frame = to; return; }
    self.target = [ZoenVideoFrameTick new]; self.target.owner = self;
    self.link = [CADisplayLink displayLinkWithTarget:self.target selector:@selector(tick:)];
    [self.link addToRunLoop:NSRunLoop.mainRunLoop forMode:NSRunLoopCommonModes];
}
- (void)tick:(CADisplayLink *)link {
    UIView *view = self.view;
    if (!view) { [self stop]; return; }
    double t = fmin(1.0, fmax(0.0, (link.timestamp - self.started) / self.duration));
    CGFloat (^value)(CGFloat, CGFloat) = ^CGFloat(CGFloat a, CGFloat b) { return round((a + (b - a) * t) * 2.0) / 2.0; };
    view.frame = CGRectMake(value(self.from.origin.x,self.to.origin.x), value(self.from.origin.y,self.to.origin.y), value(self.from.size.width,self.to.size.width), value(self.from.size.height,self.to.size.height));
    if (t >= 1.0) { view.frame = self.to; [self stop]; }
}
- (void)stop { [self.link invalidate]; self.link = nil; self.target = nil; }
@end
