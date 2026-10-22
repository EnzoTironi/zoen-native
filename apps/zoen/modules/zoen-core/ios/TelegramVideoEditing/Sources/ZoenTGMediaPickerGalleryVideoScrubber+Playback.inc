// Adapted from Telegram-iOS at f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838; GPL-2.0-or-later. See NOTICE.md.

- (void)setIsPlaying:(bool)isPlaying
{
    _isPlaying = isPlaying;
    
    if (_isPlaying)
        [self _updateScrubberAnimationsAndResetCurrentPosition:false];
    else
        [self removeHandleAnimation];
}

- (void)setValue:(NSTimeInterval)value
{
    [self setValue:value resetPosition:false];
}

- (void)setValue:(NSTimeInterval)value resetPosition:(bool)resetPosition
{
    if (_duration < FLT_EPSILON || _scrubbing)
        return;
    
    if (value > _duration)
        value = _duration;
    
    _value = value;
    
    [self _updateTimeLabels];
    if (resetPosition)
        [self _updateScrubberAnimationsAndResetCurrentPosition:true];
}

- (void)_updateScrubberAnimationsAndResetCurrentPosition:(bool)resetCurrentPosition
{
    [self _updateScrubberAnimationsAndResetCurrentPosition:resetCurrentPosition zoomedIn:_zoomedIn];
}

- (void)_updateScrubberAnimationsAndResetCurrentPosition:(bool)resetCurrentPosition zoomedIn:(bool)zoomedIn
{
    if (isnan(_duration) || _duration < FLT_EPSILON)
        return;

    CGPoint point = [self _scrubberPositionForPosition:_value duration:_duration zoomedIn:zoomedIn];
    CGRect frame = CGRectMake(floor(point.x) - _scrubberHandle.frame.size.width / 2, _scrubberHandle.frame.origin.y, _scrubberHandle.frame.size.width, _scrubberHandle.frame.size.height);
    
    if (_trimStartValue > DBL_EPSILON && fabs(_value - _trimStartValue) < 0.01)
    {
        frame = CGRectMake(_trimView.frame.origin.x + [self _scrubbingRectZoomedIn:zoomedIn].origin.x, _scrubberHandle.frame.origin.y, _scrubberHandle.frame.size.width, _scrubberHandle.frame.size.height);
    }
    else if (fabs(_value - _trimEndValue) < 0.01)
    {
        frame = CGRectMake(_trimView.frame.origin.x + _trimView.frame.size.width - [self _scrubbingRectZoomedIn:zoomedIn].origin.x - _scrubberHandle.frame.size.width, _scrubberHandle.frame.origin.y, _scrubberHandle.frame.size.width, _scrubberHandle.frame.size.height);
    }
    
    if (_isPlaying)
    {
        if (resetCurrentPosition)
            _scrubberHandle.frame = frame;
        
        CGRect scrubbingRect = [self _scrubbingRectZoomedIn:zoomedIn];
        CGFloat maxPosition = scrubbingRect.origin.x + scrubbingRect.size.width - _scrubberHandle.frame.size.width / 2;
        NSTimeInterval duration = _duration;
        NSTimeInterval value = _value;
        
        if (self.allowsTrimming)
        {
            maxPosition = MIN(maxPosition, CGRectGetMaxX(_trimView.frame) - scrubbingRect.origin.x - _scrubberHandle.frame.size.width / 2);
            duration = _trimEndValue - _trimStartValue;
            value = _value - _trimStartValue;
        }
        
        CGRect endFrame = CGRectMake(maxPosition - _scrubberHandle.frame.size.width / 2, frame.origin.y, _scrubberHandle.frame.size.width, _scrubberHandle.frame.size.height);
        
        [self addHandleAnimationFromFrame:_scrubberHandle.frame toFrame:endFrame duration:MAX(0.0, duration - value)];
    }
    else
    {
        [self removeHandleAnimation];
        _scrubberHandle.frame = frame;
    }
}

- (void)addHandleAnimationFromFrame:(CGRect)fromFrame toFrame:(CGRect)toFrame duration:(NSTimeInterval)duration
{
    [self removeHandleAnimation];
    
    if (_frameDriver == nil)
        _frameDriver = [[ZoenVideoLinearFrameDriver alloc] initWithView:_scrubberHandle];
    [_frameDriver startFromFrame:fromFrame toFrame:toFrame duration:duration];
}

- (void)removeHandleAnimation
{
    [_frameDriver stop];
}

- (void)resetToStart
{
    _value = _trimStartValue;
    [self _updateTimeLabels];
    
    [self removeHandleAnimation];
    _scrubberHandle.center = CGPointMake(_trimView.frame.origin.x + [self _scrubbingRect].origin.x + _scrubberHandle.frame.size.width / 2, _scrubberHandle.center.y);
}

- (void)_updateTimeLabels
{
    _currentTimeLabel.text = self.disableTimeDisplay ? @"" : [ZoenTGMediaPickerGalleryVideoScrubber _stringFromTotalSeconds:(NSInteger)self.value];
    
    _inverseTimeLabel.text = self.disableTimeDisplay ? @"" : [ZoenTGMediaPickerGalleryVideoScrubber _stringFromTotalSeconds:(NSInteger)self.duration];
}

#pragma mark - Scrubber Handle

- (BOOL)gestureRecognizer:(UIGestureRecognizer *)gestureRecognizer shouldRecognizeSimultaneouslyWithGestureRecognizer:(UIGestureRecognizer *)otherGestureRecognizer
{
    if (gestureRecognizer.view != otherGestureRecognizer.view)
        return false;
    
    return true;
}
