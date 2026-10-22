import UIKit

import UIKit

// Adapted from TelegramMessenger/Telegram-iOS, GPL-2.0-or-later (GPLv3 selected),
// revision f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838:
// AttachmentUI/Sources/AttachmentContainer.swift:16–28, 247–477, 504–665.
// Fixed viewport/wrapper/clip transforms and additional bottom inset remain
// distinct, as in the source. DeviceMetrics.swift:58–70, 118–213, 231–315
// supplies the original screen profile recognition and corner measurements.
// Display/Source/ContainedViewLayoutTransition.swift:41–83 and
// Display/Source/CAAnimationUtils.swift:66–110, 173–205 provide the actual
// custom spring and fallback entrance timing, without private duration APIs.
// Caption focus: AttachmentController.swift:762–766 and
// UIKitRuntimeUtils/Source/UIKitRuntimeUtils/UIKitUtils.m:53–83 (iOS 26 spring).
// Flat gallery geometry/restoration: Display/Source/Navigation/
// NavigationModalContainer.swift:384–430, 494–500, 503–543.
// UIKit hosts replace ASDisplayNode/NavigationContainer; Zoen retains admission,
// Photos selection and durable media ownership. See THIRD_PARTY_NOTICES.md.
struct ZoenAttachmentContainerMetrics {
  enum Destination: Equatable { case expanded, collapsed, dismissed }
  struct Layout {
    let wrapping: CGRect
    let clip: CGRect
    let clipScale: CGFloat
    let bottomClip: CGRect
    let navigation: CGRect
    let additionalBottomInset: CGFloat
  }

  // AttachmentContainer.swift:530–665. The controller viewport stays full
  // height; the wrapper/clip and additional bottom inset expose the sheet.
  static func layout(size: CGSize, baseTop: CGFloat, offset: CGFloat, inset: CGFloat,
                     landscape: Bool, regular: Bool, fullScreen: Bool, scale: CGFloat) -> Layout {
    let rawOffset = fullScreen || regular ? 0 : max(0, offset)
    let topInset = landscape ? 0 : min(inset, rawOffset)
    let scaleProgress: CGFloat = fullScreen || regular ? 0 : (landscape ? 1 : (inset == 0 ? 0 : topInset / inset))
    let viewportHeight = max(0, size.height - baseTop)
    let clipTopOffset: CGFloat = fullScreen || regular || landscape ? 0 : 2 * scaleProgress + 1 / max(1, scale)
    let clip = CGRect(x: 0, y: baseTop + clipTopOffset, width: size.width, height: max(0, viewportHeight - topInset))
    return Layout(wrapping: CGRect(x: 0, y: rawOffset, width: size.width, height: size.height),
      clip: clip, clipScale: size.width > 0 ? 1 - 10 / size.width * scaleProgress : 1,
      bottomClip: CGRect(x: 0, y: -100, width: size.width, height: clip.height + 100),
      navigation: CGRect(x: 0, y: 100, width: size.width, height: viewportHeight),
      additionalBottomInset: topInset)
  }

  static func collapsedInset(size: CGSize, regular: Bool) -> CGFloat {
    guard size.width <= size.height else { return 0 }
    if regular { return 210 }
    return floor(max(size.width, size.height) * (size.width <= 320 ? 0.15 : 0.2488))
  }

  static func destination(expanded: Bool, inset: CGFloat, offset: CGFloat,
                          translation: CGFloat, velocity: CGFloat,
                          innerOffset: CGFloat) -> Destination {
    let velocity = expanded && innerOffset > 0.1 ? 0 : velocity
    let boundsY = -max(0, expanded ? translation - inset : translation)
    if boundsY < -60 || (boundsY < 0 && velocity > 300) || (expanded && boundsY == 0 && velocity > 1800) {
      return .dismissed
    }
    if expanded { return velocity > 300 || offset > inset / 2 ? .collapsed : .expanded }
    return velocity < -300 || offset < inset / 2 ? .expanded : .collapsed
  }
}

@MainActor
protocol ZoenAttachmentOverlayContent: ZoenKeyboardPanelContent {
  var attachmentScrollView: UIScrollView { get }
  var attachmentHasKeyboardFocus: Bool { get }
  var attachmentHandlesVerticalGesture: Bool { get }
  var attachmentPrefersFullScreen: Bool { get }
  var attachmentAnimatedViews: [UIView] { get }
  var attachmentDidBeginEditing: (() -> Void)? { get set }
  var attachmentPresentationChanged: (() -> Void)? { get set }
  func updatePresentation(expansion: CGFloat, keyboardHeight: CGFloat, safeArea: UIEdgeInsets)
  func updatePresentation(expansion: CGFloat, keyboardHeight: CGFloat, safeArea: UIEdgeInsets, additionalBottomInset: CGFloat)
  func retireForDismissal(completion: @escaping @MainActor () -> Void)
}

extension ZoenAttachmentOverlayContent {
  var attachmentHandlesVerticalGesture: Bool { false }
  var attachmentPrefersFullScreen: Bool { false }
  var attachmentPresentationChanged: (() -> Void)? { get { nil } set {} }
  func updatePresentation(expansion: CGFloat, keyboardHeight: CGFloat, safeArea: UIEdgeInsets, additionalBottomInset: CGFloat) {
    updatePresentation(expansion: expansion, keyboardHeight: keyboardHeight, safeArea: safeArea)
  }
}

/// Hide only preexisting presenter content while its admitted attachment child
/// is present. Overlapping owners restore each exact original value only after
/// the last removal; no composer, draft or later system-menu child is mutated.
