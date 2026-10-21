// SPDX-License-Identifier: GPL-2.0-or-later
// Telegram-iOS f1dd7a2: TGMediaPickerGalleryVideoItemView:220-247 and
// TGMediaPickerGalleryInterfaceView:2060-2071,2158,2173,2188,2206.
import UIKit

/// Only source geometry. The standalone editor keeps its top safe-area anchor;
/// the original gallery places this same header above its caption/toolbar.
struct ZoenTelegramVideoEditorLayout {
  let header: CGRect
  let headerSafeArea: UIEdgeInsets
  let toolbar: CGRect
  let cancel: CGRect
  let commit: CGRect

  static func make(bounds: CGRect, safeArea: UIEdgeInsets, landscapeLeft: Bool) -> Self {
    var local = safeArea
    if bounds.width > bounds.height {
      let toolbarWidth: CGFloat = 44 + (landscapeLeft ? safeArea.left : safeArea.right)
      let toolbarX: CGFloat = landscapeLeft ? 0 : bounds.width - toolbarWidth
      let controlX: CGFloat = landscapeLeft ? toolbarWidth - 44 : 0
      // The side toolbar has already consumed this inset. The source clears it
      // before passing localSafeAreaInset to the header/scrubber a second time.
      if landscapeLeft { local.left = 0 } else { local.right = 0 }
      return Self(
        header: CGRect(x: landscapeLeft ? toolbarWidth : 0, y: safeArea.top,
                       width: max(0, bounds.width - toolbarWidth), height: 64),
        headerSafeArea: local,
        toolbar: CGRect(x: toolbarX, y: 0, width: toolbarWidth, height: bounds.height),
        cancel: CGRect(x: controlX, y: bounds.height - 44, width: 44, height: 44),
        commit: CGRect(x: controlX, y: 0, width: 44, height: 44))
    }
    return Self(
      header: CGRect(x: 0, y: safeArea.top, width: bounds.width, height: 72),
      headerSafeArea: local,
      toolbar: CGRect(x: 0, y: bounds.height - 44 - safeArea.bottom,
                      width: bounds.width, height: 44 + safeArea.bottom),
      cancel: CGRect(x: 26, y: 0, width: 44, height: 44),
      commit: CGRect(x: bounds.width - 44 - 26, y: 0, width: 44, height: 44))
  }

  static func makeHeader(scrubber: ZoenTGMediaPickerGalleryVideoScrubber) -> ZoenTGMediaPickerScrubberHeaderView {
    // 44 is the initial source frame only. Final gallery header bounds are72
    // in portrait /64 in landscape, while this scrubber's y stays0 throughout.
    let header = ZoenTGMediaPickerScrubberHeaderView(frame: CGRect(x: 0, y: 0, width: 320, height: 44))
    header.autoresizingMask = [.flexibleWidth]
    let panel = UIView(frame: CGRect(x: 0, y: 0, width: 320, height: 64))
    panel.autoresizingMask = [.flexibleWidth]
    header.panelView = panel; header.addSubview(panel)
    scrubber.frame = CGRect(x: 0, y: 0, width: 320, height: 68)
    scrubber.autoresizingMask = [.flexibleWidth]
    header.scrubberView = scrubber; panel.addSubview(scrubber)
    return header
  }
}
