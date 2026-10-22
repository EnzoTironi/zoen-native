// Adapted from Telegram-iOS at f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838; GPL-2.0-or-later. See NOTICE.md.

- (void)reloadThumbnails
{
    [self resetThumbnails];
    
    id<ZoenTGMediaPickerGalleryVideoScrubberDataSource> dataSource = self.dataSource;
    
    _summaryThumbnailViews = [[NSMutableArray alloc] init];
    
    if ([dataSource respondsToSelector:@selector(videoScrubberOriginalSize:cropRect:cropOrientation:cropMirrored:)])
        _originalSize = [dataSource videoScrubberOriginalSize:self cropRect:&_cropRect cropOrientation:&_cropOrientation cropMirrored:&_cropMirrored];
    
    CGFloat originalAspectRatio = 1.0f;
    CGFloat frameAspectRatio = 1.0f;
    if ([dataSource respondsToSelector:@selector(videoScrubberThumbnailAspectRatio:)])
        originalAspectRatio = [dataSource videoScrubberThumbnailAspectRatio:self];
    
    if (!CGRectEqualToRect(_cropRect, CGRectZero))
        frameAspectRatio = _cropRect.size.width / _cropRect.size.height;
    else
        frameAspectRatio = originalAspectRatio;
    
    frameAspectRatio = MAX(0.5, frameAspectRatio);
 
    _thumbnailAspectRatio = frameAspectRatio;
        
    NSInteger thumbnailCount = (NSInteger)ceil(_summaryThumbnailWrapperView.frame.size.width / [self _thumbnailSizeWithAspectRatio:frameAspectRatio orientation:_cropOrientation].width);
    
    if ([dataSource respondsToSelector:@selector(videoScrubber:evenlySpacedTimestamps:startingAt:endingAt:)])
        _summaryTimestamps = [dataSource videoScrubber:self evenlySpacedTimestamps:thumbnailCount startingAt:0 endingAt:_duration];
    
    CGSize thumbnailImageSize = [self _thumbnailSizeWithAspectRatio:originalAspectRatio orientation:UIImageOrientationUp];
    CGFloat scale = MIN(2.0f, self.traitCollection.displayScale);
    thumbnailImageSize = CGSizeMake(thumbnailImageSize.width * scale, thumbnailImageSize.height * scale);
    
    if ([dataSource respondsToSelector:@selector(videoScrubber:requestThumbnailImagesForTimestamps:size:isSummaryThumbnails:)])
        [dataSource videoScrubber:self requestThumbnailImagesForTimestamps:_summaryTimestamps size:thumbnailImageSize isSummaryThumbnails:true];
}

- (void)ignoreThumbnails
{
    _ignoreThumbnailLoad = true;
}

- (void)resetThumbnails
{
    _ignoreThumbnailLoad = false;
    
    if (_summaryThumbnailViews.count < _summaryTimestamps.count || _zoomedThumbnailViews.count < _zoomedTimestamps.count)
    {
        id<ZoenTGMediaPickerGalleryVideoScrubberDelegate> delegate = self.delegate;
        if ([delegate respondsToSelector:@selector(videoScrubberDidCancelRequestingThumbnails:)])
            [delegate videoScrubberDidCancelRequestingThumbnails:self];
    }
    
    for (UIView *view in _summaryThumbnailWrapperView.subviews)
        [view removeFromSuperview];
    
    for (UIView *view in _zoomedThumbnailWrapperView.subviews)
        [view removeFromSuperview];
    
    _summaryThumbnailViews = nil;
    _zoomedThumbnailViews = nil;
    
    _summaryTimestamps = nil;
    _zoomedTimestamps = nil;
    
    [self _resetZooming];
}

- (void)updateThumbnails {
    UIView *snapshotView = [_summaryThumbnailWrapperView snapshotViewAfterScreenUpdates:true];
    snapshotView.frame = _summaryThumbnailWrapperView.frame;
    [_summaryThumbnailWrapperView.superview insertSubview:snapshotView aboveSubview:_summaryThumbnailWrapperView];
    
    id<ZoenTGMediaPickerGalleryVideoScrubberDataSource> dataSource = self.dataSource;
    if ([dataSource respondsToSelector:@selector(videoScrubberOriginalSize:cropRect:cropOrientation:cropMirrored:)])
        _originalSize = [dataSource videoScrubberOriginalSize:self cropRect:&_cropRect cropOrientation:&_cropOrientation cropMirrored:&_cropMirrored];
    
    for (ZoenTGMediaPickerGalleryVideoScrubberThumbnailView *view in _summaryThumbnailWrapperView.subviews) {
        view.cropRect = _cropRect;
        view.cropOrientation = _cropOrientation;
        view.cropMirrored = _cropMirrored;
        [view updateCropping];
    }
    
    for (ZoenTGMediaPickerGalleryVideoScrubberThumbnailView *view in _zoomedThumbnailWrapperView.subviews) {
        view.cropRect = _cropRect;
        view.cropOrientation = _cropOrientation;
        view.cropMirrored = _cropMirrored;
        [view updateCropping];
    }
    
    if (snapshotView != nil)
    {
        [UIView animateWithDuration:0.2f animations:^
        {
            snapshotView.alpha = 0.0f;
        } completion:^(__unused BOOL finished)
        {
            [snapshotView removeFromSuperview];
        }];
    }
}

- (void)reloadData
{
    [self reloadDataAndReset:true];
}

- (void)reloadDataAndReset:(bool)reset
{
    id<ZoenTGMediaPickerGalleryVideoScrubberDataSource> dataSource = self.dataSource;
    if ([dataSource respondsToSelector:@selector(videoScrubberDuration:)])
        _duration = [dataSource videoScrubberDuration:self];
    else
        return;
    
    if (!reset && _summaryThumbnailViews.count > 0 && _summaryThumbnailSnapshotView == nil)
    {
        _summaryThumbnailSnapshotView = [_summaryThumbnailWrapperView snapshotViewAfterScreenUpdates:true];
        _summaryThumbnailSnapshotView.frame = _summaryThumbnailWrapperView.frame;
        [_summaryThumbnailWrapperView.superview insertSubview:_summaryThumbnailSnapshotView aboveSubview:_summaryThumbnailWrapperView];
    }
    else if (reset)
    {
        [_summaryThumbnailSnapshotView removeFromSuperview];
        _summaryThumbnailSnapshotView = nil;
    }
    
    [self _layoutTrimViewZoomedIn:false];
    
    [self reloadThumbnails];
}

- (void)setThumbnailImage:(UIImage *)image forTimestamp:(NSTimeInterval)__unused timestamp index:(NSInteger)index isSummaryThubmnail:(bool)isSummaryThumbnail last:(bool)last
{
    if (isSummaryThumbnail)
    {
        if (index == 0 && _summaryThumbnailViews.count > 0 && _summaryThumbnailSnapshotView == nil) {
            _summaryThumbnailSnapshotView = [_summaryThumbnailWrapperView snapshotViewAfterScreenUpdates:false];
            _summaryThumbnailSnapshotView.frame = _summaryThumbnailWrapperView.frame;
            [_summaryThumbnailWrapperView.superview insertSubview:_summaryThumbnailSnapshotView aboveSubview:_summaryThumbnailWrapperView];
        }
        
        if (_summaryThumbnailViews.count >= index + 1) {
            [_summaryThumbnailViews[index] setImage:image animated:true];
        } else {
            ZoenTGMediaPickerGalleryVideoScrubberThumbnailView *thumbnailView = [[ZoenTGMediaPickerGalleryVideoScrubberThumbnailView alloc] initWithImage:image originalSize:_originalSize cropRect:_cropRect cropOrientation:_cropOrientation cropMirrored:_cropMirrored];
            [_summaryThumbnailWrapperView addSubview:thumbnailView];
            [_summaryThumbnailViews addObject:thumbnailView];
        }
    }
    else
    {
        ZoenTGMediaPickerGalleryVideoScrubberThumbnailView *thumbnailView = [[ZoenTGMediaPickerGalleryVideoScrubberThumbnailView alloc] initWithImage:image originalSize:_originalSize cropRect:_cropRect cropOrientation:_cropOrientation cropMirrored:_cropMirrored];
        [_zoomedThumbnailWrapperView addSubview:thumbnailView];
        [_zoomedThumbnailViews addObject:thumbnailView];
    }
    
    if (last)
    {
        if (!_ignoreThumbnailLoad)
        {
            id<ZoenTGMediaPickerGalleryVideoScrubberDelegate> delegate = self.delegate;
            if ([delegate respondsToSelector:@selector(videoScrubberDidFinishRequestingThumbnails:)])
                [delegate videoScrubberDidFinishRequestingThumbnails:self];
        }
        _ignoreThumbnailLoad = false;
        
        if (isSummaryThumbnail)
        {
            [self _layoutSummaryThumbnailViewsForZoom:false];
            
            UIView *snapshotView = _summaryThumbnailSnapshotView;
            _summaryThumbnailSnapshotView = nil;
                        
            if (snapshotView != nil)
            {
                [UIView animateWithDuration:0.2f animations:^
                {
                    snapshotView.alpha = 0.0f;
                } completion:^(__unused BOOL finished)
                {
                    [snapshotView removeFromSuperview];
                }];
            }
        }
        else
        {
            [self commitZoomIn];
        }
    }
}

- (CGSize)_thumbnailSize
{
    return [self _thumbnailSizeWithAspectRatio:_thumbnailAspectRatio orientation:_cropOrientation];
}

- (CGSize)_thumbnailSizeWithAspectRatio:(CGFloat)aspectRatio orientation:(UIImageOrientation)orientation
{
    if (aspectRatio < FLT_EPSILON || isnan(aspectRatio))
        aspectRatio = 1.0f;
    
    if (orientation == UIImageOrientationLeft || orientation == UIImageOrientationRight)
        aspectRatio = 1.0f / aspectRatio;
    return CGSizeMake(ceil(40.0f * aspectRatio), 40.0f);
}

- (void)_layoutSummaryThumbnailViewsForZoom:(bool)forZoom
{
    if (_summaryThumbnailViews.count == 0)
        return;
    
    CGSize thumbnailViewSize = [self _thumbnailSize];
    CGFloat totalWidth = thumbnailViewSize.width * _summaryThumbnailViews.count;
    CGFloat originX = (_summaryThumbnailWrapperView.frame.size.width - totalWidth) / 2;
    
    if (!forZoom)
    {
        [_summaryThumbnailViews enumerateObjectsUsingBlock:^(UIView *view, NSUInteger index, __unused BOOL *stop)
        {
            view.frame = CGRectMake(originX + thumbnailViewSize.width * index, 0, thumbnailViewSize.width, thumbnailViewSize.height);
        }];
    }
    else
    {
        CGRect leftThumbnailFrame = [_zoomedThumbnailViews.firstObject frame];
        CGRect rightThumbnailFrame = [_zoomedThumbnailViews.lastObject frame];
        
        CGRect pivotThumbnailFrame = CGRectMake(originX + thumbnailViewSize.width * _zoomedPivotTimestampIndex + _zoomPivotOffset, 0, thumbnailViewSize.width, thumbnailViewSize.height);
        
        [_summaryThumbnailViews enumerateObjectsUsingBlock:^(UIView *view, NSUInteger index, __unused BOOL *stop)
        {
            if ((NSInteger)index == _zoomedPivotTimestampIndex)
            {
                view.frame = pivotThumbnailFrame;
            }
            else
            {
                if ((NSInteger)index < _zoomedPivotTimestampIndex)
                {
                    CGFloat delta = pivotThumbnailFrame.origin.x - leftThumbnailFrame.origin.x + leftThumbnailFrame.size.width;
                    view.frame = CGRectMake(pivotThumbnailFrame.origin.x - delta * (_zoomedPivotTimestampIndex - index), 0, thumbnailViewSize.width, thumbnailViewSize.height);
                }
                else
                {
                    CGFloat delta = rightThumbnailFrame.origin.x + rightThumbnailFrame.size.width - pivotThumbnailFrame.origin.x;
                    view.frame = CGRectMake(pivotThumbnailFrame.origin.x + delta * (index - _zoomedPivotTimestampIndex), 0, thumbnailViewSize.width, thumbnailViewSize.height);
                }
            }
        }];
    }
}

- (void)_layoutZoomedThumbnailViewsStacked:(bool)stacked
{
    if (_zoomedThumbnailViews.count == 0 || _summaryThumbnailViews.count == 0)
        return;
    
    CGSize thumbnailViewSize = [self _thumbnailSize];
    CGRect stackFrame = [_summaryThumbnailViews[_zoomedPivotTimestampIndex] frame];
    
    if (stacked)
    {
        [_zoomedThumbnailViews enumerateObjectsUsingBlock:^(UIView *view, __unused NSUInteger index, __unused BOOL *stop)
        {
            view.frame = stackFrame;
        }];
    }
    else
    {
        NSTimeInterval zoomedPivotThumbnailTimestamp = [_summaryTimestamps[_zoomedPivotTimestampIndex] doubleValue];
        
        __block NSInteger i = 1;
        [_zoomedThumbnailViews enumerateObjectsUsingBlock:^(UIView *view, NSUInteger index, __unused BOOL *stop)
        {
            NSTimeInterval timestamp = [_zoomedTimestamps[index] doubleValue];
            if (timestamp >= zoomedPivotThumbnailTimestamp)
            {
                view.frame = CGRectMake(stackFrame.origin.x + thumbnailViewSize.width * i + _zoomPivotOffset, 0, thumbnailViewSize.width, thumbnailViewSize.height);
                i++;
            }
        }];
        
        i = 1;
        [_zoomedThumbnailViews enumerateObjectsWithOptions:NSEnumerationReverse usingBlock:^(UIView *view, NSUInteger index, __unused BOOL *stop)
        {
            NSTimeInterval timestamp = [_zoomedTimestamps[index] doubleValue];
            if (timestamp < zoomedPivotThumbnailTimestamp)
            {
                view.frame = CGRectMake(stackFrame.origin.x - thumbnailViewSize.width * i + _zoomPivotOffset, 0, thumbnailViewSize.width, thumbnailViewSize.height);
                i++;
            }
        }];
    }
}
