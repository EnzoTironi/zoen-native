// Adapted from TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// GPL-2.0-or-later; GPL-3.0 selected. See NOTICE.md and UPSTREAM_SOURCES.json.
import Foundation
import UIKit

extension LiquidLensView {
    private func updateLens(params: LensParams, transition: ComponentTransition) {
        guard let lensView = self.lensView else {
            return
        }

        if self.isApplyingLensParams {
            self.pendingLensParams = params
            return
        }
        self.isApplyingLensParams = true
        let previousParams = self.appliedLensParams
        self.appliedLensParams = params

        if previousParams?.isLifted != params.isLifted {
            self.isAnimating = true
            
            let selector = NSSelectorFromString("setLifted:animated:alongsideAnimations:completion:")
            var shouldScheduleUpdate = false
            var didProcessUpdate = false
            self.pendingLensParams = params
            if let lensView = self.lensView, let method = lensView.method(for: selector) {
                typealias ObjCMethod = @convention(c) (AnyObject, Selector, Bool, Bool, @escaping () -> Void, (() -> Void)?) -> Void
                let function = unsafeBitCast(method, to: ObjCMethod.self)
                function(lensView, selector, params.isLifted, !transition.animation.isImmediate, { [weak self] in
                    guard let self else {
                        return
                    }
                    let liftedInset: CGFloat = params.isLifted ? params.liftedInset : (-params.inset)
                    lensView.bounds = CGRect(origin: CGPoint(), size: CGSize(width: params.baseFrame.width + liftedInset * 2.0, height: params.baseFrame.height + liftedInset * 2.0))
                    didProcessUpdate = true
                    if shouldScheduleUpdate {
                        DispatchQueue.main.async { [weak self] in
                            guard let self, let pendingLensParams = self.pendingLensParams else {
                                return
                            }
                            self.isApplyingLensParams = false
                            self.pendingLensParams = nil
                            self.updateLens(params: pendingLensParams, transition: transition)
                        }
                    }
                }, { [weak self] in
                    guard let self else {
                        return
                    }
                    if !self.isApplyingLensParams {
                        self.isAnimating = false
                    }
                    self.isLiftedAnimationCompleted?()
                })
            }
            if didProcessUpdate {
                transition.animateView {
                    lensView.center = CGPoint(x: params.baseFrame.midX, y: params.baseFrame.midY)
                }
                self.pendingLensParams = nil
                self.isApplyingLensParams = false
            } else {
                shouldScheduleUpdate = true
            }
        } else {
            let liftedInset: CGFloat = params.isLifted ? params.liftedInset : (-params.inset)
            let lensBounds = CGRect(origin: CGPoint(), size: CGSize(width: params.baseFrame.width + liftedInset * 2.0, height: params.baseFrame.height + liftedInset * 2.0))
            let lensCenter = CGPoint(x: params.baseFrame.midX, y: params.baseFrame.midY)
            
            let previousBounds: CGRect = lensView.bounds
            transition.animateView {
                lensView.bounds = lensBounds
            }
            
            if let info = transition.userData(TransitionInfo.self), info.disableAnimationWorkarounds {
            } else {
                lensView.layer.removeAllAnimations()
                lensView.bounds = lensBounds
            }
            
            if !transition.animation.isImmediate {
                self.isAnimating = true
            }
            transition.setPosition(view: lensView, position: lensCenter, completion: { [weak self] flag in
                guard let self, flag else {
                    return
                }
                if !self.isApplyingLensParams {
                    self.isAnimating = false
                }
            })
            // No idea why
            transition.animatePosition(layer: lensView.layer, from: CGPoint(x: (lensBounds.width - previousBounds.width) * 0.5, y: 0.0), to: CGPoint(), additive: true)
            
            self.isApplyingLensParams = false
        }
    }

    private func updateLiftedLensPosition() {
        // Without this, the lens won't update its bouncing animations unless it's being moved
        if self.isApplyingLensParams {
            return
        }
        guard let lensView = self.lensView else {
            return
        }
        guard let params = self.appliedLensParams else {
            return
        }
        lensView.center = CGPoint(x: params.baseFrame.midX, y: params.baseFrame.midY)
    }

    func update(params: Params, transition: ComponentTransition) {
        let isFirstTime = self.params == nil
        let transition: ComponentTransition = isFirstTime ? .immediate : transition

        self.params = params

        transition.setFrame(view: self.containerView, frame: CGRect(origin: CGPoint(), size: params.size))

        if let backgroundContainer = self.backgroundContainer {
            transition.setFrame(view: backgroundContainer, frame: CGRect(origin: CGPoint(), size: params.size))
            backgroundContainer.update(size: params.size, isDark: params.isDark, transition: transition)
        } else if let genericBackgroundContainer = self.genericBackgroundContainer {
            transition.setFrame(view: genericBackgroundContainer, frame: CGRect(origin: CGPoint(), size: params.size))
        }
        
        if let backgroundView = self.backgroundView {
            transition.setFrame(view: backgroundView, frame: CGRect(origin: CGPoint(), size: params.size))
            backgroundView.update(size: params.size, cornerRadius: params.cornerRadius ?? (params.size.height * 0.5), isDark: params.isDark, tintColor: GlassBackgroundView.TintColor.init(kind: .panel), isInteractive: true, transition: transition)
        }
        
        if self.contentView.bounds.size != params.size {
            self.contentView.clipsToBounds = true
            transition.setFrame(view: self.contentView, frame: CGRect(origin: CGPoint(), size: params.size), completion: { [weak self] completed in
                guard let self, completed else {
                    return
                }
                self.contentView.clipsToBounds = false
            })
            transition.setCornerRadius(layer: self.contentView.layer, cornerRadius: params.cornerRadius ?? (params.size.height * 0.5))

            self.liftedContainerView.clipsToBounds = true
            transition.setFrame(view: self.liftedContainerView, frame: CGRect(origin: CGPoint(), size: params.size), completion: { [weak self] completed in
                guard let self, completed else {
                    return
                }
                self.liftedContainerView.clipsToBounds = false
            })
            transition.setCornerRadius(layer: self.liftedContainerView.layer, cornerRadius: params.cornerRadius ?? (params.size.height * 0.5))
        }

        
        let baseLensFrame = CGRect(origin: params.selectionOrigin, size: params.selectionSize)
        self.updateLens(params: LensParams(baseFrame: baseLensFrame, inset: params.inset, liftedInset: params.liftedInset, isLifted: params.isLifted), transition: transition)
        
        if let legacyContentMaskView = self.legacyContentMaskView {
            transition.setFrame(view: legacyContentMaskView, frame: CGRect(origin: CGPoint(), size: params.size))
        }
        if let legacyContentMaskBlobView = self.legacyContentMaskBlobView, let legacyLiftedContentBlobMaskView = self.legacyLiftedContentBlobMaskView, let legacySelectionView = self.legacySelectionView {
            let lensFrame = baseLensFrame.insetBy(dx: params.inset, dy: params.inset)
            let effectiveLensFrame = lensFrame.insetBy(dx: params.isLifted ? -2.0 : 0.0, dy: params.isLifted ? -2.0 : 0.0)
            
            if legacyContentMaskBlobView.image?.size.height != lensFrame.height {
                legacyContentMaskBlobView.image = generateStretchableFilledCircleImage(diameter: lensFrame.height, color: .black)
                legacyLiftedContentBlobMaskView.image = legacyContentMaskBlobView.image
                legacySelectionView.image = generateStretchableFilledCircleImage(diameter: lensFrame.height, color: .white)?.withRenderingMode(.alwaysTemplate)
            }
            transition.setFrame(view: legacyContentMaskBlobView, frame: effectiveLensFrame)
            transition.setFrame(view: legacyLiftedContentBlobMaskView, frame: effectiveLensFrame)
            
            legacySelectionView.tintColor = UIColor(white: params.isDark ? 1.0 : 0.0, alpha: params.isDark ? 0.1 : 0.075)
            transition.setFrame(view: legacySelectionView, frame: effectiveLensFrame)
        }

        transition.setFrame(view: self.restingBackgroundView, frame: CGRect(origin: CGPoint(), size: params.size))
        self.restingBackgroundView.update(isDark: params.isDark)
        transition.setAlpha(view: self.restingBackgroundView, alpha: (params.isLifted || params.isCollapsed) ? 0.0 : 1.0)

        if params.isLifted {
            if self.liftedDisplayLink == nil {
                self.liftedDisplayLink = SharedDisplayLinkDriver.shared.add(framesPerSecond: .max, { [weak self] _ in
                    guard let self else {
                        return
                    }
                    self.updateLiftedLensPosition()
                })
            }
        } else if let liftedDisplayLink = self.liftedDisplayLink {
            self.liftedDisplayLink = nil
            liftedDisplayLink.invalidate()
        }
    }
}
