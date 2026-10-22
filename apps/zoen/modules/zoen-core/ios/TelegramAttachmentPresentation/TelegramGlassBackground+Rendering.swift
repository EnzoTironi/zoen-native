// Adapted from TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// GPL-2.0-or-later; GPL-3.0 selected. See NOTICE.md and UPSTREAM_SOURCES.json.
import Foundation
import UIKit

extension GlassBackgroundView {
    public func update(size: CGSize, cornerRadius: CGFloat, isDark: Bool, tintColor: TintColor, isInteractive: Bool = false, isVisible: Bool = true, transition: ComponentTransition) {
        let shape: Shape = .roundedRect(cornerRadius: cornerRadius)
        self.update(size: size, shape: shape, isDark: isDark, tintColor: tintColor, isInteractive: isInteractive, isVisible: isVisible, transition: transition)
    }

    public func update(size: CGSize, cornerRadii: CornerRadii, isDark: Bool, tintColor: TintColor, isInteractive: Bool = false, isVisible: Bool = true, transition: ComponentTransition) {
        let shape: Shape = .customRoundedRect(cornerRadii: cornerRadii)
        self.update(size: size, shape: shape, isDark: isDark, tintColor: tintColor, isInteractive: isInteractive, isVisible: isVisible, transition: transition)
    }

    func update(size: CGSize, shape: Shape, isDark: Bool, tintColor: TintColor, isInteractive: Bool = false, isVisible: Bool = true, transition: ComponentTransition) {
        
        if let glassHighlightRecognizer = self.glassHighlightRecognizer {
            glassHighlightRecognizer.isEnabled = isInteractive
        }
        
        if let nativeView = self.nativeView, let nativeViewClippingContext = self.nativeViewClippingContext, (nativeView.bounds.size != size || nativeViewClippingContext.shape != shape || (nativeView.overrideUserInterfaceStyle == .dark) != isDark) {
            nativeViewClippingContext.update(shape: shape, size: size, transition: transition)
            if transition.animation.isImmediate {
                nativeView.frame = CGRect(origin: CGPoint(), size: size)
            } else {
                let nativeFrame = CGRect(origin: CGPoint(), size: size)
                transition.animateView {
                    nativeView.frame = nativeFrame
                }
            }
            nativeView.overrideUserInterfaceStyle = isDark ? .dark : .light
        }
        if let legacyView = self.legacyView {
            let style: LegacyGlassView.Style
            switch tintColor.kind {
            case .panel:
                style = .normal
            case .clear:
                style = .clear
            case let .custom(styleValue, _):
                switch styleValue {
                case .clear:
                    style = .clear
                case .default:
                    style = .normal
                }
            }
            legacyView.update(size: size, shape: shape, style: style, transition: transition)
            transition.setFrame(view: legacyView, frame: CGRect(origin: CGPoint(), size: size))
            transition.setAlpha(view: legacyView, alpha: isVisible ? 1.0 : 0.0)
            
            transition.setPosition(view: self.contentView, position: CGPoint(x: size.width * 0.5, y: size.height * 0.5))
            transition.setBounds(view: self.contentView, bounds: CGRect(origin: CGPoint(), size: size))
        }
        if let legacyHighlightContainerView = self.legacyHighlightContainerView {
            transition.setFrame(view: legacyHighlightContainerView, frame: CGRect(origin: CGPoint(), size: size))
            self.legacyHighlightClippingContext?.update(shape: shape, size: size, transition: transition)
        }
        
        let shadowInset: CGFloat = 32.0
        
        if let innerColor = tintColor.innerColor {
            let innerBackgroundFrame = CGRect(origin: CGPoint(), size: size).insetBy(dx: tintColor.innerInset, dy: tintColor.innerInset)
            let innerBackgroundRadius = min(innerBackgroundFrame.width, innerBackgroundFrame.height) * 0.5
            
            let innerBackgroundView: UIView
            var innerBackgroundTransition = transition
            var animateIn = false
            if let current = self.innerBackgroundView {
                innerBackgroundView = current
            } else {
                innerBackgroundView = UIView()
                innerBackgroundTransition = innerBackgroundTransition.withAnimation(.none)
                self.innerBackgroundView = innerBackgroundView
                self.contentView.insertSubview(innerBackgroundView, at: 0)
                
                innerBackgroundView.frame = innerBackgroundFrame
                innerBackgroundView.layer.cornerRadius = innerBackgroundRadius
                animateIn = true
            }
            
            innerBackgroundView.backgroundColor = innerColor
            innerBackgroundTransition.setFrame(view: innerBackgroundView, frame: innerBackgroundFrame)
            innerBackgroundTransition.setCornerRadius(layer: innerBackgroundView.layer, cornerRadius: innerBackgroundRadius)
            
            if animateIn {
                transition.animateAlpha(view: innerBackgroundView, from: 0.0, to: 1.0)
                transition.animateScale(view: innerBackgroundView, from: 0.001, to: 1.0)
            }
        } else if let innerBackgroundView = self.innerBackgroundView {
            self.innerBackgroundView = nil
            
            transition.setAlpha(view: innerBackgroundView, alpha: 0.0, completion: { [weak innerBackgroundView] _ in
                innerBackgroundView?.removeFromSuperview()
            })
            transition.setScale(view: innerBackgroundView, scale: 0.001)
            
            innerBackgroundView.removeFromSuperview()
        }
        
        let params = Params(shape: shape, isDark: isDark, tintColor: tintColor, isInteractive: isInteractive, isVisible: isVisible)
        if self.params != params {
            self.params = params
            
            if let shadowView = self.shadowView {
                switch shape {
                case let .roundedRect(cornerRadius):
                    shadowView.image = Self.generateLegacyShadowImage(cornerRadius: cornerRadius, shadowInset: shadowInset)
                case let .customRoundedRect(cornerRadii):
                    shadowView.image = Self.generateLegacyShadowImage(cornerRadii: GlassBackgroundView.clampedCornerRadii(size: size, cornerRadii: cornerRadii), shadowInset: shadowInset)
                }
                transition.setAlpha(view: shadowView, alpha: isVisible ? 1.0 : 0.0)
            }
            
            if let foregroundView = self.foregroundView {
                let fillColor: UIColor
                let borderWidthFactor: CGFloat
                switch tintColor.kind {
                case .panel:
                    borderWidthFactor = 1.0
                    if isDark {
                        fillColor = UIColor(white: 1.0, alpha: 1.0).mixedWith(.black, alpha: 1.0 - 0.11).withAlphaComponent(0.85)
                    } else {
                        fillColor = UIColor(white: 1.0, alpha: 0.7)
                    }
                case .clear:
                    borderWidthFactor = 2.0
                    fillColor = UIColor(white: 1.0, alpha: 0.0)
                case let .custom(style, color):
                    fillColor = color
                    switch style {
                    case .clear:
                        borderWidthFactor = 2.0
                    case .default:
                        borderWidthFactor = 1.0
                    }
                }
                switch shape {
                case let .roundedRect(cornerRadius):
                    foregroundView.image = GlassBackgroundView.generateLegacyGlassImage(size: CGSize(width: cornerRadius * 2.0, height: cornerRadius * 2.0), inset: shadowInset, borderWidthFactor: borderWidthFactor, isDark: isDark, fillColor: fillColor)
                case let .customRoundedRect(cornerRadii):
                    foregroundView.image = GlassBackgroundView.generateLegacyGlassImage(cornerRadii: GlassBackgroundView.clampedCornerRadii(size: size, cornerRadii: cornerRadii), inset: shadowInset, borderWidthFactor: borderWidthFactor, isDark: isDark, fillColor: fillColor)
                }
                #if DEBUG
                //foregroundView.image = nil
                #endif
                transition.setAlpha(view: foregroundView, alpha: isVisible ? 1.0 : 0.0)
            } else {
                if let nativeParamsView = self.nativeParamsView, let nativeView = self.nativeView {
                    if #available(iOS 26.0, *) {
                        var glassEffect: UIGlassEffect?
                        
                        if isVisible {
                            let glassEffectValue: UIGlassEffect
                            switch tintColor.kind {
                            case .panel:
                                if isDark {
                                    glassEffectValue = UIGlassEffect(style: .regular)
                                    glassEffectValue.tintColor = UIColor(white: 1.0, alpha: 0.025)
                                } else {
                                    glassEffectValue = UIGlassEffect(style: .regular)
                                    glassEffectValue.tintColor = UIColor(white: 1.0, alpha: 0.1)
                                }
                            case let .custom(style, color):
                                switch style {
                                case .default:
                                    glassEffectValue = UIGlassEffect(style: .regular)
                                    glassEffectValue.tintColor = color
                                case .clear:
                                    glassEffectValue = UIGlassEffect(style: .clear)
                                    glassEffectValue.tintColor = color
                                }
                            case .clear:
                                glassEffectValue = UIGlassEffect(style: .clear)
                                if isDark {
                                    glassEffectValue.tintColor = UIColor(white: 0.0, alpha: 0.28)
                                } else {
                                    glassEffectValue.tintColor = nil
                                }
                            }
                            glassEffectValue.isInteractive = isInteractive
                            glassEffect = glassEffectValue
                        }
                        
                        if glassEffect == nil {
                            if nativeView.effect is UIGlassEffect {
                                if #available(iOS 26.1, *) {
                                    if transition.animation.isImmediate {
                                        nativeView.effect = nil
                                    } else {
                                        transition.animateView {
                                            nativeView.effect = nil
                                        }
                                    }
                                } else {
                                    if transition.animation.isImmediate {
                                        nativeView.effect = UIVisualEffect()
                                    } else {
                                        transition.animateView {
                                            nativeView.effect = UIVisualEffect()
                                        }
                                    }
                                }
                            }
                        } else {
                            if transition.animation.isImmediate {
                                nativeView.effect = glassEffect
                            } else {
                                if let glassEffect, let currentEffect = nativeView.effect as? UIGlassEffect, currentEffect.tintColor == glassEffect.tintColor, currentEffect.isInteractive == glassEffect.isInteractive {
                                } else {
                                    transition.animateView {
                                        nativeView.effect = glassEffect
                                    }
                                }
                            }
                        }
                        
                        if isDark {
                            nativeParamsView.lumaMin = 0.0
                            nativeParamsView.lumaMax = 0.15
                        } else {
                            nativeParamsView.lumaMin = 0.8
                            nativeParamsView.lumaMax = 0.801
                        }
                    }
                }
            }
        }
        
        if let nativeParamsView = self.nativeParamsView {
            transition.setFrame(view: nativeParamsView, frame: CGRect(origin: CGPoint(x: 0.0, y: 0.0), size: size))
        }
        transition.setFrame(view: self.maskContainerView, frame: CGRect(origin: CGPoint(), size: CGSize(width: size.width + shadowInset * 2.0, height: size.height + shadowInset * 2.0)))
        transition.setFrame(view: self.maskContentView, frame: CGRect(origin: CGPoint(x: shadowInset, y: shadowInset), size: size))
        if let foregroundView = self.foregroundView {
            transition.setFrame(view: foregroundView, frame: CGRect(origin: CGPoint(), size: size).insetBy(dx: -shadowInset, dy: -shadowInset))
        }
        if let shadowView = self.shadowView {
            transition.setFrame(view: shadowView, frame: CGRect(origin: CGPoint(), size: size).insetBy(dx: -shadowInset, dy: -shadowInset))
        }
        transition.setFrame(view: self.contentContainer, frame: CGRect(origin: CGPoint(), size: size))
    }
    
}
