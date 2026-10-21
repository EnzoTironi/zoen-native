#import <UIKit/UIKit.h>

NS_ASSUME_NONNULL_BEGIN

/// Scoped equivalents of upstream hitTestEdgeInsets; no UIKit class swizzling.
@interface ZoenVideoHitControl : UIControl
@property (nonatomic) UIEdgeInsets hitTestEdgeInsets;
@end
@interface ZoenVideoHitButton : UIButton
@property (nonatomic) UIEdgeInsets hitTestEdgeInsets;
@end

/// The scrubber's original POPBasicAnimation has a linear frame property,
/// clampBoth and roundingFactor=0.5. This driver preserves those kinematics.
@interface ZoenVideoLinearFrameDriver : NSObject
- (instancetype)initWithView:(UIView *)view;
- (void)startFromFrame:(CGRect)from toFrame:(CGRect)to duration:(NSTimeInterval)duration;
- (void)stop;
@end

static inline UIColor *ZoenVideoColorRGBA(uint32_t rgb, CGFloat alpha) {
    return [UIColor colorWithRed:((rgb >> 16) & 255) / 255.0 green:((rgb >> 8) & 255) / 255.0 blue:(rgb & 255) / 255.0 alpha:alpha];
}
static inline UIColor *ZoenVideoColorRGB(uint32_t rgb) { return ZoenVideoColorRGBA(rgb, 1.0); }
static inline UIImage *ZoenVideoTintedImage(UIImage *image, UIColor *color) {
    return [image imageWithTintColor:color renderingMode:UIImageRenderingModeAlwaysOriginal];
}
static inline CGFloat ZoenVideoSeparatorHeight(UIView *view) {
    CGFloat scale = view.traitCollection.displayScale;
    if (fabs(scale - 2.0) < FLT_EPSILON) return 0.5;
    if (fabs(scale - 3.0) < FLT_EPSILON) return 0.33;
    return 1.0;
}
static inline CGFloat ZoenVideoRotationForOrientation(UIImageOrientation orientation) {
    switch (orientation) {
        case UIImageOrientationDown: return -M_PI;
        case UIImageOrientationLeft: return -M_PI_2;
        case UIImageOrientationRight: return M_PI_2;
        default: return 0.0;
    }
}

NS_ASSUME_NONNULL_END
