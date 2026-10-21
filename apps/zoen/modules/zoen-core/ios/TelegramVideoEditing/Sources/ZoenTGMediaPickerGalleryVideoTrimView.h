// Adapted from Telegram-iOS at f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838; GPL-2.0-or-later. See NOTICE.md.
#import "ZoenVideoEditingPrimitives.h"

@interface ZoenTGMediaPickerGalleryVideoTrimView : ZoenVideoHitControl

@property (nonatomic, copy) void(^didBeginEditing)(bool start);
@property (nonatomic, copy) void(^startHandleMoved)(CGPoint translation);
@property (nonatomic, copy) void(^endHandleMoved)(CGPoint translation);
@property (nonatomic, copy) void(^didEndEditing)(void);

@property (nonatomic, assign) bool trimmingEnabled;

- (void)setTrimming:(bool)trimming animated:(bool)animated;
- (void)setTrimmingEnabled:(bool)trimmingEnabled animated:(bool)animated;

@end
