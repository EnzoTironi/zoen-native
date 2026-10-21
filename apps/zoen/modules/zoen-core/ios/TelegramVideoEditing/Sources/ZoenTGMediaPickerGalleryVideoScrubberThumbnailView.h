// Adapted from Telegram-iOS at f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838; GPL-2.0-or-later. See NOTICE.md.
#import "ZoenVideoEditingPrimitives.h"

@interface ZoenTGMediaPickerGalleryVideoScrubberThumbnailView : UIView

@property (nonatomic, assign) CGRect cropRect;
@property (nonatomic, assign) UIImageOrientation cropOrientation;
@property (nonatomic, assign) bool cropMirrored;

@property (nonatomic, strong) UIImage *image;
- (void)setImage:(UIImage *)image animated:(bool)animated;

- (instancetype)initWithImage:(UIImage *)image originalSize:(CGSize)originalSize cropRect:(CGRect)cropRect cropOrientation:(UIImageOrientation)cropOrientation cropMirrored:(bool)cropMirrored;

- (void)updateCropping;
- (void)updateCropping:(bool)animated;

@end
