// Adapted from Telegram-iOS at f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838; GPL-2.0-or-later. See NOTICE.md.

- (void)setScrubberHandleHidden:(bool)hidden animated:(bool)animated
{
    if (animated)
    {
        _scrubberHandle.hidden = false;
        [UIView animateWithDuration:0.25f animations:^
        {
            _scrubberHandle.alpha = hidden ? 0.0f : 1.0f;
        } completion:^(BOOL finished)
        {
            if (finished)
                _scrubberHandle.hidden = hidden;
        }];
    }
    else
    {
        _scrubberHandle.hidden = hidden;
        _scrubberHandle.alpha = hidden ? 0.0f : 1.0f;
    }
}

- (void)setDotHandleHidden:(bool)hidden animated:(bool)animated
{
    if (animated)
    {
        _dotHandle.hidden = false;
        [UIView animateWithDuration:0.25f animations:^
        {
            _dotHandle.alpha = hidden ? 0.0f : 1.0f;
        } completion:^(BOOL finished)
        {
            if (finished)
                _dotHandle.hidden = hidden;
        }];
    }
    else
    {
        _dotHandle.hidden = hidden;
        _dotHandle.alpha = hidden ? 0.0f : 1.0f;
    }
}

- (CGPoint)scrubberPositionForPosition:(NSTimeInterval)position
{
    return [self _scrubberPositionForPosition:position duration:_duration];
}

- (CGPoint)_scrubberPositionForPosition:(NSTimeInterval)position duration:(NSTimeInterval)duration
{
    return [self _scrubberPositionForPosition:position duration:duration zoomedIn:_zoomedIn];
}

- (CGPoint)_scrubberPositionForPosition:(NSTimeInterval)position duration:(NSTimeInterval)duration zoomedIn:(bool)zoomedIn
{
    CGRect scrubbingRect = [self _scrubbingRectZoomedIn:zoomedIn];
    
    if (duration < FLT_EPSILON)
    {
        position = 0.0;
        duration = 1.0;
    }
    
    return CGPointMake(_scrubberHandle.frame.size.width / 2 + scrubbingRect.origin.x + (CGFloat)(position / duration) * (scrubbingRect.size.width - _scrubberHandle.frame.size.width), CGRectGetMidY([self _scrubbingRect]));
}

- (NSTimeInterval)_positionForScrubberPosition:(CGPoint)scrubberPosition duration:(NSTimeInterval)duration
{
    CGRect scrubbingRect = [self _scrubbingRect];
    return (scrubberPosition.x - _scrubberHandle.frame.size.width / 2 - scrubbingRect.origin.x) / (scrubbingRect.size.width - _scrubberHandle.frame.size.width) * duration;
}

- (CGRect)_scrubbingRect
{
    return [self _scrubbingRectZoomedIn:_zoomedIn];
}

- (CGRect)_scrubbingRectZoomedIn:(bool)zoomedIn
{
    CGFloat width = self.frame.size.width;
    CGFloat origin = 0;
    CGFloat handleWidth = self.allowsTrimming ? 12.0f : 0.0f;
    
    width = width - handleWidth * 2.0f - TGVideoScrubberPadding * 2.0f;
    origin = handleWidth;
    
    if (zoomedIn)
    {
        CGFloat zoomedDuration = _zoomedDuration;
        if (zoomedDuration < FLT_EPSILON)
            zoomedDuration = _duration;
        
        CGFloat newWidth = (CGFloat)(width * _duration / _zoomedDuration);
        CGFloat newPosition = _zoomPivotCenter * newWidth / width;
        
        origin += _zoomPivotCenter - newPosition;
        
        width = newWidth;
    }
    
    return CGRectMake(origin, 24, width, 40);
}

#pragma mark - Dot

- (CGPoint)_dotPositionForPosition:(NSTimeInterval)position duration:(NSTimeInterval)duration
{
    CGRect scrubbingRect = [self _scrubbingRectZoomedIn:false];
    
    if (duration < FLT_EPSILON)
    {
        position = 0.0;
        duration = 1.0;
    }
    
    return CGPointMake(_dotHandle.frame.size.width / 2 + scrubbingRect.origin.x + (CGFloat)(position / duration) * (scrubbingRect.size.width - _dotHandle.frame.size.width), CGRectGetMidY([self _scrubbingRect]));
}

#pragma mark - Trimming

- (bool)hasTrimming
{
    return (_allowsTrimming && (_trimStartValue > FLT_EPSILON || _trimEndValue < _duration));
}

- (void)setAllowsTrimming:(bool)allowsTrimming
{
    _allowsTrimming = allowsTrimming;
    _trimView.trimmingEnabled = allowsTrimming;
}

- (void)setTrimStartValue:(NSTimeInterval)trimStartValue
{
    _trimStartValue = trimStartValue;
    
    [self _layoutTrimViewZoomedIn:_zoomedIn];
    
    if (_value < _trimStartValue)
    {
        [self setValue:_trimStartValue];
        _scrubberHandle.center = CGPointMake(_trimView.frame.origin.x + 12 + _scrubberHandle.frame.size.width / 2, _scrubberHandle.center.y);
    }
}

- (void)setTrimEndValue:(NSTimeInterval)trimEndValue
{
    _trimEndValue = trimEndValue;
    
    [self _layoutTrimViewZoomedIn:_zoomedIn];
    
    if (_value > _trimEndValue)
    {
        [self setValue:_trimEndValue];
        _scrubberHandle.center = CGPointMake(CGRectGetMaxX(_trimView.frame) - 12 - _scrubberHandle.frame.size.width / 2, _scrubberHandle.center.y);
    }
}

- (void)setTrimApplied:(bool)trimApplied
{
    [_trimView setTrimming:trimApplied animated:false];
}

- (void)_trimStartPosition:(NSTimeInterval *)trimStartPosition trimEndPosition:(NSTimeInterval *)trimEndPosition forTrimFrame:(CGRect)trimFrame duration:(NSTimeInterval)duration
{
    if (trimStartPosition == NULL || trimEndPosition == NULL)
        return;
    
    CGRect trimRect = [self _scrubbingRect];
    
    *trimStartPosition = (CGRectGetMinX(trimFrame) + 12 - trimRect.origin.x) / trimRect.size.width * duration;
    *trimEndPosition = (CGRectGetMaxX(trimFrame) - 12 - trimRect.origin.x) / trimRect.size.width * duration;
}

- (CGRect)_trimFrameForStartPosition:(NSTimeInterval)startPosition endPosition:(NSTimeInterval)endPosition duration:(NSTimeInterval)duration zoomedIn:(bool)zoomedIn
{
    CGRect trimRect = [self _scrubbingRectZoomedIn:zoomedIn];
    CGRect normalScrubbingRect = [self _scrubbingRectZoomedIn:false];
    
    CGFloat minX = duration > FLT_EPSILON ? ((CGFloat)startPosition * trimRect.size.width / (CGFloat)duration + trimRect.origin.x - normalScrubbingRect.origin.x) : 0.0f;
    CGFloat maxX = duration > FLT_EPSILON ? ((CGFloat)endPosition * trimRect.size.width / (CGFloat)duration + trimRect.origin.x + normalScrubbingRect.origin.x) : 0.0f;
    
    return CGRectMake(minX, 0, maxX - minX, 40);
}

- (void)_layoutTrimViewZoomedIn:(bool)zoomedIn
{
    if (_duration > DBL_EPSILON)
    {
        NSTimeInterval endPosition = _trimEndValue;
        if (endPosition < DBL_EPSILON)
            endPosition = _duration;
        
        _trimView.frame = [self _trimFrameForStartPosition:_trimStartValue endPosition:_trimEndValue duration:_duration zoomedIn:zoomedIn];
    }
    else
    {
        _trimView.frame = _wrapperView.bounds;
    }
    
    [self _layoutTrimCurtainViews];
}

- (void)_layoutTrimCurtainViews
{
    _leftCurtainView.hidden = !self.allowsTrimming;
    _rightCurtainView.hidden = !self.allowsTrimming;
    
    if (self.allowsTrimming)
    {
        CGRect scrubbingRect = [self _scrubbingRect];
        CGRect normalScrubbingRect = [self _scrubbingRectZoomedIn:false];
        
        _leftCurtainView.frame = CGRectMake(scrubbingRect.origin.x - 12.0f, 0.0f, _trimView.frame.origin.x - scrubbingRect.origin.x + normalScrubbingRect.origin.x + 11.0f, 40.0f);
        _rightCurtainView.frame = CGRectMake(CGRectGetMaxX(_trimView.frame) - 7.0f, 0.0, scrubbingRect.origin.x + scrubbingRect.size.width - CGRectGetMaxX(_trimView.frame) - scrubbingRect.origin.x + normalScrubbingRect.origin.x + 7.0f + 12.0f, 40.0f);
    }
}

- (void)setFrame:(CGRect)frame
{
    if (isnan(frame.origin.x) || isnan(frame.origin.y) || isnan(frame.size.width) || isnan(frame.size.height))
        return;
    
    [super setFrame:frame];
}

#pragma mark - Layout

- (void)layoutSubviews
{
    _wrapperView.frame = CGRectMake(TGVideoScrubberPadding, 24, self.frame.size.width - TGVideoScrubberPadding * 2.0f, 40);
    [self _layoutTrimViewZoomedIn:_zoomedIn];
    
    CGRect scrubbingRect = [self _scrubbingRect];
    if (isnan(scrubbingRect.origin.x) || isnan(scrubbingRect.origin.y))
        return;
    
    _summaryThumbnailWrapperView.frame = CGRectMake(MIN(0.0, scrubbingRect.origin.x), 0.0f, MAX(_wrapperView.frame.size.width, scrubbingRect.size.width), 40.0f);
    _zoomedThumbnailWrapperView.frame = _summaryThumbnailWrapperView.frame;
    
    [self _updateScrubberAnimationsAndResetCurrentPosition:true];
}

+ (NSString *)_stringFromTotalSeconds:(NSInteger)totalSeconds
{
    NSInteger hours = (NSInteger)totalSeconds / 3600;
    NSInteger minutes = (NSInteger)(totalSeconds / 60) % 60;
    NSInteger seconds = (NSInteger)(totalSeconds % 60);
    
    if (hours > 0)
        return [NSString stringWithFormat:@"%02d:%02d:%02d", (int)hours, (int)minutes, (int)seconds];
    else
        return [NSString stringWithFormat:@"%d:%02d", (int)minutes, (int)seconds];
}

- (void)zoenSuspend
{
    [self cancelZoomIn];
    [NSObject cancelPreviousPerformRequestsWithTarget:self];
    [self removeHandleAnimation];
    [self ignoreThumbnails];
}
