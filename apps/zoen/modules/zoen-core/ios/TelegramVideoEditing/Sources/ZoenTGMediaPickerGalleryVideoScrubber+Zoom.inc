// Adapted from Telegram-iOS at f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838; GPL-2.0-or-later. See NOTICE.md.

- (void)startHandleMoved:(CGPoint)translation {
    ZoenTGMediaPickerGalleryVideoScrubber *strongSelf = self;
    
    if (strongSelf->_animatingZoomIn)
        return;
    
    UIView *trimView = strongSelf->_trimView;
    
    CGRect availableTrimRect = [strongSelf _scrubbingRect];
    CGRect normalScrubbingRect = [strongSelf _scrubbingRectZoomedIn:false];
    CGFloat originX = MAX(0, trimView.frame.origin.x + translation.x);
    CGFloat delta = originX - trimView.frame.origin.x;
    CGFloat maxWidth = availableTrimRect.size.width + normalScrubbingRect.origin.x * 2 - originX;
    
    CGRect trimViewRect = CGRectMake(originX, trimView.frame.origin.y, MIN(maxWidth, trimView.frame.size.width - delta), trimView.frame.size.height);
    
    NSTimeInterval trimStartPosition = 0.0;
    NSTimeInterval trimEndPosition = 0.0;
    [strongSelf _trimStartPosition:&trimStartPosition trimEndPosition:&trimEndPosition forTrimFrame:trimViewRect duration:strongSelf.duration];
    
    NSTimeInterval duration = trimEndPosition - trimStartPosition;
    
    if (trimEndPosition - trimStartPosition < self.minimumLength)
        return;
    
    if (strongSelf.maximumLength > DBL_EPSILON && duration > strongSelf.maximumLength)
    {
        trimViewRect = CGRectMake(trimView.frame.origin.x + delta, trimView.frame.origin.y, trimView.frame.size.width, trimView.frame.size.height);
        
        [strongSelf _trimStartPosition:&trimStartPosition trimEndPosition:&trimEndPosition forTrimFrame:trimViewRect duration:strongSelf.duration];
    }
    
    trimView.frame = trimViewRect;
    
    [strongSelf _layoutTrimCurtainViews];
    
    strongSelf->_trimStartValue = trimStartPosition;
    strongSelf->_trimEndValue = trimEndPosition;
    
    [strongSelf setValue:trimStartPosition];
    
    UIView *handle = strongSelf->_scrubberHandle;
    handle.center = CGPointMake(trimView.frame.origin.x + 12 + handle.frame.size.width / 2, handle.center.y);
    
    UIView *dotHandle = strongSelf->_dotHandle;
    dotHandle.center = CGPointMake(trimView.frame.origin.x + 12 + dotHandle.frame.size.width / 2, dotHandle.center.y);
    
    id<ZoenTGMediaPickerGalleryVideoScrubberDelegate> delegate = strongSelf.delegate;
    if ([delegate respondsToSelector:@selector(videoScrubber:editingStartValueDidChange:)])
        [delegate videoScrubber:strongSelf editingStartValueDidChange:trimStartPosition];
    
    [strongSelf cancelZoomIn];
    if ([strongSelf zoomAvailable])
    {
        strongSelf->_pivotSource = ZoenTGMediaPickerGalleryVideoScrubberPivotSourceTrimStart;
        [strongSelf performSelector:@selector(zoomIn) withObject:nil afterDelay:TGVideoScrubberZoomActivationInterval];
    }
}

- (void)setHasDotPicker:(bool)hasDotPicker {
    _hasDotPicker = hasDotPicker;
    _tapGestureRecognizer.enabled = hasDotPicker;
}

- (void)setDotVideoView:(UIView *)dotVideoView {
    _dotVideoView = dotVideoView;
    _dotVideoView.frame = _dotImageView.frame;
    [_dotContentView addSubview:_dotVideoView];
}

- (void)setDotImage:(UIImage *)dotImage {
    _dotImageView.image = dotImage;
}

- (bool)zoomAvailable
{
    if (_disableZoom || _zoomedIn || _preparingToZoomIn || _summaryTimestamps.count == 0)
        return false;
    
    return _duration > 1.0f;
}

- (void)zoomIn
{
    if (![self zoomAvailable])
        return;
    
    _preparingToZoomIn = true;
    
    NSTimeInterval trimStartValue = 0.0;
    NSTimeInterval trimEndValue = 0.0;
    
    [self _trimStartPosition:&trimStartValue trimEndPosition:&trimEndValue forTrimFrame:_trimView.frame duration:_duration];
    
    switch (_pivotSource)
    {
        case ZoenTGMediaPickerGalleryVideoScrubberPivotSourceTrimStart:
            _zoomPivotCenter = [self _zoomPivotCenterForTrimStart];
            _zoomPivotPosition = trimStartValue;
            break;
            
        case ZoenTGMediaPickerGalleryVideoScrubberPivotSourceTrimEnd:
            _zoomPivotCenter = [self _zoomPivotCenterForTrimEnd];
            _zoomPivotPosition = trimEndValue;
            break;
            
        default:
            _zoomPivotCenter = [self _zoomPivotCenterForHandle];
            _zoomPivotPosition = [self _positionForScrubberPosition:_scrubberHandle.center duration:_duration];
            break;
    }
    
    if (_summaryTimestamps.count > 1)
        _zoomedDuration = [_summaryTimestamps[1] doubleValue] - [_summaryTimestamps[0] doubleValue];

    __block NSTimeInterval minimalInterval = DBL_MAX;
    __block NSUInteger timestampIndex = 0;
    [_summaryTimestamps enumerateObjectsUsingBlock:^(NSNumber *timestamp, NSUInteger index, __unused BOOL *stop)
    {
        NSTimeInterval timestampValue = timestamp.doubleValue;
        NSTimeInterval interval = fabs(timestampValue - _zoomPivotPosition);
        if (interval < minimalInterval)
        {
            minimalInterval = interval;
            timestampIndex = index;
        }
    }];
    
    _zoomedPivotTimestampIndex = timestampIndex;
    
    id<ZoenTGMediaPickerGalleryVideoScrubberDataSource> dataSource = self.dataSource;
    
    NSInteger leftSummaryTimestampIndex = MAX(0, (NSInteger)_zoomedPivotTimestampIndex - 1);
    NSInteger rightSummaryTimestampIndex = MIN((NSInteger)_zoomedPivotTimestampIndex + 1, (NSInteger)_summaryTimestamps.count - 1);
    
    NSTimeInterval leftTimestamp = [[_summaryTimestamps objectAtIndex:leftSummaryTimestampIndex] doubleValue];
    NSTimeInterval rightTimestamp = [[_summaryTimestamps objectAtIndex:rightSummaryTimestampIndex] doubleValue];
    
    if ((NSUInteger)rightSummaryTimestampIndex == _summaryTimestamps.count - 1)
        rightTimestamp = _duration;
    
    CGSize thumbnailImageSize = [self _thumbnailSize];
    CGFloat countMultiplier = 1.0f;
    if (_zoomedPivotTimestampIndex > 01)
        countMultiplier = 2.1f;
    NSInteger thumbnailCount = (NSInteger)ceil(_summaryThumbnailWrapperView.frame.size.width / thumbnailImageSize.width * countMultiplier);
    
    if ([dataSource respondsToSelector:@selector(videoScrubber:evenlySpacedTimestamps:startingAt:endingAt:)])
        _zoomedTimestamps = [dataSource videoScrubber:self evenlySpacedTimestamps:thumbnailCount startingAt:leftTimestamp endingAt:rightTimestamp];
    
    CGFloat scale = MIN(2.0f, self.traitCollection.displayScale);
    thumbnailImageSize = CGSizeMake(thumbnailImageSize.width * scale, thumbnailImageSize.height * scale);
    
    _zoomedThumbnailViews = [[NSMutableArray alloc] init];
    
    if ([dataSource respondsToSelector:@selector(videoScrubber:requestThumbnailImagesForTimestamps:size:isSummaryThumbnails:)])
        [dataSource videoScrubber:self requestThumbnailImagesForTimestamps:_zoomedTimestamps size:thumbnailImageSize isSummaryThumbnails:false];
}

- (CGFloat)_zoomPivotCenterForHandle
{
    CGFloat duration = MAX(0.0001, _duration);
    CGFloat fractValue = (CGFloat)_value / duration * 2 - 1;
    return _scrubberHandle.center.x - [self _scrubbingRectZoomedIn:false].origin.x + fractValue * _scrubberHandle.frame.size.width / 2;
}

- (CGFloat)_zoomPivotCenterForTrimStart
{
    return CGRectGetMinX(_trimView.frame);
}

- (CGFloat)_zoomPivotCenterForTrimEnd
{
    CGRect scrubbingRect = [self _scrubbingRectZoomedIn:false];
    return CGRectGetMaxX(_trimView.frame) - scrubbingRect.origin.x * 2;
}

- (void)commitZoomIn
{
    if (!_preparingToZoomIn)
        return;
    
    if (_summaryThumbnailViews.count == 0)
        return;
    
    _zoomedIn = true;
    _preparingToZoomIn = false;
    _animatingZoomIn = true;
    
    [self _layoutZoomedThumbnailViewsStacked:true];
    
    NSTimeInterval pivotTimestamp = [_summaryTimestamps[_zoomedPivotTimestampIndex] doubleValue];
    CGRect normalPivotFrame = [_summaryThumbnailViews[_zoomedPivotTimestampIndex] frame];
    CGRect normalScrubbingRect = [self _scrubbingRectZoomedIn:false];
    CGRect zoomedScrubbingRect = [self _scrubbingRectZoomedIn:true];
    
    CGFloat duration = MAX(0.0001, _duration);
    CGFloat zoomedPivotPosition = (zoomedScrubbingRect.size.width - [self _thumbnailSize].width) * (CGFloat)pivotTimestamp / (CGFloat)duration;
    _zoomPivotOffset = zoomedPivotPosition - normalPivotFrame.origin.x + zoomedScrubbingRect.origin.x - normalScrubbingRect.origin.x;
    
    _summaryThumbnailWrapperView.clipsToBounds = false;
    [UIView animateWithDuration:0.3f delay:0.0f options:UIViewAnimationOptionCurveEaseInOut | UIViewAnimationOptionLayoutSubviews | UIViewAnimationOptionAllowUserInteraction animations:^
    {
        [self _layoutZoomedThumbnailViewsStacked:false];
        [self _layoutSummaryThumbnailViewsForZoom:true];
        [self _layoutTrimViewZoomedIn:_zoomedIn];
    } completion:^(__unused BOOL finished)
    {
        _animatingZoomIn = false;
    }];
}

- (void)cancelZoomIn
{
    [NSObject cancelPreviousPerformRequestsWithTarget:self selector:@selector(zoomIn) object:nil];
    
    if (!_preparingToZoomIn)
        return;
    
    _preparingToZoomIn = false;
    
    id<ZoenTGMediaPickerGalleryVideoScrubberDelegate> delegate = self.delegate;
    if ([delegate respondsToSelector:@selector(videoScrubberDidCancelRequestingThumbnails:)])
        [delegate videoScrubberDidCancelRequestingThumbnails:self];
    
    [self _resetZooming];
}

- (void)zoomOut
{
    _animatingZoomOut = true;
    _trimView.userInteractionEnabled = false;
    
    [UIView animateWithDuration:0.3f delay:0.0f options:UIViewAnimationOptionCurveEaseInOut | UIViewAnimationOptionLayoutSubviews animations:^
    {
        [self _layoutSummaryThumbnailViewsForZoom:false];
        [self _layoutZoomedThumbnailViewsStacked:true];
        [self _layoutTrimViewZoomedIn:false];
        [self _updateScrubberAnimationsAndResetCurrentPosition:true zoomedIn:false];
    } completion:^(__unused BOOL finished)
    {
        _zoomedIn = false;
        _animatingZoomOut = false;
        
        [self _resetZooming];
        
        _trimView.userInteractionEnabled = true;
        _summaryThumbnailWrapperView.clipsToBounds = true;
    }];
}

- (void)_resetZooming
{
    _zoomedIn = false;
    _zoomedDuration = 0.0;
    _zoomPivotPosition = 0.0f;
    _zoomPivotOffset = 0.0f;
    _zoomedPivotTimestampIndex = -1;
    for (UIView *view in _zoomedThumbnailWrapperView.subviews)
        [view removeFromSuperview];
    _zoomedThumbnailViews = nil;
}
