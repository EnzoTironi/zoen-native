import AVFoundation
import Photos
import UIKit

@MainActor extension ZoenAttachmentKeyboardContent {
  func update(model: [String: Any], appearance: ZoenKeyboardPanelAppearance) throws {
    guard !retired, let items = model["items"] as? [[String: Any]], items.count <= 20_000,
          let selected = model["selected"] as? [String], selected.count <= 100,
          let labels = model["labels"] as? [String: String],
          items.allSatisfy({ ($0["id"] as? String)?.hasPrefix("ph://") == true }),
          selected.allSatisfy({ $0.hasPrefix("ph://") && $0.utf8.count <= 2048 && !$0.contains("\0") }),
          Set(items.compactMap { $0["id"] as? String }).count == items.count,
          Set(selected).count == selected.count else { throw Failure.invalid }
    let itemsChanged = !NSArray(array: items).isEqual(to: self.items)
    let nextAlbum = model["albumId"] as? String
    let albumChanged = nextAlbum != albumId
    let nextScreen = model["screen"] as? String ?? "grid"
    guard ["grid", "albums", "preview", "selected"].contains(nextScreen) else { throw Failure.invalid }
    let screenChanged = screen != nextScreen
    if screenChanged { caption.resignFirstResponder() }
    screen = nextScreen
    activeCategory = model["activeCategory"] as? String ?? "photos"
    guard ["photos", "document", "audio"].contains(activeCategory) else { throw Failure.invalid }
    let nextCamera = model["cameraAvailable"] as? Bool != false && model["albumId"] is NSNull
    let layoutChanged = nextCamera != showCamera
    if itemsChanged || albumChanged { requestedCount = nil }
    albumId = nextAlbum
    self.items = items; self.selected = selected; self.labels = labels; self.appearance = appearance
    root.overrideUserInterfaceStyle = appearance.isDark ? .dark : .light
    busy = model["busy"] as? Bool == true; granted = model["granted"] as? Bool == true
    limited = model["limited"] as? Bool == true; loading = model["loading"] as? Bool == true
    hasMore = model["hasMore"] as? Bool == true; hasError = model["error"] as? Bool == true
    hasAudio = model["hasAudio"] as? Bool == true
    selectionGesture.isEnabled = !busy && granted
    showCamera = nextCamera; layout.camera = showCamera
    root.backgroundColor = appearance.background; grid.backgroundColor = appearance.background
    close.tintColor = appearance.text; close.accessibilityLabel = screen == "grid" ? labels["close"] : labels["back"]
    close.setImage(ZoenTelegramAttachmentGlyph.image(screen == "grid" ? .close : .back), for: .normal)
    albums.tintColor = appearance.text; albums.setTitleColor(appearance.text, for: .normal)
    albums.setTitle(activeCategory == "photos" ? model["albumTitle"] as? String : labels[activeCategory == "audio" ? "audioTitle" : "fileTitle"], for: .normal)
    albumArrow.image = ZoenTelegramAttachmentGlyph.image(.titleExpand)
    albumArrow.tintColor = appearance.text.withAlphaComponent(0.4)
    albumArrow.isHidden = activeCategory != "photos"
    albums.alpha = screen == "albums" ? 0.5 : 1
    var albumRows = [[String: Any]]()
    var recents: [String: Any] = ["id": "", "title": labels["recents"] ?? ""]
    if model["albumId"] is NSNull {
      if let count = model["total"] as? Int { recents["count"] = count }
      if let first = items.first?["id"] as? String { recents["previewId"] = first }
    }
    albumRows.append(recents)
    albumRows.append(contentsOf: (model["albums"] as? [[String: Any]] ?? []).filter { ($0["id"] as? String) != "" })
    try albumMenu.update(rows: albumRows, dismissLabel: labels["back"] ?? labels["close"], appearance: appearance)
    review.tintColor = appearance.text; review.accessibilityLabel = labels["more"]
    let revision = (model["revision"] as? String).flatMap {
      !$0.isEmpty && $0.utf8.count <= 128 && !$0.contains("\0") ? $0 : nil
    }
    sendOptionsRevision = revision
    sendAsFileOptionEnabled = false
    sendUngroupedOptionEnabled = false
    let optionSelection = selected
    let options = activeCategory == "photos" && screen == "grid" && revision != nil
      ? model["sendOptions"] as? [[String: Any]] ?? [] : []
    sendOptionActions = options.compactMap { option -> ZoenAttachmentReferenceMenu.Action? in
      guard let id = option["id"] as? String, ["send-as-file", "send-without-grouping"].contains(id), let revision,
            let title = option["title"] as? String, !title.isEmpty,
            title.utf16.count <= 1024, !title.contains("\0"),
            (id == "send-without-grouping" ? (2...10) : (1...10)).contains(optionSelection.count) else { return nil }
      let enabled = option["enabled"] as? Bool != false
      if id == "send-as-file" { sendAsFileOptionEnabled = enabled }
      else { sendUngroupedOptionEnabled = enabled }
      // MediaPickerScreen:3174–3181,3195–3203 uses the original File and
      // GroupingOff glyphs with explicit terminal intents; no MIME substitution.
      return ZoenAttachmentReferenceMenu.Action(id: id, title: title,
        image: ZoenTelegramAttachmentGlyph.image(id == "send-as-file" ? .sendAsFile : .sendUngrouped), enabled: enabled) { [weak self] in
        self?.performSendOption(id: id, revision: revision, selection: optionSelection)
      }
    }
    let hasOptions = !sendOptionActions.isEmpty
    review.isEnabled = !busy && hasOptions
    review.isHidden = !hasOptions
    headerRightSurface.isHidden = !hasOptions
    selectedButton.setImage(ZoenTelegramAttachmentGlyph.image(.selected), for: .normal)
    selectedButton.setTitle(" \(selected.count)", for: .normal)
    selectedButton.tintColor = appearance.text
    selectedButton.accessibilityLabel = labels["selected"]
    selectedButton.accessibilityValue = String(selected.count)
    selectedButton.isEnabled = !busy
    close.isEnabled = !busy; albums.isEnabled = !busy && activeCategory == "photos"
    manage.setTitle(labels["manage"], for: .normal); manage.tintColor = appearance.accent; manage.isEnabled = !busy
    message.text = labels["permission"]; message.textColor = appearance.text
    settings.setTitle(labels["settings"], for: .normal); settings.tintColor = appearance.accent
    retry.setTitle(labels["retry"], for: .normal); retry.tintColor = appearance.accent
    activity.color = appearance.secondary
    if loading || busy { activity.startAnimating() } else { activity.stopAnimating() }
    let isDark = appearance.isDark
    footer.update(isDark: isDark, cornerRadius: 31)
    captionBackground.update(isDark: isDark, cornerRadius: 22)
    headerLeftSurface.update(isDark: isDark, cornerRadius: 22)
    headerRightSurface.update(isDark: isDark, cornerRadius: 22)
    try fileLanding.update(model: model, appearance: appearance)
    fileLanding.setVisible(activeCategory != "photos" && !galleryVisible)
    recentSelected = (model["recent"] as? [String: Any])?["selected"] as? [String] ?? []
    try updateCaption(model["caption"] as? [String: Any])
    if activeCategory != "photos" {
      review.setImage(ZoenTelegramFileVisuals.searchIcon(), for: .normal)
      review.accessibilityLabel = labels["search"]
      review.isEnabled = !busy && fileLanding.searchAvailable
      review.isHidden = !fileLanding.searchAvailable
      headerRightSurface.isHidden = !fileLanding.searchAvailable
    } else {
      review.setImage(ZoenTelegramAttachmentGlyph.image(.more), for: .normal)
    }
    rebuildCategories(model["categories"] as? [[String: String]])
    if galleryVisible {
      var galleryModel = model; galleryModel["source"] = "photos"
      try gallery.update(model: galleryModel, appearance: appearance)
    }
    gallery.setVisible(galleryVisible)
    if itemsChanged || layoutChanged { grid.reloadData(); layout.invalidateLayout() }
    else {
      for case let cell as PhotoCell in grid.visibleCells {
        guard let id = cell.identifier else { continue }
        cell.updateSelection(order: selected.firstIndex(of: id).map { $0 + 1 }, appearance: appearance)
      }
    }
    if albumChanged { grid.setContentOffset(.zero, animated: false) }
    if screenChanged {
      gallery.view.isHidden = !galleryVisible
      albumMenu.isHidden = screen != "albums"
      UIAccessibility.post(notification: .layoutChanged, argument: galleryVisible ? gallery.view : (screen == "albums" ? albumMenu : header))
    }
    let nextSelecting = activeCategory == "photos" ? !selected.isEmpty : !recentSelected.isEmpty
    if selecting != nextSelecting {
      selecting = nextSelecting
      if !selecting { caption.resignFirstResponder() }
      selectionAnimation += 1
      let animation = selectionAnimation
      selectionAnimating = true
      reflow()
      let duration = UIAccessibility.isReduceMotionEnabled ? 0 : (selecting ? 0.1 : 0.25)
      captionPanel.isHidden = false; footer.isHidden = false
      UIView.animate(withDuration: duration, delay: 0, options: [.beginFromCurrentState, .allowUserInteraction]) {
        self.updateSelectionPresentation()
      } completion: { [weak self] _ in
        guard let self, self.selectionAnimation == animation else { return }
        self.selectionAnimating = false
        self.reflow()
      }
    }
    reflow()
    referenceMenu?.invalidateIfNeeded()
    if activeCategory != "photos" && fileLanding.searching {
      DispatchQueue.main.async { [weak self] in self?.fileLanding.focusSearch() }
    }
    if screenChanged { attachmentPresentationChanged?() }
  }

  private func updateCaption(_ value: [String: Any]?) throws {
    captionEnabled = value != nil
    captionSending = value?["sending"] as? Bool == true
    if let value {
      guard let text = value["value"] as? String, text.utf16.count <= 4096, !text.contains("\0") else { throw Failure.invalid }
      // A delayed React echo cannot replace the active native selection or IME.
      if !caption.isFirstResponder && caption.text != text { caption.text = text }
      captionPlaceholder.text = value["placeholder"] as? String
      caption.accessibilityLabel = value["placeholder"] as? String
      captionEditable = value["editable"] as? Bool == true && value["sending"] as? Bool != true
      sendButton.accessibilityLabel = value["sendLabel"] as? String
    } else { captionEditable = false }
    caption.isEditable = captionEditable && !busy
    caption.keyboardAppearance = appearance?.isDark == true ? .dark : .light
    caption.textColor = appearance?.text; caption.tintColor = appearance?.accent
    captionPlaceholder.textColor = appearance?.secondary; captionPlaceholder.isHidden = !caption.text.isEmpty
    sendButton.backgroundColor = appearance?.accent; sendButton.tintColor = .white
    let selectedCount = activeCategory == "photos" ? selected.count : recentSelected.count
    sendButton.isEnabled = !busy && !captionSending && selectedCount > 0
    countLabel.text = String(selectedCount); countLabel.backgroundColor = appearance?.background; countLabel.textColor = appearance?.accent
  }

  private func rebuildCategories(_ supplied: [[String: String]]?) {
    var choices = [("photos", labels["photos"] ?? ""), ("document", labels["files"] ?? "")]
    if hasAudio { choices.append(("audio", labels["audio"] ?? "")) }
    if let supplied, !supplied.isEmpty {
      var seen = Set<String>()
      choices = supplied.compactMap { value in
        guard let id = value["id"], Self.categoryGlyphs[id] != nil, let label = value["label"],
              !label.isEmpty, seen.insert(id).inserted else { return nil }
        return (id, label)
      }
      if choices.isEmpty { choices = [("photos", labels["photos"] ?? ""), ("document", labels["files"] ?? "")] }
    }
    let key = choices.map { $0.0 + ":" + $0.1 }.joined(separator: "|")
    if categoriesKey != key {
      for (_, button) in buttons { button.removeFromSuperview() }
      for button in selectedButtons { button.removeFromSuperview() }
      selectedButtons.removeAll()
      buttons.removeAll(); categoriesKey = key
      for (action, title) in choices {
        let button = AttachmentCategoryButton(type: .custom)
        button.setTitle(title, for: .normal)
        button.accessibilityIdentifier = "attachment-keyboard-\(action)"
        button.addAction(UIAction { [weak self] _ in
          guard let self else { return }
          if action == self.activeCategory {
            let scroll = self.attachmentScrollView
            scroll.setContentOffset(CGPoint(x: 0, y: -scroll.adjustedContentInset.top), animated: true)
            self.attachmentPresentationChanged?()
          }
          else { self.emit("category", id: action) }
        }, for: .touchUpInside)
        unselectedCategories.addSubview(button); buttons.append((action, button))
        let selected = AttachmentCategoryButton(type: .custom)
        selected.setTitle(title, for: .normal)
        selectedCategories.addSubview(selected)
        selectedButtons.append(selected)
      }
    }
    for (index, pair) in buttons.enumerated() {
      let (id, button) = pair
      if let glyph = Self.categoryGlyphs[id], let appearance {
        button.update(glyph: glyph, tintColor: appearance.secondary, isAnimating: id == activeCategory)
        selectedButtons[index].update(glyph: glyph, tintColor: appearance.accent, isAnimating: id == activeCategory)
      }
      button.accessibilityTraits = id == activeCategory ? [.button, .selected] : .button
      button.isEnabled = !busy
    }
  }

}
