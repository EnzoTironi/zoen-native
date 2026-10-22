import AVFoundation
import Photos
import UIKit

@MainActor extension ZoenAttachmentKeyboardContent {
  func photo(at index: Int) -> [String: Any]? {
    let photoIndex = index - (showCamera ? 1 : 0)
    return items.indices.contains(photoIndex) ? items[photoIndex] : nil
  }
  func emit(_ action: String, id: String? = nil) {
    guard !retired, !busy else { return }
    send(action, id.map { ["id": $0] } ?? [:])
  }
  func showSendOptions() {
    guard referenceMenu == nil, let appearance, let window = review.window,
          let revision = sendOptionsRevision, !sendOptionActions.isEmpty else { return }
    let selection = selected
    let optionKeys = sendOptionActions.map { [$0.id, $0.title, $0.enabled ? "enabled" : "disabled"] }
    let menu = ZoenAttachmentReferenceMenu(source: review, actions: sendOptionActions,
      dismissLabel: labels["close"] ?? "", appearance: appearance,
      isValid: { [weak self] in
        guard let self else { return false }
        return !self.retired && !self.busy && !self.captionSending && self.captionEnabled &&
          self.activeCategory == "photos" && self.screen == "grid" &&
          self.sendOptionsRevision == revision && self.selected == selection &&
          self.sendOptionActions.map { [$0.id, $0.title, $0.enabled ? "enabled" : "disabled"] } == optionKeys &&
          !self.review.isHidden && self.appearance?.isDark == appearance.isDark
      }, didDismiss: { [weak self] menu in
        if self?.referenceMenu === menu { self?.referenceMenu = nil }
      })
    referenceMenu = menu
    if !menu.present(in: window) { referenceMenu = nil }
  }
  func performSendAsFileOption(revision: String, selection: [String]) {
    performSendOption(id: "send-as-file", revision: revision, selection: selection)
  }
  func performSendOption(id: String, revision: String, selection: [String]) {
    guard ["send-as-file", "send-without-grouping"].contains(id),
          id == "send-as-file" ? sendAsFileOptionEnabled : sendUngroupedOptionEnabled,
          !retired, !busy, !captionSending, captionEnabled,
          activeCategory == "photos", screen == "grid", sendOptionsRevision == revision,
          (id == "send-without-grouping" ? (2...10) : (1...10)).contains(selection.count), selection == selected,
          caption.text.utf16.count <= 4096, !caption.text.contains("\0") else { return }
    send("option", ["id": id, "ids": selection,
                    "revision": revision, "text": caption.text ?? ""])
  }
  func textView(_ textView: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
    guard captionEditable, !busy, !retired, !text.contains("\0"), let swiftRange = Range(range, in: textView.text) else { return false }
    return textView.text.replacingCharacters(in: swiftRange, with: text).utf16.count <= 4096
  }
  func textViewDidChange(_ textView: UITextView) {
    guard !retired, !busy, captionEditable, textView.text.utf16.count <= 4096, !textView.text.contains("\0") else { return }
    captionPlaceholder.isHidden = !textView.text.isEmpty
    send("caption", ["text": textView.text ?? ""]); reflow()
  }
  func textViewDidBeginEditing(_ textView: UITextView) {
    guard textView === caption, !retired, captionEditable, !busy else { return }
    attachmentDidBeginEditing?()
    reflow()
  }
  func textViewDidEndEditing(_ textView: UITextView) { reflow() }
  func collectionView(_ collectionView: UICollectionView, numberOfItemsInSection section: Int) -> Int { granted ? items.count + (showCamera ? 1 : 0) : 0 }
  func collectionView(_ collectionView: UICollectionView, cellForItemAt indexPath: IndexPath) -> UICollectionViewCell {
    guard let cell = collectionView.dequeueReusableCell(withReuseIdentifier: "photo", for: indexPath) as? PhotoCell else { return UICollectionViewCell() }
    if showCamera && indexPath.item == 0 {
      cell.camera(label: labels["camera"] ?? "", appearance: appearance, preview: cameraPreview) { [weak self] in self?.emit("camera") }
    }
    else if let item = photo(at: indexPath.item) {
      let id = item["id"] as? String ?? ""
      cell.photo(item: item, order: selected.firstIndex(of: id).map { $0 + 1 }, manager: manager, appearance: appearance, loadThumbnail: !retired, preview: { [weak self, weak cell] in
        guard cell?.identifier == id else { return }; self?.emit("preview", id: id)
      }) { [weak self, weak cell] in
        guard cell?.identifier == id else { return }; self?.emit("select", id: id)
      }
    }
    return cell
  }
  func collectionView(_ collectionView: UICollectionView, didSelectItemAt indexPath: IndexPath) {
    if showCamera && indexPath.item == 0 { cameraPreview.setRunning(false); emit("camera") }
    else if let id = photo(at: indexPath.item)?["id"] as? String { emit("preview", id: id) }
  }
  func collectionView(_ collectionView: UICollectionView, didEndDisplaying cell: UICollectionViewCell, forItemAt indexPath: IndexPath) { (cell as? PhotoCell)?.cancel(); updateCameraPreview() }
  func scrollViewDidScroll(_ scrollView: UIScrollView) {
    if scrollView === categories { updateCategoryLens(); return }
    guard scrollView === grid else { return }
    updateCameraPreview()
    guard hasMore, !loading, !busy, !retired, requestedCount != items.count,
          scrollView.contentOffset.y + scrollView.bounds.height >= scrollView.contentSize.height - scrollView.bounds.height * 0.6 else { return }
    requestedCount = items.count; emit("more")
  }
  func updateCameraPreview() {
    let visible = activeCategory == "photos" && !galleryVisible && screen != "albums" && showCamera && granted && !retired && root.window != nil && grid.indexPathsForVisibleItems.contains(IndexPath(item: 0, section: 0))
    cameraPreview.setRunning(visible)
  }
  func updateCategoryLens(transition: ComponentTransition = .immediate) {
    unselectedCategories.layer.sublayerTransform = CATransform3DMakeTranslation(-categories.contentOffset.x, 0, 0)
    selectedCategories.layer.sublayerTransform = CATransform3DMakeTranslation(-categories.contentOffset.x, 0, 0)
    let index = buttons.firstIndex(where: { $0.0 == activeCategory })
    let button = index.map { buttons[$0].1 }
    let selectionWidth = button.map { button in
      let titleWidth = min(64, button.titleLabel?.sizeThatFits(CGSize(width: 64, height: 62)).width ?? 0)
      return max(index == 0 || index == buttons.count - 1 ? button.bounds.width : 64, titleWidth + 24)
    } ?? 0
    let selection = CGRect(x: (button?.frame.midX ?? 0) - selectionWidth / 2, y: 0, width: selectionWidth, height: 56)
    let width = max(0, footer.bounds.width - 6)
    var selectionX = selection.minX - categories.contentOffset.x
    if let selection = tabSelection, selection.isLifted { selectionX = selection.currentX }
    if !categories.isScrollEnabled { selectionX = max(0, min(selectionX, width - selectionWidth)) }
    categoryLens.update(size: CGSize(width: width, height: 56), cornerRadius: 28,
      selectionOrigin: CGPoint(x: selectionX, y: 0),
      selectionSize: CGSize(width: selection.width, height: 56), inset: 0,
      isDark: appearance?.isDark ?? false, isLifted: tabSelection?.isLifted == true || lensIsLifted,
      isCollapsed: selecting || buttons.count < 2, transition: transition)
  }
  func scrollViewWillBeginDragging(_ scrollView: UIScrollView) {
    guard scrollView === categories, tabSelection != nil else { return }
    tabSelection = nil
    updateCategoryLens(transition: .spring(duration: 0.4))
  }
  func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer) -> Bool { true }
  private func tab(at point: CGPoint) -> (String, AttachmentCategoryButton)? {
    let translated = CGPoint(x: point.x + categories.contentOffset.x, y: point.y)
    if let item = buttons.first(where: { $0.1.frame.contains(translated) }) { return item }
    return buttons.min { abs($0.1.frame.midX - translated.x) < abs($1.1.frame.midX - translated.x) }
  }
  @objc func selectTab(_ recognizer: TabSelectionRecognizer) {
    guard !retired, !busy, !selecting else { return }
    let point = recognizer.location(in: categoryLens.contentView)
    switch recognizer.state {
    case .began:
      guard let (id, button) = tab(at: point) else { return }
      tabSelection = (button.frame.minX, button.frame.minX, id, !categories.isScrollEnabled)
      updateCategoryLens(transition: .spring(duration: 0.4))
    case .changed:
      guard var selection = tabSelection else { return }
      selection.currentX = selection.startX + recognizer.translation(in: root).x
      if let (id, _) = tab(at: point) { selection.id = id }
      tabSelection = selection
      updateCategoryLens()
    case .ended, .cancelled:
      guard let selection = tabSelection else { return }
      tabSelection = nil
      if !selection.isLifted {
        lensGeneration += 1
        let generation = lensGeneration
        lensIsLifted = true; categoryLens.clipsToBounds = false
        updateCategoryLens(transition: .spring(duration: 0.4))
        // AttachmentPanel:2015–2030 uses these visual-only source delays. They
        // never decide dismissal, ownership, selection, materialization or send.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { [weak self] in
          guard let self, !self.retired, self.lensGeneration == generation else { return }
          self.lensIsLifted = false
          self.updateCategoryLens(transition: .spring(duration: 0.4))
          DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) { [weak self] in
            guard let self, !self.retired, self.lensGeneration == generation else { return }
            self.categoryLens.clipsToBounds = true
          }
        }
      }
      if recognizer.state == .ended, let (_, button) = buttons.first(where: { $0.0 == selection.id }) {
        if selection.id == activeCategory {
          let scroll = attachmentScrollView
          scroll.setContentOffset(CGPoint(x: 0, y: -scroll.adjustedContentInset.top), animated: true)
          attachmentPresentationChanged?()
        } else { emit("category", id: selection.id) }
        if buttons.count > 5 { categories.scrollRectToVisible(button.frame.insetBy(dx: -35, dy: 0), animated: true) }
      }
      updateCategoryLens(transition: .spring(duration: 0.4))
    default: break
    }
  }
}
