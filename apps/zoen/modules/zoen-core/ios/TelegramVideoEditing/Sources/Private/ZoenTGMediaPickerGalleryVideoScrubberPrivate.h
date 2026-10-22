#pragma once
// Adapted from Telegram-iOS at f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838; GPL-2.0-or-later. See NOTICE.md.
#import "../ZoenVideoEditingPrimitives.h"
#import "../ZoenTGMediaPickerGalleryVideoScrubber.h"





#import "../ZoenTGMediaPickerGalleryVideoScrubberThumbnailView.h"
#import "../ZoenTGMediaPickerGalleryVideoTrimView.h"

static const CGFloat TGVideoScrubberMinimumTrimDuration = 1.0f;
static const CGFloat TGVideoScrubberZoomActivationInterval = 0.25f;
static const CGFloat TGVideoScrubberTrimRectEpsilon = 3.0f;
static const CGFloat TGVideoScrubberPadding = 8.0f;

typedef enum
{
    ZoenTGMediaPickerGalleryVideoScrubberPivotSourceHandle,
    ZoenTGMediaPickerGalleryVideoScrubberPivotSourceTrimStart,
    ZoenTGMediaPickerGalleryVideoScrubberPivotSourceTrimEnd
} ZoenTGMediaPickerGalleryVideoScrubberPivotSource;

@interface ZoenTGMediaPickerGalleryVideoScrubber () <UIGestureRecognizerDelegate>
{
    ZoenVideoLinearFrameDriver *_frameDriver;
    UILabel *_currentTimeLabel;
    UILabel *_inverseTimeLabel;
    
    ZoenVideoHitControl *_wrapperView;
    UIView *_summaryThumbnailSnapshotView;
    UIView *_zoomedThumbnailWrapperView;
    UIView *_summaryThumbnailWrapperView;
    ZoenTGMediaPickerGalleryVideoTrimView *_trimView;
    UIImageView *_leftCurtainView;
    UIImageView *_rightCurtainView;
    ZoenVideoHitControl *_scrubberHandle;
    
    ZoenVideoHitControl *_dotHandle;
    UIView *_dotContentView;
    UIImageView *_dotImageView;
    __weak UIView *_dotVideoView;
    UIImageView *_dotFrameView;
    
    UIPanGestureRecognizer *_panGestureRecognizer;
    UIPanGestureRecognizer *_dotPanGestureRecognizer;
    UILongPressGestureRecognizer *_pressGestureRecognizer;
    UITapGestureRecognizer *_tapGestureRecognizer;
    
    bool _beganInteraction;
    bool _endedInteraction;
    
    bool _scrubbing;
    
    NSTimeInterval _duration;

    bool _ignoreThumbnailLoad;
    CGFloat _thumbnailAspectRatio;
    NSArray *_summaryTimestamps;
    NSMutableArray *_summaryThumbnailViews;
    
    CGSize _originalSize;
    CGRect _cropRect;
    UIImageOrientation _cropOrientation;
    bool _cropMirrored;
    
    bool _zoomedIn;
    bool _preparingToZoomIn;
    bool _cancelledZoomIn;
    bool _animatingZoomIn;
    bool _animatingZoomOut;
    
    ZoenTGMediaPickerGalleryVideoScrubberPivotSource _pivotSource;
    NSTimeInterval _zoomedDuration;
    NSTimeInterval _zoomPivotPosition;
    CGFloat _zoomPivotCenter;
    CGFloat _zoomPivotOffset;
    NSInteger _zoomedPivotTimestampIndex;
    NSArray *_zoomedTimestamps;
    NSMutableArray *_zoomedThumbnailViews;
}
@end
