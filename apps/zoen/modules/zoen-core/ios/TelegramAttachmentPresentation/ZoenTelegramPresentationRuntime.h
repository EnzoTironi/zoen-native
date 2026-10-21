// Adapted from TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// GPL-2.0-or-later; GPL-3.0 selected. See NOTICE.md and UPSTREAM_SOURCES.json.
#import <UIKit/UIKit.h>
@interface CALayerSpringParametersOverrideParameters : NSObject

- (instancetype _Nonnull)init;

@end

@interface CALayerSpringParametersOverrideParametersSpring : CALayerSpringParametersOverrideParameters

@property (nonatomic, readonly) CGFloat stiffness;
@property (nonatomic, readonly) CGFloat damping;
@property (nonatomic, readonly) double duration;

- (instancetype _Nonnull)initWithStiffness:(CGFloat)stiffness damping:(CGFloat)damping duration:(double)duration;

@end

@interface CALayerSpringParametersOverrideParametersCustomCurve : CALayerSpringParametersOverrideParameters

@property (nonatomic, readonly) CGPoint cp1;
@property (nonatomic, readonly) CGPoint cp2;

- (instancetype _Nonnull)initWithCp1:(CGPoint)cp1 cp2:(CGPoint)cp2;

@end

@interface CALayerSpringParametersOverride : NSObject

@property (nonatomic, strong, readonly) CALayerSpringParametersOverrideParameters * _Nullable parameters;

- (instancetype _Nonnull)initWithParameters:(CALayerSpringParametersOverrideParameters * _Nullable)parameters;

@end

@interface CALayer (TelegramAddAnimation)

+ (void)pushSpringParametersOverride:(CALayerSpringParametersOverride * _Nonnull)springParametersOverride;
+ (void)popSpringParametersOverride;

@end

@interface EffectSettingsContainerView : UIView

@property (nonatomic) double lumaMin;
@property (nonatomic) double lumaMax;

@end

#ifndef MeshTransformApi_h
#define MeshTransformApi_h

#import <Foundation/Foundation.h>
#import <UIKit/UIKit.h>

typedef struct MeshTransformMeshFace {
    unsigned int indices[4];
    float w[4];
} MeshTransformMeshFace;
 
typedef struct MeshTransformPoint3D {
    CGFloat x;
    CGFloat y;
    CGFloat z;
} MeshTransformPoint3D;
 
typedef struct MeshTransformMeshVertex {
    CGPoint from;
    MeshTransformPoint3D to;
} MeshTransformMeshVertex;

@protocol MeshTransformClass <NSObject>

- (id)meshTransformWithVertexCount:(NSUInteger)vertexCount
                                    vertices:(MeshTransformMeshVertex *)vertices
                                   faceCount:(NSUInteger)faceCount
                                       faces:(MeshTransformMeshFace *)faces
                          depthNormalization:(NSString *)depthNormalization;

@end

#endif

NS_ASSUME_NONNULL_BEGIN
double animationDurationFactorImpl(void);
CABasicAnimation *makeSpringAnimationImpl(NSString *keyPath, double duration);
CABasicAnimation *make26SpringAnimationImpl(NSString *keyPath, double duration);
CASpringAnimation *makeSpringBounceAnimationImpl(NSString *keyPath, CGFloat initialVelocity, CGFloat damping);
void setMonochromaticEffectImpl(UIView *view, bool isEnabled);
NSObject * _Nullable makeBlurFilter(void);
NSObject * _Nullable makeLuminanceToAlphaFilter(void);
NSObject * _Nullable makeColorMatrixFilter(void);
void ZoenTelegramInstallPresentationRuntime(void);
NS_ASSUME_NONNULL_END
