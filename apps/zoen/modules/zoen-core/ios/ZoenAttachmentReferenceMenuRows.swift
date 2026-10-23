import UIKit

@MainActor final class ReferenceActionRow: UIControl {
  private let title = UILabel()
  private let icon = UIImageView()
  init(action: ZoenAttachmentContextMenu.Action, appearance: ZoenKeyboardPanelAppearance) {
    super.init(frame: .zero)
    title.text = action.title; title.numberOfLines = 2
    title.lineBreakMode = .byTruncatingTail
    title.textColor = action.enabled ? appearance.text : appearance.secondary
    icon.image = action.image; icon.tintColor = title.textColor
    icon.contentMode = .center
    title.isUserInteractionEnabled = false; icon.isUserInteractionEnabled = false
    addSubview(title); addSubview(icon)
    isEnabled = action.enabled; isExclusiveTouch = true
    isAccessibilityElement = true; accessibilityLabel = action.title
    accessibilityTraits = action.enabled ? .button : [.button, .notEnabled]
    accessibilityIdentifier = "attachment-reference-action-\(action.id)"
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
  func measure(font: UIFont, constrainedWidth: CGFloat) -> CGSize {
    title.font = font
    let right = icon.image == nil ? 18 : max(32, icon.image?.size.width ?? 0) + 8
    return title.sizeThatFits(CGSize(width: max(1, constrainedWidth - 18 - right), height: .greatestFiniteMagnitude))
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    let iconSize = icon.image?.size ?? .zero
    title.frame = CGRect(x: 18, y: 11, width: max(0, bounds.width - 18 - (icon.image == nil ? 18 : max(32, iconSize.width) + 20 + 8)), height: max(0, bounds.height - 22))
    icon.frame = CGRect(x: bounds.width - 20 - 32 + floor((32 - iconSize.width) / 2),
      y: floor((bounds.height - iconSize.height) / 2), width: iconSize.width, height: iconSize.height)
  }
}

/// ContextControllerActionsStackNode.ItemSelectionRecognizer:1405–1467.
/// Begin immediately on touch, instead of a pan recognizer's distance threshold.
@MainActor final class ReferenceMenuSelectionRecognizer: UIGestureRecognizer {
  var shouldBegin: ((CGPoint) -> Bool)?
  override init(target: Any?, action: Selector?) {
    super.init(target: target, action: action)
    delaysTouchesBegan = false; delaysTouchesEnded = false
  }
  override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesBegan(touches, with: event)
    guard let location = touches.first?.location(in: view), shouldBegin?(location) != false else {
      state = .failed; return
    }
    state = .began
  }
  override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesMoved(touches, with: event); state = .changed
  }
  override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesEnded(touches, with: event); state = .ended
  }
  override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesCancelled(touches, with: event); state = .cancelled
  }
}
