// Adapted from Telegram-iOS at f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838; GPL-2.0-or-later. See NOTICE.md.
#import "ZoenVideoEditingPrimitives.h"

#import "ZoenTGMediaPickerGalleryVideoScrubber.h"

@interface ZoenTGMediaPickerScrubberHeaderView : UIView

@property (nonatomic, assign) UIEdgeInsets safeAreaInset;
@property (nonatomic, strong) UIView *panelView;
@property (nonatomic, strong) ZoenTGMediaPickerGalleryVideoScrubber *scrubberView;
@property (nonatomic, strong) ZoenTGMediaPickerGalleryVideoScrubber *coverScrubberView;

@end
