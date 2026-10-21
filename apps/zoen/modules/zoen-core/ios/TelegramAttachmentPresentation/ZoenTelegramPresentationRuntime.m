// Adapted from TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// GPL-2.0-or-later; GPL-3.0 selected. See NOTICE.md and UPSTREAM_SOURCES.json.
#import "ZoenTelegramPresentationRuntime.h"
#import <objc/runtime.h>
#if TARGET_IPHONE_SIMULATOR
UIKIT_EXTERN float UIAnimationDragCoefficient();
#endif

double animationDurationFactorImpl() {
#if TARGET_IPHONE_SIMULATOR
    return (double)UIAnimationDragCoefficient();
#endif   
    return 1.0f;
}

CABasicAnimation * _Nonnull makeSpringAnimationImpl(NSString * _Nonnull keyPath, double duration) {
    if (@available(iOS 26.0, *)) {
        return make26SpringAnimationImpl(keyPath, duration);
    }
    
    CASpringAnimation *springAnimation = [CASpringAnimation animationWithKeyPath:keyPath];
    springAnimation.mass = 3.0f;
    springAnimation.stiffness = 1000.0f;
    springAnimation.damping = 500.0f;
    springAnimation.duration = 0.5;
    springAnimation.timingFunction = [CAMediaTimingFunction functionWithName:kCAMediaTimingFunctionLinear];
    
    return springAnimation;
}

CABasicAnimation * _Nonnull make26SpringAnimationImpl(NSString * _Nonnull keyPath, double duration) {
    CASpringAnimation *springAnimation = [CASpringAnimation animationWithKeyPath:keyPath];
    springAnimation.mass = 1.0f;
    springAnimation.stiffness = 555.027;
    springAnimation.damping = 47.118;
    springAnimation.duration = duration;
    springAnimation.timingFunction = [CAMediaTimingFunction functionWithName:kCAMediaTimingFunctionLinear];
    if (@available(iOS 17.0, *)) {
        springAnimation.allowsOverdamping = false;
    }
    if (@available(iOS 15.0, *)) {
        [springAnimation setValue:@(1048619) forKey:@"highFrameRateReason"];
        springAnimation.preferredFrameRateRange = CAFrameRateRangeMake(80.0, 120.0, 120.0);
    }
    
    return springAnimation;
}

CASpringAnimation * _Nonnull makeSpringBounceAnimationImpl(NSString * _Nonnull keyPath, CGFloat initialVelocity, CGFloat damping) {
    CASpringAnimation *springAnimation = [CASpringAnimation animationWithKeyPath:keyPath];
    springAnimation.mass = 5.0f;
    springAnimation.stiffness = 900.0f;
    springAnimation.damping = damping;
    static bool canSetInitialVelocity = true;
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        canSetInitialVelocity = [springAnimation respondsToSelector:@selector(setInitialVelocity:)];
    });
    if (canSetInitialVelocity) {
        springAnimation.initialVelocity = initialVelocity;
        springAnimation.duration = springAnimation.settlingDuration;
    } else {
        springAnimation.duration = 0.1;
    }
    springAnimation.timingFunction = [CAMediaTimingFunction functionWithName:kCAMediaTimingFunctionLinear];
    return springAnimation;
}

static void setBoolField(NSObject *object, NSString *name, BOOL value) {
    SEL selector = NSSelectorFromString(name);
    NSMethodSignature *signature = [[object class] instanceMethodSignatureForSelector:selector];
    if (signature == nil) {
        return;
    }
    
    NSInvocation *inv = [NSInvocation invocationWithMethodSignature:signature];
    [inv setSelector:selector];
    [inv setArgument:&value atIndex:2];
    [inv setTarget:object];
    [inv invoke];
}
static void setLongLongField(NSObject *object, NSString *name, long long value) {
    SEL selector = NSSelectorFromString(name);
    NSMethodSignature *signature = [[object class] instanceMethodSignatureForSelector:selector];
    if (signature == nil) {
        return;
    }
    
    NSInvocation *inv = [NSInvocation invocationWithMethodSignature:signature];
    [inv setSelector:selector];
    [inv setArgument:&value atIndex:2];
    [inv setTarget:object];
    [inv invoke];
}
@protocol GraphicsFilterProtocol <NSObject>
    
- (NSObject * _Nullable)filterWithName:(NSString * _Nonnull)name;

@end

NSObject * _Nullable makeBlurFilter() {
    return [(id<GraphicsFilterProtocol>)NSClassFromString(@"CAFilter") filterWithName:@"gaussianBlur"];
}

NSObject * _Nullable makeLuminanceToAlphaFilter() {
    return [(id<GraphicsFilterProtocol>)NSClassFromString(@"CAFilter") filterWithName:@"luminanceToAlpha"];
}
NSObject * _Nullable makeColorMatrixFilter() {
    return [(id<GraphicsFilterProtocol>)NSClassFromString(@"CAFilter") filterWithName:@"colorMatrix"];
}
void setMonochromaticEffectImpl(UIView * _Nonnull view, bool isEnabled) {
    if (@available(iOS 26.0, *)) {
        static NSString *key1 = nil;
        static NSString *key2 = nil;
        static NSString *key3 = nil;
        static dispatch_once_t onceToken;
        dispatch_once(&onceToken, ^{
            key1 = [[@"_" stringByAppendingString:@"setAllows"] stringByAppendingString:@"MonochromaticTreatment:"];
            key2 = [[@"_" stringByAppendingString:@"setEnable"] stringByAppendingString:@"MonochromaticTreatment:"];
            key3 = [[@"_" stringByAppendingString:@"set"] stringByAppendingString:@"MonochromaticTreatment:"];
        });
        
        if (isEnabled) {
            setBoolField(view, key1, true);
            setBoolField(view, key2, true);
            setLongLongField(view, key3, 2);
        } else {
            setBoolField(view, key1, false);
            setBoolField(view, key2, false);
            setLongLongField(view, key3, 0);
        }
    }
}
@implementation CALayerSpringParametersOverrideParameters

- (instancetype _Nonnull)init {
    self = [super init];
    if (self != nil) {
    }
    return self;
}

@end

@implementation CALayerSpringParametersOverrideParametersSpring

- (instancetype _Nonnull)initWithStiffness:(CGFloat)stiffness damping:(CGFloat)damping duration:(double)duration {
    self = [super init];
    if (self != nil) {
        _stiffness = stiffness;
        _damping = damping;
        _duration = duration;
    }
    return self;
}

@end

@implementation CALayerSpringParametersOverrideParametersCustomCurve

- (instancetype _Nonnull)initWithCp1:(CGPoint)cp1 cp2:(CGPoint)cp2 {
    self = [super init];
    if (self != nil) {
        _cp1 = cp1;
        _cp2 = cp2;
    }
    return self;
}

@end


@implementation CALayerSpringParametersOverride

- (instancetype _Nonnull)initWithParameters:(CALayerSpringParametersOverrideParameters * _Nullable)parameters {
    self = [super init];
    if (self != nil) {
        _parameters = parameters;
    }
    return self;
}

@end

static NSMutableArray<CALayerSpringParametersOverride *> *currentSpringParametersOverrideStack() {
    static NSMutableArray *array = nil;
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        array = [[NSMutableArray alloc] init];
    });
    return array;
}

@implementation CALayer (TelegramAddAnimation)

+ (void)pushSpringParametersOverride:(CALayerSpringParametersOverride * _Nonnull)springParametersOverride {
    if (springParametersOverride) {
        [currentSpringParametersOverrideStack() addObject:springParametersOverride];
    }
}

+ (void)popSpringParametersOverride {
    if (currentSpringParametersOverrideStack().count != 0) {
        [currentSpringParametersOverrideStack() removeLastObject];
    }
}

- (void)_65087dc8_addAnimation:(CAAnimation *)anim forKey:(NSString *)key {
    CAAnimation *updatedAnimation = anim;
    if (currentSpringParametersOverrideStack().count != 0 && [anim isKindOfClass:[CASpringAnimation class]]) {
        CALayerSpringParametersOverride *overrideData = [currentSpringParametersOverrideStack() lastObject];
        if (overrideData) {
            if ([overrideData.parameters isKindOfClass:[CALayerSpringParametersOverrideParametersSpring class]]) {
                CALayerSpringParametersOverrideParametersSpring *parameters = (CALayerSpringParametersOverrideParametersSpring *)overrideData.parameters;
                CABasicAnimation *sourceAnimation = (CABasicAnimation *)anim;
                
                CASpringAnimation *animation = makeSpringBounceAnimationImpl(sourceAnimation.keyPath, 0.0, parameters.damping);
                
                animation.stiffness = parameters.stiffness;
                animation.fromValue = sourceAnimation.fromValue;
                animation.toValue = sourceAnimation.toValue;
                animation.byValue = sourceAnimation.byValue;
                animation.additive = sourceAnimation.additive;
                animation.removedOnCompletion = sourceAnimation.isRemovedOnCompletion;
                animation.fillMode = sourceAnimation.fillMode;
                animation.beginTime = sourceAnimation.beginTime;
                animation.timeOffset = sourceAnimation.timeOffset;
                animation.repeatCount = sourceAnimation.repeatCount;
                animation.autoreverses = sourceAnimation.autoreverses;
                
                float k = animationDurationFactorImpl();
                __unused float speed = 1.0f;
                if (k != 0.0 && k != 1.0) {
                    speed = 1.0f / k;
                }
                animation.speed = sourceAnimation.speed * (float)(animation.duration / parameters.duration);
                
                updatedAnimation = animation;
            } else if ([overrideData.parameters isKindOfClass:[CALayerSpringParametersOverrideParametersCustomCurve class]]) {
                CALayerSpringParametersOverrideParametersCustomCurve *parameters = (CALayerSpringParametersOverrideParametersCustomCurve *)overrideData.parameters;
                CABasicAnimation *sourceAnimation = (CABasicAnimation *)anim;
                
                CABasicAnimation *animation = [CABasicAnimation animationWithKeyPath:sourceAnimation.keyPath];
                animation.fromValue = sourceAnimation.fromValue;
                animation.toValue = sourceAnimation.toValue;
                animation.byValue = sourceAnimation.byValue;
                animation.additive = sourceAnimation.additive;
                animation.duration = sourceAnimation.duration;
                animation.timingFunction = [[CAMediaTimingFunction alloc] initWithControlPoints:parameters.cp1.x :parameters.cp1.y :parameters.cp2.x :parameters.cp2.y];
                animation.removedOnCompletion = sourceAnimation.isRemovedOnCompletion;
                animation.fillMode = sourceAnimation.fillMode;
                animation.speed = sourceAnimation.speed;
                animation.beginTime = sourceAnimation.beginTime;
                animation.timeOffset = sourceAnimation.timeOffset;
                animation.repeatCount = sourceAnimation.repeatCount;
                animation.autoreverses = sourceAnimation.autoreverses;
                
                float k = animationDurationFactorImpl();
                float speed = 1.0f;
                if (k != 0.0 && k != 1.0) {
                    speed = 1.0f / k;
                }
                animation.speed = speed * sourceAnimation.speed;
                
                updatedAnimation = animation;
            } else {
                bool isNativeGlass = false;
                if (@available(iOS 26.0, *)) {
                    isNativeGlass = true;
                }
                if (isNativeGlass && ABS(anim.duration - 0.3832) <= 0.0001) {
                } else if (ABS(anim.duration - 0.5) <= 0.0001) {
                } else {
                    CABasicAnimation *sourceAnimation = (CABasicAnimation *)anim;
                    
                    CABasicAnimation *animation = [CABasicAnimation animationWithKeyPath:sourceAnimation.keyPath];
                    animation.fromValue = sourceAnimation.fromValue;
                    animation.toValue = sourceAnimation.toValue;
                    animation.byValue = sourceAnimation.byValue;
                    animation.additive = sourceAnimation.additive;
                    animation.duration = sourceAnimation.duration;
                    animation.timingFunction = [[CAMediaTimingFunction alloc] initWithControlPoints:0.380 :0.700 :0.125 :1.000];
                    animation.removedOnCompletion = sourceAnimation.isRemovedOnCompletion;
                    animation.fillMode = sourceAnimation.fillMode;
                    animation.speed = sourceAnimation.speed;
                    animation.beginTime = sourceAnimation.beginTime;
                    animation.timeOffset = sourceAnimation.timeOffset;
                    animation.repeatCount = sourceAnimation.repeatCount;
                    animation.autoreverses = sourceAnimation.autoreverses;
                    
                    float k = animationDurationFactorImpl();
                    float speed = 1.0f;
                    if (k != 0.0 && k != 1.0) {
                        speed = 1.0f / k;
                    }
                    animation.speed = speed * sourceAnimation.speed;
                    
                    updatedAnimation = animation;
                }
            }
        }
    }
    [self _65087dc8_addAnimation:updatedAnimation forKey:key];
}

@end

static EffectSettingsContainerView *findTopmostEffectSuperview(UIView *view, int depth) {
    if (depth > 10) {
        return nil;
    }
    if ([view isKindOfClass:[EffectSettingsContainerView class]]) {
        return (EffectSettingsContainerView *)view;
    }
    if (view.superview != nil) {
        return findTopmostEffectSuperview(view.superview, depth + 1);
    } else {
        return nil;
    }
}

static id (*original_backdropLayerDidChangeLuma)(UIView *, SEL, CALayer *, double) = NULL;
static void replacement_backdropLayerDidChangeLuma(UIView *self, SEL selector, CALayer *layer, double luma) {
    EffectSettingsContainerView *topmostSuperview = findTopmostEffectSuperview(self, 0);
    if (topmostSuperview) {
        luma = MIN(MAX(luma, topmostSuperview.lumaMin), topmostSuperview.lumaMax);
    }
    original_backdropLayerDidChangeLuma(self, selector, layer, luma);
}

static void registerEffectViewOverrides(void) {
    NSMutableArray<NSString *> *nameList = [[NSMutableArray alloc] init];
    [nameList addObject:@"_TtC5UIKitP33_ACD4A08F4BE9D00246F2A9C24A80CA8817UISDFBackdropView"];
    NSString *selectorString = [@"backdropLayer" stringByAppendingString:@":didChangeLuma:"];
    
    for (NSString *name in nameList) {
        Class classValue = NSClassFromString(name);
        if (classValue == nil) {
            continue;
        }
        
        Method method = (Method)class_getInstanceMethod(classValue, NSSelectorFromString(selectorString));
        if (method) {
            const char *typeEncoding = method_getTypeEncoding(method);
            if (strcmp(typeEncoding, "v32@0:8@16d24") == 0) {
                original_backdropLayerDidChangeLuma = (id (*)(id, SEL, CALayer *, double))method_getImplementation(method);
                method_setImplementation(method, (IMP)&replacement_backdropLayerDidChangeLuma);
            }
        }
        break;
    }
}
@implementation EffectSettingsContainerView

- (instancetype)initWithFrame:(CGRect)frame {
    self = [super initWithFrame:frame];
    if (self != nil) {
        _lumaMin = 0.0;
        _lumaMax = 0.0;
    }
    return self;
}

@end

void ZoenTelegramInstallPresentationRuntime(void) {
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        Method original = class_getInstanceMethod([CALayer class], @selector(addAnimation:forKey:));
        Method replacement = class_getInstanceMethod([CALayer class], @selector(_65087dc8_addAnimation:forKey:));
        if (original && replacement) method_exchangeImplementations(original, replacement);
        if (@available(iOS 26.0, *)) registerEffectViewOverrides();
    });
}
