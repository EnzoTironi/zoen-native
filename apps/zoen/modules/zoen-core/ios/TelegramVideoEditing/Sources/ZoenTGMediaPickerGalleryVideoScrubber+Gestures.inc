// Adapted from Telegram-iOS at f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838; GPL-2.0-or-later. See NOTICE.md.

- (bool)isScrubbing {
    return _scrubbing;
}

- (void)handlePress:(UILongPressGestureRecognizer *)gestureRecognizer
{
    switch (gestureRecognizer.state)
    {
        case UIGestureRecognizerStateBegan:
        {
            if (_beganInteraction)
                return;
            
            _scrubbing = true;
            
            id<ZoenTGMediaPickerGalleryVideoScrubberDelegate> delegate = self.delegate;
            if ([delegate respondsToSelector:@selector(videoScrubberDidBeginScrubbing:)])
                [delegate videoScrubberDidBeginScrubbing:self];
            
            [self cancelZoomIn];
            if ([self zoomAvailable])
            {
                _pivotSource = ZoenTGMediaPickerGalleryVideoScrubberPivotSourceHandle;
                [self performSelector:@selector(zoomIn) withObject:nil afterDelay:TGVideoScrubberZoomActivationInterval];
            }
            
            _endedInteraction = false;
            _beganInteraction = true;
        }
            break;
            
        case UIGestureRecognizerStateEnded:
        case UIGestureRecognizerStateCancelled:
        {
            _beganInteraction = false;
            
            if (_endedInteraction)
                return;
            
            _scrubbing = false;
            
            id<ZoenTGMediaPickerGalleryVideoScrubberDelegate> delegate = self.delegate;
            if ([delegate respondsToSelector:@selector(videoScrubberDidEndScrubbing:)])
                [delegate videoScrubberDidEndScrubbing:self];
            
            [self cancelZoomIn];
            if (_zoomedIn)
                [self zoomOut];
            
            _endedInteraction = true;
        }
            break;
            
        default:
            break;
    }
}

- (void)handlePan:(UIPanGestureRecognizer *)gestureRecognizer
{
    CGPoint translation = [gestureRecognizer translationInView:self];
    [gestureRecognizer setTranslation:CGPointZero inView:self];
    
    UIView *handle = gestureRecognizer.view;
    
    switch (gestureRecognizer.state)
    {
        case UIGestureRecognizerStateBegan:
        {
            if (_beganInteraction)
                return;
            
            _scrubbing = true;
            
            [self removeHandleAnimation];
            
            id<ZoenTGMediaPickerGalleryVideoScrubberDelegate> delegate = self.delegate;
            if ([delegate respondsToSelector:@selector(videoScrubberDidBeginScrubbing:)])
                [delegate videoScrubberDidBeginScrubbing:self];
            
            [self cancelZoomIn];
            
            _endedInteraction = false;
            _beganInteraction = true;
        }
            break;
            
        case UIGestureRecognizerStateChanged:
        {
            if (_animatingZoomIn || _animatingZoomOut)
                return;
            
            CGRect scrubbingRect = [self _scrubbingRect];
            CGRect normalScrubbingRect = [self _scrubbingRectZoomedIn:false];
            CGFloat minPosition = scrubbingRect.origin.x + handle.frame.size.width / 2;
            CGFloat maxPosition = scrubbingRect.origin.x + scrubbingRect.size.width - handle.frame.size.width / 2;
            if (self.allowsTrimming)
            {
                minPosition = MAX(minPosition, _trimView.frame.origin.x + normalScrubbingRect.origin.x + handle.frame.size.width / 2);
                maxPosition = MIN(maxPosition, CGRectGetMaxX(_trimView.frame) - normalScrubbingRect.origin.x - handle.frame.size.width / 2);
            }
            
            handle.center = CGPointMake(MIN(MAX(handle.center.x + translation.x, minPosition), maxPosition), handle.center.y);
            
            NSTimeInterval position = [self _positionForScrubberPosition:handle.center duration:_duration];
            
            if (self.allowsTrimming)
            {
                if (ABS(handle.center.x - minPosition) < FLT_EPSILON)
                    position = _trimStartValue;
                else if (ABS(handle.center.x - maxPosition) < FLT_EPSILON)
                    position = _trimEndValue;
            }
            
            _value = position;
            [self _updateTimeLabels];
            
            id<ZoenTGMediaPickerGalleryVideoScrubberDelegate> delegate = self.delegate;
            if ([delegate respondsToSelector:@selector(videoScrubber:valueDidChange:)])
                [delegate videoScrubber:self valueDidChange:position];
            
            [self cancelZoomIn];
            if ([self zoomAvailable])
            {
                _pivotSource = ZoenTGMediaPickerGalleryVideoScrubberPivotSourceHandle;
                [self performSelector:@selector(zoomIn) withObject:nil afterDelay:TGVideoScrubberZoomActivationInterval];
            }
        }
            break;
            
        case UIGestureRecognizerStateEnded:
        case UIGestureRecognizerStateCancelled:
        {
            _beganInteraction = false;
            
            if (_endedInteraction)
                return;
            
            _scrubbing = false;
                        
            id<ZoenTGMediaPickerGalleryVideoScrubberDelegate> delegate = self.delegate;
            if ([delegate respondsToSelector:@selector(videoScrubberDidEndScrubbing:)])
                [delegate videoScrubberDidEndScrubbing:self];
            
            [self cancelZoomIn];
            if (_zoomedIn)
                [self zoomOut];
            
            _endedInteraction = true;
        }
            break;
            
        default:
            break;
    }
}

- (void)handleTap:(UITapGestureRecognizer *)gestureRecognizer
{
    CGPoint location = [gestureRecognizer locationInView:_dotHandle.superview];
    
    CGFloat position = MAX(_trimStartValue, MIN(_trimEndValue, [self _positionForScrubberPosition:location duration:_duration]));
    _value = position;

    CGPoint center = [self _dotPositionForPosition:position duration:_duration];
    [UIView animateWithDuration:0.2 delay:0.0 usingSpringWithDamping:1.1 initialSpringVelocity:0.0 options:kNilOptions animations:^{
        _dotHandle.center = CGPointMake(center.x, _dotHandle.center.y);
    } completion:^(BOOL finished) {
    }];
    
    id<ZoenTGMediaPickerGalleryVideoScrubberDelegate> delegate = self.delegate;
    if ([delegate respondsToSelector:@selector(videoScrubberDidBeginScrubbing:)])
        [delegate videoScrubberDidBeginScrubbing:self];
    
    if ([delegate respondsToSelector:@selector(videoScrubber:valueDidChange:)])
        [delegate videoScrubber:self valueDidChange:position];
    
    if ([delegate respondsToSelector:@selector(videoScrubberDidEndScrubbing:)])
        [delegate videoScrubberDidEndScrubbing:self];
}
