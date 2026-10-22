// Adapted from Telegram-iOS at f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838; GPL-2.0-or-later. See NOTICE.md.
#import "Private/ZoenTGMediaPickerGalleryVideoScrubberPrivate.h"

@implementation ZoenTGMediaPickerGalleryVideoScrubber

- (instancetype)initWithFrame:(CGRect)frame cover:(bool)cover
{
    self = [super initWithFrame:frame];
    if (self != nil)
    {
        _allowsTrimming = true;
        _minimumLength = TGVideoScrubberMinimumTrimDuration;
        
        _currentTimeLabel = [[UILabel alloc] initWithFrame:CGRectMake(8, 4, 100, 15)];
        _currentTimeLabel.font = [UIFont systemFontOfSize:12.0f];
        _currentTimeLabel.backgroundColor = [UIColor clearColor];
        _currentTimeLabel.text = @"0:00";
        _currentTimeLabel.textColor = [UIColor whiteColor];
        _currentTimeLabel.layer.shadowOffset = CGSizeMake(0.0, 0.0);
        _currentTimeLabel.layer.shadowRadius = 4.0;
        _currentTimeLabel.layer.shadowColor = [UIColor blackColor].CGColor;
        _currentTimeLabel.layer.shadowOpacity = 0.6;
        _currentTimeLabel.layer.rasterizationScale = self.traitCollection.displayScale;
        _currentTimeLabel.layer.shouldRasterize = true;
        //[self addSubview:_currentTimeLabel];
        
        _inverseTimeLabel = [[UILabel alloc] initWithFrame:CGRectMake(frame.size.width - 108, 4, 100, 15)];
        _inverseTimeLabel.autoresizingMask = UIViewAutoresizingFlexibleLeftMargin;
        _inverseTimeLabel.font = [UIFont systemFontOfSize:12.0f];
        _inverseTimeLabel.backgroundColor = [UIColor clearColor];
        _inverseTimeLabel.text = @"0:00";
        _inverseTimeLabel.textAlignment = NSTextAlignmentRight;
        _inverseTimeLabel.textColor = [UIColor whiteColor];
        _inverseTimeLabel.layer.shadowOffset = CGSizeMake(0.0, 0.0);
        _inverseTimeLabel.layer.shadowRadius = 4.0;
        _inverseTimeLabel.layer.shadowColor = [UIColor blackColor].CGColor;
        _inverseTimeLabel.layer.shadowOpacity = 0.6;
        _inverseTimeLabel.layer.rasterizationScale = self.traitCollection.displayScale;
        _inverseTimeLabel.layer.shouldRasterize = true;
        //[self addSubview:_inverseTimeLabel];
        
        _wrapperView = [[ZoenVideoHitControl alloc] initWithFrame:CGRectMake(8, 24, 0, 40)];
        _wrapperView.hitTestEdgeInsets = UIEdgeInsetsMake(-5, -10, -5, -10);
        [self addSubview:_wrapperView];
        
        _zoomedThumbnailWrapperView = [[UIView alloc] initWithFrame:CGRectMake(0, 0, 0, 32)];
        [_wrapperView addSubview:_zoomedThumbnailWrapperView];
        
        _summaryThumbnailWrapperView = [[UIView alloc] initWithFrame:CGRectMake(0, 0, 0, 32)];
        _summaryThumbnailWrapperView.clipsToBounds = true;
        _summaryThumbnailWrapperView.layer.cornerRadius = 9.0;
        [_wrapperView addSubview:_summaryThumbnailWrapperView];
        
        static dispatch_once_t onceToken;
        static UIImage *leftCurtain;
        static UIImage *rightCurtain;
        dispatch_once(&onceToken, ^
        {
            UIGraphicsBeginImageContextWithOptions(CGSizeMake(11.0f, 40.0f), false, 0.0f);
            CGContextRef context = UIGraphicsGetCurrentContext();
            
            CGContextSetFillColorWithColor(context, ZoenVideoColorRGBA(0x000000, 0.8).CGColor);
            UIBezierPath *path = [UIBezierPath bezierPathWithRoundedRect:CGRectMake(0, 0, 40, 40) cornerRadius:9.0];
            CGContextAddPath(context, path.CGPath);
            CGContextFillPath(context);
            
            leftCurtain = UIGraphicsGetImageFromCurrentImageContext();
            UIGraphicsEndImageContext();
            
            UIGraphicsBeginImageContextWithOptions(CGSizeMake(11.0f, 40.0f), false, 0.0f);
            context = UIGraphicsGetCurrentContext();
            
            CGContextSetFillColorWithColor(context, ZoenVideoColorRGBA(0x000000, 0.8).CGColor);
            path = [UIBezierPath bezierPathWithRoundedRect:CGRectMake(-29.0, 0, 40, 40) cornerRadius:9.0];
            CGContextAddPath(context, path.CGPath);
            CGContextFillPath(context);
            
            rightCurtain = UIGraphicsGetImageFromCurrentImageContext();
            UIGraphicsEndImageContext();
        });
        
        _leftCurtainView = [[UIImageView alloc] initWithFrame:CGRectMake(0, 0, 0, 0)];
        _leftCurtainView.image = [leftCurtain resizableImageWithCapInsets:UIEdgeInsetsMake(0, 11, 0, 0)];
        _leftCurtainView.clipsToBounds = true;
        [_wrapperView addSubview:_leftCurtainView];
        
        _rightCurtainView = [[UIImageView alloc] initWithFrame:CGRectMake(0, 0, 0, 0)];
        _rightCurtainView.image = [rightCurtain resizableImageWithCapInsets:UIEdgeInsetsMake(0, 0, 0, 11)];
        _rightCurtainView.clipsToBounds = true;
        [_wrapperView addSubview:_rightCurtainView];
        
        __weak ZoenTGMediaPickerGalleryVideoScrubber *weakSelf = self;
        _trimView = [[ZoenTGMediaPickerGalleryVideoTrimView alloc] initWithFrame:CGRectZero];
        _trimView.exclusiveTouch = true;
        _trimView.trimmingEnabled = _allowsTrimming;
        _trimView.didBeginEditing = ^(bool start)
        {
            __strong ZoenTGMediaPickerGalleryVideoScrubber *strongSelf = weakSelf;
            if (strongSelf == nil)
                return;
            
            id<ZoenTGMediaPickerGalleryVideoScrubberDelegate> delegate = strongSelf.delegate;
            if ([delegate respondsToSelector:@selector(videoScrubberDidBeginEditing:)])
                [delegate videoScrubberDidBeginEditing:strongSelf];
            
            [strongSelf cancelZoomIn];
            if ([strongSelf zoomAvailable])
            {
                if (start)
                    strongSelf->_pivotSource = ZoenTGMediaPickerGalleryVideoScrubberPivotSourceTrimStart;
                else
                    strongSelf->_pivotSource = ZoenTGMediaPickerGalleryVideoScrubberPivotSourceTrimEnd;
                
                [strongSelf performSelector:@selector(zoomIn) withObject:nil afterDelay:TGVideoScrubberZoomActivationInterval];
            }
            
            [strongSelf->_trimView setTrimming:true animated:true];
            
            [strongSelf setScrubberHandleHidden:true animated:false];
        };
        _trimView.didEndEditing = ^
        {
            __strong ZoenTGMediaPickerGalleryVideoScrubber *strongSelf = weakSelf;
            if (strongSelf == nil)
                return;
            
            id<ZoenTGMediaPickerGalleryVideoScrubberDelegate> delegate = strongSelf.delegate;
            if ([delegate respondsToSelector:@selector(videoScrubberDidEndEditing:)])
                [delegate videoScrubberDidEndEditing:strongSelf];
            
            CGRect newTrimRect = strongSelf->_trimView.frame;
            CGRect trimRect = [strongSelf _scrubbingRect];
            CGRect normalScrubbingRect = [strongSelf _scrubbingRectZoomedIn:false];
            CGFloat maxWidth = trimRect.size.width + normalScrubbingRect.origin.x * 2;
            
            CGFloat leftmostPosition = trimRect.origin.x - normalScrubbingRect.origin.x;
            if (newTrimRect.origin.x < leftmostPosition + TGVideoScrubberTrimRectEpsilon)
            {
                CGFloat delta = leftmostPosition - newTrimRect.origin.x;
                
                newTrimRect.origin.x += delta;
                newTrimRect.size.width = MIN(maxWidth, newTrimRect.size.width - delta);
            }
            
            CGFloat rightmostPosition = maxWidth;
            if (CGRectGetMaxX(newTrimRect) > maxWidth - TGVideoScrubberTrimRectEpsilon)
            {
                CGFloat delta = rightmostPosition - CGRectGetMaxX(newTrimRect);
                
                newTrimRect.size.width = MIN(maxWidth, newTrimRect.size.width + delta);
            }
            
            strongSelf->_trimView.frame = newTrimRect;
            
            NSTimeInterval trimStartPosition = 0.0;
            NSTimeInterval trimEndPosition = 0.0;
            
            [strongSelf _trimStartPosition:&trimStartPosition trimEndPosition:&trimEndPosition forTrimFrame:newTrimRect duration:strongSelf.duration];
            
            strongSelf->_trimStartValue = trimStartPosition;
            strongSelf->_trimEndValue = trimEndPosition;
            
            bool isTrimmed = (strongSelf->_trimStartValue > FLT_EPSILON || fabs(strongSelf->_trimEndValue - strongSelf->_duration) > FLT_EPSILON);
            
            [strongSelf->_trimView setTrimming:isTrimmed animated:true];
            
            [strongSelf setScrubberHandleHidden:false animated:true];
            
            [strongSelf cancelZoomIn];
            if (strongSelf->_zoomedIn)
                [strongSelf zoomOut];
        };
        _trimView.startHandleMoved = ^(CGPoint translation)
        {
            __strong ZoenTGMediaPickerGalleryVideoScrubber *strongSelf = weakSelf;
            if (strongSelf) {
                [strongSelf startHandleMoved:translation];
            }
        };
        _trimView.endHandleMoved = ^(CGPoint translation)
        {
            __strong ZoenTGMediaPickerGalleryVideoScrubber *strongSelf = weakSelf;
            if (strongSelf == nil)
                return;
            
            if (strongSelf->_animatingZoomIn)
                return;
            
            UIView *trimView = strongSelf->_trimView;
            
            CGRect availableTrimRect = [strongSelf _scrubbingRect];
            CGRect normalScrubbingRect = [strongSelf _scrubbingRectZoomedIn:false];
            CGFloat localOriginX = trimView.frame.origin.x - availableTrimRect.origin.x + normalScrubbingRect.origin.x;
            CGFloat maxWidth = availableTrimRect.size.width + normalScrubbingRect.origin.x * 2 - localOriginX;
            
            CGRect trimViewRect = CGRectMake(trimView.frame.origin.x,
                                             trimView.frame.origin.y,
                                             MIN(maxWidth, trimView.frame.size.width + translation.x),
                                             trimView.frame.size.height);
            
            NSTimeInterval trimStartPosition = 0.0;
            NSTimeInterval trimEndPosition = 0.0;
            [strongSelf _trimStartPosition:&trimStartPosition trimEndPosition:&trimEndPosition forTrimFrame:trimViewRect duration:strongSelf.duration];
            
            NSTimeInterval duration = trimEndPosition - trimStartPosition;
            
            if (trimEndPosition - trimStartPosition < strongSelf.minimumLength)
                return;
            
            if (strongSelf.maximumLength > DBL_EPSILON && duration > strongSelf.maximumLength)
            {
                trimViewRect = CGRectMake(trimView.frame.origin.x + translation.x,
                                          trimView.frame.origin.y,
                                          trimView.frame.size.width,
                                          trimView.frame.size.height);
                
                [strongSelf _trimStartPosition:&trimStartPosition trimEndPosition:&trimEndPosition forTrimFrame:trimViewRect duration:strongSelf.duration];
            }
            
            trimView.frame = trimViewRect;
            
            [strongSelf _layoutTrimCurtainViews];
            
            strongSelf->_trimStartValue = trimStartPosition;
            strongSelf->_trimEndValue = trimEndPosition;
            
            [strongSelf setValue:trimEndPosition];
            
            UIView *handle = strongSelf->_scrubberHandle;
            handle.center = CGPointMake(CGRectGetMaxX(trimView.frame) - 12 - handle.frame.size.width / 2, handle.center.y);
            
            UIView *dotHandle = strongSelf->_dotHandle;
            dotHandle.center = CGPointMake(CGRectGetMaxX(trimView.frame) - 12 + dotHandle.frame.size.width / 2, dotHandle.center.y);
            
            id<ZoenTGMediaPickerGalleryVideoScrubberDelegate> delegate = strongSelf.delegate;
            if ([delegate respondsToSelector:@selector(videoScrubber:editingEndValueDidChange:)])
                [delegate videoScrubber:strongSelf editingEndValueDidChange:trimEndPosition];
            
            [strongSelf cancelZoomIn];
            if ([strongSelf zoomAvailable])
            {
                strongSelf->_pivotSource = ZoenTGMediaPickerGalleryVideoScrubberPivotSourceTrimEnd;
                [strongSelf performSelector:@selector(zoomIn) withObject:nil afterDelay:TGVideoScrubberZoomActivationInterval];
            }
        };
        [_wrapperView addSubview:_trimView];
        
        _dotHandle = [[ZoenVideoHitControl alloc] initWithFrame:CGRectMake(0, -4.0f, 26.0f, 44.0f)];
        _dotHandle.hitTestEdgeInsets = UIEdgeInsetsMake(-5, -12, -5, -12);
        _dotHandle.hidden = true;
        [_wrapperView addSubview:_dotHandle];
        
        static UIImage *dotFrameImage = nil;
        static dispatch_once_t onceToken2;
        dispatch_once(&onceToken2, ^
                      {
            UIGraphicsBeginImageContextWithOptions(CGSizeMake(_dotHandle.frame.size.width, _dotHandle.frame.size.height), false, 0.0f);
            CGContextRef context = UIGraphicsGetCurrentContext();
            CGContextSetStrokeColorWithColor(context, [UIColor whiteColor].CGColor);
            CGContextSetLineWidth(context, 3.0);
            
            UIBezierPath *path = [UIBezierPath bezierPathWithRoundedRect:CGRectMake(1.5f, 1.5f, _dotHandle.frame.size.width - 3.0, _dotHandle.frame.size.height - 3.0f) cornerRadius:4.0f];
            CGContextAddPath(context, path.CGPath);
            CGContextStrokePath(context);
            
            dotFrameImage = UIGraphicsGetImageFromCurrentImageContext();
            UIGraphicsEndImageContext();
        });
        
        _dotContentView = [[UIView alloc] initWithFrame:CGRectInset(_dotHandle.bounds, 2.0, 2.0)];
        _dotContentView.clipsToBounds = true;
        [_dotHandle addSubview:_dotContentView];
        
        _dotImageView = [[UIImageView alloc] initWithFrame:_dotContentView.bounds];
        _dotImageView.clipsToBounds = true;
        _dotImageView.contentMode = UIViewContentModeScaleAspectFill;
        [_dotContentView addSubview:_dotImageView];
        
        _dotFrameView = [[UIImageView alloc] initWithFrame:_dotHandle.bounds];
        _dotFrameView.image = dotFrameImage;
        [_dotHandle addSubview:_dotFrameView];
        
        _scrubberHandle = [[ZoenVideoHitControl alloc] initWithFrame:CGRectMake(0, -4.0f, cover ? 30.0 : 5.0f, 48.0f)];
        _scrubberHandle.hitTestEdgeInsets = UIEdgeInsetsMake(-5, -12, -5, -12);
        [_wrapperView addSubview:_scrubberHandle];
        
        UIImage *handleViewImage = nil;
        {
            UIGraphicsBeginImageContextWithOptions(CGSizeMake(_scrubberHandle.frame.size.width, _scrubberHandle.frame.size.height), false, 0.0f);
            CGContextRef context = UIGraphicsGetCurrentContext();
            CGContextSetShadowWithColor(context, CGSizeMake(0, 0), 0.5f, [UIColor colorWithWhite:0.0f alpha:0.65f].CGColor);
            CGContextSetFillColorWithColor(context, [UIColor whiteColor].CGColor);
            
            if (cover) {
                CGFloat lineWidth = 2.0 - ZoenVideoSeparatorHeight(self);
                CGContextSetLineWidth(context, lineWidth);
                CGContextSetStrokeColorWithColor(context, UIColor.whiteColor.CGColor);
                UIBezierPath *path = [UIBezierPath bezierPathWithRoundedRect:CGRectInset(CGRectMake(0, 0, 30, 48), lineWidth / 2.0, lineWidth / 2.0) cornerRadius:8];
                CGContextAddPath(context, path.CGPath);
                CGContextStrokePath(context);
            } else {
                UIBezierPath *path = [UIBezierPath bezierPathWithRoundedRect:CGRectMake((_scrubberHandle.frame.size.width - 2.0) / 2.0, 1.5f, 2.0, _scrubberHandle.frame.size.height - 3.0f) cornerRadius:2.0f];
                CGContextAddPath(context, path.CGPath);
                CGContextFillPath(context);
            }
            
            handleViewImage = UIGraphicsGetImageFromCurrentImageContext();
            UIGraphicsEndImageContext();
        }
        
        UIImageView *scrubberImageView = [[UIImageView alloc] initWithFrame:_scrubberHandle.bounds];
        scrubberImageView.image = handleViewImage;
        [_scrubberHandle addSubview:scrubberImageView];
        
        _pressGestureRecognizer = [[UILongPressGestureRecognizer alloc] initWithTarget:self action:@selector(handlePress:)];
        _pressGestureRecognizer.delegate = self;
        _pressGestureRecognizer.minimumPressDuration = 0.1f;
        [_scrubberHandle addGestureRecognizer:_pressGestureRecognizer];
        
        _panGestureRecognizer = [[UIPanGestureRecognizer alloc] initWithTarget:self action:@selector(handlePan:)];
        _panGestureRecognizer.delegate = self;
        [_scrubberHandle addGestureRecognizer:_panGestureRecognizer];
        
        _dotPanGestureRecognizer = [[UIPanGestureRecognizer alloc] initWithTarget:self action:@selector(handlePan:)];
        _dotPanGestureRecognizer.delegate = self;
        [_dotHandle addGestureRecognizer:_dotPanGestureRecognizer];
        
        _tapGestureRecognizer = [[UITapGestureRecognizer alloc] initWithTarget:self action:@selector(handleTap:)];
        _tapGestureRecognizer.enabled = cover;
        [_trimView addGestureRecognizer:_tapGestureRecognizer];
    }
    return self;
}

#include "ZoenTGMediaPickerGalleryVideoScrubber+Zoom.inc"
#include "ZoenTGMediaPickerGalleryVideoScrubber+Thumbnails.inc"
#include "ZoenTGMediaPickerGalleryVideoScrubber+Playback.inc"
#include "ZoenTGMediaPickerGalleryVideoScrubber+Gestures.inc"
#include "ZoenTGMediaPickerGalleryVideoScrubber+TrimLayout.inc"

@end
