// Adapted from Telegram-iOS at f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838; GPL-2.0-or-later. See NOTICE.md.
#import "ZoenVideoEditingPrimitives.h"

@protocol ZoenTGMediaPickerGalleryVideoScrubberDelegate;
@protocol ZoenTGMediaPickerGalleryVideoScrubberDataSource;

@interface ZoenTGMediaPickerGalleryVideoScrubber : ZoenVideoHitControl

@property (nonatomic, weak) id<ZoenTGMediaPickerGalleryVideoScrubberDelegate> delegate;
@property (nonatomic, weak) id<ZoenTGMediaPickerGalleryVideoScrubberDataSource> dataSource;

@property (nonatomic, readonly) NSTimeInterval duration;

@property (nonatomic, assign) bool allowsTrimming;
@property (nonatomic, readonly) bool hasTrimming;
@property (nonatomic, assign) NSTimeInterval trimStartValue;
@property (nonatomic, assign) NSTimeInterval trimEndValue;

@property (nonatomic, assign) bool hasDotPicker;
- (void)setDotVideoView:(UIView *)dotVideoView;
- (void)setDotImage:(UIImage *)dotImage;

@property (nonatomic, assign) NSTimeInterval minimumLength;
@property (nonatomic, assign) NSTimeInterval maximumLength;

@property (nonatomic, assign) bool disableZoom;
@property (nonatomic, assign) bool disableTimeDisplay;

@property (nonatomic, readonly) bool isScrubbing;
@property (nonatomic, assign) bool isPlaying;
@property (nonatomic, assign) NSTimeInterval value;

- (instancetype)initWithFrame:(CGRect)frame cover:(bool)cover;

- (void)setValue:(NSTimeInterval)value resetPosition:(bool)resetPosition;

- (void)setTrimApplied:(bool)trimApplied;

- (void)resetToStart;

/// Zoen lifecycle fence; never changes selection or writes media.
- (void)zoenSuspend;

- (void)reloadData;
- (void)reloadDataAndReset:(bool)reset;

- (void)reloadThumbnails;
- (void)ignoreThumbnails;
- (void)resetThumbnails;

- (void)updateThumbnails;

- (void)setThumbnailImage:(UIImage *)image forTimestamp:(NSTimeInterval)timestamp index:(NSInteger)index isSummaryThubmnail:(bool)isSummaryThumbnail last:(bool)last;

- (CGPoint)scrubberPositionForPosition:(NSTimeInterval)position;

- (void)_updateScrubberAnimationsAndResetCurrentPosition:(bool)resetCurrentPosition;
- (void)_layoutTrimCurtainViews;

@end

NS_SWIFT_UI_ACTOR
@protocol ZoenTGMediaPickerGalleryVideoScrubberDelegate <NSObject>

- (void)videoScrubberDidBeginScrubbing:(ZoenTGMediaPickerGalleryVideoScrubber *)videoScrubber;
- (void)videoScrubberDidEndScrubbing:(ZoenTGMediaPickerGalleryVideoScrubber *)videoScrubber;
- (void)videoScrubber:(ZoenTGMediaPickerGalleryVideoScrubber *)videoScrubber valueDidChange:(NSTimeInterval)position;

- (void)videoScrubberDidBeginEditing:(ZoenTGMediaPickerGalleryVideoScrubber *)videoScrubber;
- (void)videoScrubberDidEndEditing:(ZoenTGMediaPickerGalleryVideoScrubber *)videoScrubber;
- (void)videoScrubber:(ZoenTGMediaPickerGalleryVideoScrubber *)videoScrubber editingStartValueDidChange:(NSTimeInterval)startValue;
- (void)videoScrubber:(ZoenTGMediaPickerGalleryVideoScrubber *)videoScrubber editingEndValueDidChange:(NSTimeInterval)endValue;

- (void)videoScrubberDidFinishRequestingThumbnails:(ZoenTGMediaPickerGalleryVideoScrubber *)videoScrubber;
- (void)videoScrubberDidCancelRequestingThumbnails:(ZoenTGMediaPickerGalleryVideoScrubber *)videoScrubber;

@end

NS_SWIFT_UI_ACTOR
@protocol ZoenTGMediaPickerGalleryVideoScrubberDataSource <NSObject>

- (NSTimeInterval)videoScrubberDuration:(ZoenTGMediaPickerGalleryVideoScrubber *)videoScrubber;

- (NSArray *)videoScrubber:(ZoenTGMediaPickerGalleryVideoScrubber *)videoScrubber evenlySpacedTimestamps:(NSInteger)count startingAt:(NSTimeInterval)startTimestamp endingAt:(NSTimeInterval)endTimestamp;

- (void)videoScrubber:(ZoenTGMediaPickerGalleryVideoScrubber *)videoScrubber requestThumbnailImagesForTimestamps:(NSArray *)timestamps size:(CGSize)size isSummaryThumbnails:(bool)isSummaryThumbnails;

- (CGFloat)videoScrubberThumbnailAspectRatio:(ZoenTGMediaPickerGalleryVideoScrubber *)videoScrubber;

- (CGSize)videoScrubberOriginalSize:(ZoenTGMediaPickerGalleryVideoScrubber *)videoScrubber cropRect:(CGRect *)cropRect cropOrientation:(UIImageOrientation *)cropOrientation cropMirrored:(bool *)cropMirrored;

@end
