import AVFoundation
import Photos
import UIKit

@MainActor extension ZoenAttachmentKeyboardContent {
  func updatePresentation(expansion: CGFloat, keyboardHeight: CGFloat, safeArea: UIEdgeInsets) {
    updatePresentation(expansion: expansion, keyboardHeight: keyboardHeight, safeArea: safeArea, additionalBottomInset: 0)
  }
  func updatePresentation(expansion: CGFloat, keyboardHeight: CGFloat, safeArea: UIEdgeInsets, additionalBottomInset: CGFloat) {
    self.expansion = expansion; self.keyboardHeight = keyboardHeight; self.safeArea = safeArea
    self.additionalBottomInset = additionalBottomInset
    if galleryVisible {
      var gallerySafeArea = safeArea
      gallerySafeArea.bottom += additionalBottomInset
      gallery.updatePresentation(keyboardHeight: keyboardHeight, safeArea: gallerySafeArea)
    }
    reflow()
    referenceMenu?.invalidateIfNeeded()
  }
  func updateSelectionPresentation() {
    let active = selecting && captionEnabled
    captionPanel.alpha = active ? 1 : 0
    captionPanel.transform = active ? .identity : CGAffineTransform(scaleX: 0.85, y: 0.85)
    footer.alpha = active ? 0 : 1
    footer.transform = active ? CGAffineTransform(scaleX: 0.85, y: 0.85) : .identity
  }
  func reflow() {
    let width = root.bounds.width, height = root.bounds.height
    guard width > 0, height > 0 else { return }
    gallery.view.frame = root.bounds
    gallery.view.isHidden = !galleryVisible
    if galleryVisible {
      for view in [grid, fileLanding.view, header, footer, captionPanel, manage, message, settings, retry, activity] { view.isHidden = true }
      albumMenu.isHidden = true
      cameraPreview.setRunning(false)
      return
    }
    header.isHidden = false
    albumMenu.frame = root.bounds
    albumMenu.isHidden = screen != "albums"
    for view in [grid, header, footer, captionPanel, manage, message, settings, retry] {
      view.accessibilityElementsHidden = screen == "albums"
    }
    let landscape = width > (root.window?.bounds.height ?? height)
    layout.columns = landscape ? 5 : 3
    // AttachmentController marks every child modal; ViewController's glass
    // navigation height is 68, independent of the sheet expansion fraction.
    let headerHeight: CGFloat = 68
    header.frame = CGRect(x: safeArea.left, y: 16, width: width - safeArea.left - safeArea.right, height: 44)
    header.alpha = 1; header.isUserInteractionEnabled = true
    headerLeftSurface.frame = CGRect(x: 16, y: 0, width: 44, height: 44)
    headerRightSurface.frame = CGRect(x: header.bounds.width - 60, y: 0, width: 44, height: 44)
    close.frame = CGRect(origin: .zero, size: CGSize(width: 44, height: 44))
    review.frame = CGRect(origin: .zero, size: CGSize(width: 44, height: 44))
    albums.frame = CGRect(x: 64, y: 0, width: max(0, header.bounds.width - 128), height: 44)
    albums.setNeedsLayout()
    selectedButton.isHidden = selected.isEmpty || activeCategory != "photos"
    selectedButton.frame = CGRect(x: 72, y: 0, width: max(44, selectedButton.sizeThatFits(CGSize(width: 80, height: 44)).width), height: 44)
    // The title and trailing controls stay centered independently of selection.
    // The upstream selected-count control is positioned after the close button.
    albumMenu.updateLayout(anchor: header.convert(albums.frame, to: root), safeArea: UIEdgeInsets(top: 12, left: safeArea.left, bottom: safeArea.bottom, right: safeArea.right))
    let manageHeight: CGFloat = limited ? 36 : 0
    manage.isHidden = !limited || activeCategory != "photos"
    manage.frame = CGRect(x: 8 + safeArea.left, y: headerHeight, width: width - 16 - safeArea.left - safeArea.right, height: manageHeight)
    let top = headerHeight + manageHeight
    grid.frame = CGRect(x: safeArea.left, y: top, width: width - safeArea.left - safeArea.right, height: max(0, height - top))
    grid.isHidden = activeCategory != "photos" || !granted || frozenGallery != nil
    let availableWidth = width - safeArea.left - safeArea.right
    let barWidth: CGFloat
    switch buttons.count {
    case 2: barWidth = min(160, availableWidth - 44)
    case 3: barWidth = min(240, availableWidth - 44)
    case 4: barWidth = min(300, availableWidth - 44)
    default: barWidth = max(0, availableWidth - 44)
    }
    let bottom = max(22, safeArea.bottom) + additionalBottomInset
    footer.frame = CGRect(x: safeArea.left + floor((availableWidth - barWidth) / 2), y: max(top, height - bottom - 62), width: barWidth, height: 62)
    categoryLens.frame = footer.bounds.insetBy(dx: 3, dy: 3)
    categories.frame = CGRect(x: 3, y: 0, width: max(0, barWidth - 6), height: 62)
    let buttonWidth: CGFloat = 72
    let stride = buttons.count > 5 && !landscape ? 62 : floor((barWidth - 6 - buttonWidth) / CGFloat(max(1, buttons.count - 1)))
    for (index, pair) in buttons.enumerated() {
      pair.1.frame = CGRect(x: CGFloat(index) * stride, y: -3, width: buttonWidth, height: 62)
      selectedButtons[index].frame = pair.1.frame
    }
    categories.contentSize = CGSize(width: max(barWidth - 6, buttonWidth + CGFloat(max(0, buttons.count - 1)) * stride), height: 62)
    categories.isScrollEnabled = categories.contentSize.width - categories.bounds.width > 1
    if !lensIsLifted { categoryLens.clipsToBounds = categories.isScrollEnabled }
    selectedCategories.frame = CGRect(origin: .zero, size: categories.contentSize)
    unselectedCategories.frame = selectedCategories.frame
    updateCategoryLens()
    let captionInset: CGFloat = caption.isFirstResponder ? 16 : 28
    let captionWidth = max(0, availableWidth - captionInset * 2), textWidth = max(0, captionWidth - 52)
    let textHeight = min(128, max(44, caption.sizeThatFits(CGSize(width: textWidth, height: .greatestFiniteMagnitude)).height))
    caption.isScrollEnabled = textHeight >= 128
    let captionBottom: CGFloat = (keyboardHeight > 0 ? keyboardHeight + 8 : max(28, safeArea.bottom)) + additionalBottomInset
    captionPanel.frame = CGRect(x: safeArea.left + captionInset, y: max(top, height - captionBottom - textHeight), width: captionWidth, height: textHeight)
    caption.frame = CGRect(x: 0, y: 0, width: textWidth, height: textHeight)
    captionBackground.frame = caption.frame; captionBackground.layer.cornerRadius = min(22, textHeight / 2)
    captionPlaceholder.frame = CGRect(x: 15, y: 0, width: max(0, textWidth - 30), height: 44)
    sendButton.frame = CGRect(x: captionWidth - 44, y: textHeight - 44, width: 44, height: 44)
    countLabel.frame = CGRect(x: captionWidth - 17, y: textHeight - 49, width: 18, height: 18)
    if !selectionAnimating {
      updateSelectionPresentation()
      captionPanel.isHidden = !selecting || !captionEnabled; footer.isHidden = selecting && captionEnabled
    }
    let footerTop = selecting && captionEnabled ? captionPanel.frame.minY : footer.frame.minY
    fileLanding.view.frame = root.bounds
    fileLanding.view.isHidden = activeCategory == "photos"
    fileLanding.updateLayout(navigationHeight: headerHeight, bottomInset: max(0, height - footerTop, fileLanding.searching ? keyboardHeight : 0), safeArea: safeArea, additionalBottomInset: additionalBottomInset)
    if activeCategory != "photos" && fileLanding.searching {
      header.isHidden = true; footer.isHidden = true; captionPanel.isHidden = true
    }
    footer.isUserInteractionEnabled = screen != "albums"
    captionPanel.isUserInteractionEnabled = screen != "albums"
    grid.contentInset.bottom = max(0, height - footerTop + 12); grid.verticalScrollIndicatorInsets.bottom = grid.contentInset.bottom
    message.isHidden = activeCategory != "photos" || granted || loading; settings.isHidden = activeCategory != "photos" || granted || loading
    message.frame = CGRect(x: 20, y: top, width: width - 40, height: max(0, footerTop - top - 44))
    settings.frame = CGRect(x: 20, y: max(top, footerTop - 44), width: width - 40, height: 44)
    retry.isHidden = activeCategory != "photos" || !hasError; retry.frame = CGRect(x: 8, y: grid.frame.minY, width: width - 16, height: 44)
    activity.center = CGPoint(x: width / 2, y: (top + footerTop) / 2)
    activity.isHidden = activeCategory != "photos" || !(loading || busy)
    updateCameraPreview()
  }
}
