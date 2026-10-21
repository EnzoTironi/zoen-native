// SPDX-License-Identifier: GPL-2.0-or-later
// Presentation-only Expo hosts for the original Telegram-iOS glass at f1dd7a2.
// ChatTextInputPanelNode:1614-1622,3131,3538,3873; upstream rendering is in
// TelegramAttachmentPresentation/TelegramGlassBackground.swift and its manifest.
import ExpoModulesCore
import UIKit

/// Fabric retains the React children and their layout. Only the native content
/// parent changes; no text clone, responder transfer, child-frame write or portal.
@MainActor
class ZoenComposerGlassHost: ExpoView {
  var contentHost: UIView { fatalError("subclass must supply its original content view") }
  private(set) var children: [UIView] = []

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    clipsToBounds = false
  }
  override func mountChildComponentView(_ childComponentView: UIView, index: Int) {
    guard index >= 0, index <= children.count, childComponentView.superview == nil else { return }
    children.insert(childComponentView, at: index)
    contentHost.insertSubview(childComponentView, at: index)
  }
  override func unmountChildComponentView(_ childComponentView: UIView, index: Int) {
    guard children.indices.contains(index), children[index] === childComponentView,
          childComponentView.superview === contentHost else { return }
    children.remove(at: index)
    childComponentView.removeFromSuperview()
  }
}

@MainActor
final class ZoenComposerGlassSurface: ZoenComposerGlassHost {
  let glass = ZoenTelegramAttachmentGlassView(frame: .zero)
  override var contentHost: UIView { glass.contentView }
  var isDark = false
  var cornerRadius: Double = 20
  var interactive = true
  var preference = "panel"

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    addSubview(glass)
  }
  func apply() {
    let radius = cornerRadius.isFinite && cornerRadius >= 0 ? CGFloat(cornerRadius) : 20
    glass.update(isDark: isDark, cornerRadius: radius, isInteractive: interactive,
                 tintKind: preference == "clear" ? .clear : .panel)
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    glass.frame = bounds
    apply()
    glass.layoutIfNeeded()
  }
}

@MainActor
final class ZoenComposerGlassContainer: ZoenComposerGlassHost {
  // ChatTextInputPanelNode constructs GlassBackgroundContainerView with its7pt default.
  private let glass = ZoenTelegramAttachmentGlassContainerView(spacing: 7)
  override var contentHost: UIView { glass.contentView }
  var isDark = false

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    addSubview(glass)
  }
  func apply() { glass.update(isDark: isDark) }
  override func layoutSubviews() {
    super.layoutSubviews()
    glass.frame = bounds
    apply()
    glass.layoutIfNeeded()
  }
}
