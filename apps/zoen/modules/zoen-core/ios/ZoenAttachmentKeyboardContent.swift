import AVFoundation
import Photos
import UIKit

// Adapted from TelegramMessenger/Telegram-iOS, GPL-2.0-or-later (GPLv3 selected),
// f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838: AttachmentPanel.swift:35–58,
// 326–432, 1968–2059, 2080–2310, 2530–2805;
// MediaPickerScreen.swift:1236–1307, 1745–1769, 2508–2558, 2688–2720,
// 2848–2855, 3174–3181, 3515–3610; Display/Source/ViewController.swift:263–283.
// Selection: MediaPickerGridItem.swift:824–825; CheckNode.swift:39–57, 476–488,
// 604–618 (29-point overlay, inset stroke and rounded selection order).
// File/Audio landing: TelegramUI/Components/AttachmentFileController/Sources/
// AttachmentFileController.swift:313–351, 885–926; AttachmentFileEmptyItem.swift:
// 120–168; ItemListPeerActionItem.swift:215–267, 308–310.
// See THIRD_PARTY_NOTICES.md. PhotoSession owns the query, selection, copies and
// send. UIKit requests local read-only thumbnails; preview never requests access.
@MainActor
final class ZoenAttachmentKeyboardContent: NSObject, ZoenAttachmentOverlayContent,
  UICollectionViewDataSource, UICollectionViewDelegate, UITextViewDelegate, UIGestureRecognizerDelegate {
  private let root = AttachmentRootView()
  var view: UIView { root }
  var attachmentScrollView: UIScrollView {
    if galleryVisible { return gallery.scrollView }
    if screen == "albums" { return albumMenu.scrollView }
    if activeCategory != "photos" { return fileLanding.scrollView }
    return grid
  }
  var attachmentHasKeyboardFocus: Bool { caption.isFirstResponder || (galleryVisible && gallery.hasKeyboardFocus) || (activeCategory != "photos" && fileLanding.hasKeyboardFocus) }
  var attachmentHandlesVerticalGesture: Bool { screen == "preview" || screen == "albums" }
  var attachmentPrefersFullScreen: Bool { screen == "preview" }
  var attachmentDidBeginEditing: (() -> Void)?
  var attachmentPresentationChanged: (() -> Void)?
  var attachmentAnimatedViews: [UIView] {
    [grid, header, headerLeftSurface, headerRightSurface, footer, categoryLens, categories, unselectedCategories, selectedCategories, captionPanel,
     captionBackground, caption, captionPlaceholder, sendButton, countLabel,
     close, albums, albumArrow, selectedButton, review, manage, message, settings, retry, activity]
      + buttons.map(\.1) + selectedButtons + gallery.animatedViews + albumMenu.animatedViews + fileLanding.animatedViews
  }
  private let send: (String, [String: Any]) -> Void
  private let layout = PhotoLayout()
  private lazy var grid = UICollectionView(frame: .zero, collectionViewLayout: layout)
  private let header = UIView()
  private let headerLeftSurface = ZoenTelegramAttachmentGlassView()
  private let headerRightSurface = ZoenTelegramAttachmentGlassView()
  private let footer = ZoenTelegramAttachmentGlassView()
  private let categoryLens = LiquidLensView(kind: .noContainer)
  private let categories = UIScrollView()
  private let unselectedCategories = UIView()
  private let selectedCategories = UIView()
  private let captionPanel = UIView()
  private let captionBackground = ZoenTelegramAttachmentGlassView()
  private let caption = UITextView()
  private let captionPlaceholder = UILabel()
  private let sendButton = UIButton(type: .system)
  private let countLabel = UILabel()
  private let close = UIButton(type: .system)
  private let albums = AttachmentAlbumTitleButton(type: .custom)
  private var albumArrow: UIImageView { albums.arrow }
  private let selectedButton = UIButton(type: .system)
  private let review = UIButton(type: .system)
  private let manage = UIButton(type: .system)
  private let message = UILabel()
  private let settings = UIButton(type: .system)
  private let retry = UIButton(type: .system)
  private let activity = UIActivityIndicatorView(style: .medium)
  private let manager = PHCachingImageManager()
  private let selectionGesture = GridSelectionGesture()
  private let cameraPreview = AttachmentCameraPreview()
  private lazy var gallery = ZoenAttachmentGalleryContent { [weak self] action, payload in
    guard let self, !self.retired, !self.busy else { return }
    self.send(action, payload)
  }
  private lazy var albumMenu = ZoenAttachmentAlbumMenu(select: { [weak self] id in
    self?.emit("album", id: id)
  }, dismiss: { [weak self] in self?.emit("back") }, details: { [weak self] id in self?.emit("albumDetails", id: id) })
  private lazy var fileLanding = ZoenAttachmentFileContent { [weak self] action, payload in
    guard let self, !self.retired, !self.busy, self.activeCategory != "photos" else { return }
    self.send(action, payload)
  }
  private var recentSelected: [String] = []
  private var items: [[String: Any]] = []
  private var selected: [String] = []
  private var labels: [String: String] = [:]
  private var buttons: [(String, AttachmentCategoryButton)] = []
  private var selectedButtons: [AttachmentCategoryButton] = []
  private var categoriesKey = ""
  private var gestureSent = Set<String>()
  private var busy = false
  private var retired = false
  private var granted = false
  private var limited = false
  private var loading = false
  private var hasMore = false
  private var hasError = false
  private var hasAudio = false
  private var showCamera = false
  private var captionEditable = false
  private var captionEnabled = false
  private var captionSending = false
  private var selecting = false
  private var selectionAnimation = 0
  private var selectionAnimating = false
  private var tabSelection: (startX: CGFloat, currentX: CGFloat, id: String, isLifted: Bool)?
  private var lensIsLifted = false
  private var lensGeneration = 0
  private lazy var tabSelectionRecognizer = TabSelectionRecognizer(target: self, action: #selector(selectTab(_:)))
  private var expansion: CGFloat = 0
  private var keyboardHeight: CGFloat = 0
  private var additionalBottomInset: CGFloat = 0
  private var safeArea = UIEdgeInsets.zero
  private var requestedCount: Int?
  private var sendOptionsRevision: String?
  private var sendAsFileOptionEnabled = false
  private var albumId: String?
  private var screen = "grid"
  private var activeCategory = "photos"
  private var galleryVisible: Bool { screen == "preview" || screen == "selected" }
  private var frozenGallery: UIView?
  private var appearance: ZoenKeyboardPanelAppearance?
  private static let categoryGlyphs: [String: ZoenTelegramAttachmentGlyph] = ["photos": .gallery, "document": .file,
    "audio": .audio, "location": .location, "poll": .poll, "contact": .contact]

  init(send: @escaping (String, [String: Any]) -> Void) {
    self.send = send
    super.init()
    root.accessibilityIdentifier = "attachment-keyboard-panel"
    root.onLayout = { [weak self] in self?.reflow() }
    grid.register(PhotoCell.self, forCellWithReuseIdentifier: "photo")
    grid.dataSource = self; grid.delegate = self
    grid.alwaysBounceVertical = true
    // MediaPickerScreen.gridNode.scrollingInitiated dismisses caption input.
    grid.keyboardDismissMode = .onDrag
    grid.accessibilityIdentifier = "attachment-keyboard-photos"
    grid.contentInsetAdjustmentBehavior = .never
    root.addSubview(grid)
    for child in [header, footer, captionPanel, manage, message, settings, retry, activity] { root.addSubview(child) }
    for child in [headerLeftSurface, albums, selectedButton, headerRightSurface] { header.addSubview(child) }
    headerLeftSurface.contentView.addSubview(close)
    headerRightSurface.contentView.addSubview(review)
    footer.clipsToBounds = true; footer.layer.cornerCurve = .continuous; footer.layer.cornerRadius = 31
    categories.showsHorizontalScrollIndicator = false
    categories.accessibilityIdentifier = "attachment-category-scroll"
    categories.contentInsetAdjustmentBehavior = .never
    footer.contentView.addSubview(categoryLens)
    // AttachmentPanel:2671–2687 keeps the interactive scroll surface above
    // the lens. Its two item containers are visual only; the original lens
    // deliberately disables interaction inside its rendering container.
    footer.contentView.addSubview(categories)
    categoryLens.contentView.addSubview(unselectedCategories)
    categoryLens.selectedContentView.addSubview(selectedCategories)
    categories.delegate = self
    selectedCategories.isUserInteractionEnabled = false
    selectedCategories.accessibilityElementsHidden = true
    categoryLens.clipsToBounds = true; categoryLens.layer.cornerRadius = 28
    tabSelectionRecognizer.delegate = self
    categories.addGestureRecognizer(tabSelectionRecognizer)
    captionPanel.addSubview(captionBackground)
    captionBackground.clipsToBounds = true
    captionBackground.layer.cornerCurve = .continuous; captionBackground.layer.cornerRadius = 22
    for child in [caption, captionPlaceholder, sendButton, countLabel] { captionPanel.addSubview(child) }
    caption.font = .systemFont(ofSize: 17); caption.backgroundColor = .clear
    caption.textContainerInset = UIEdgeInsets(top: 10, left: 10, bottom: 10, right: 10)
    caption.delegate = self; caption.accessibilityIdentifier = "attachment-caption"
    captionPlaceholder.font = .systemFont(ofSize: 17); captionPlaceholder.isUserInteractionEnabled = false
    sendButton.setImage(ZoenTelegramAttachmentGlyph.image(.send), for: .normal)
    sendButton.layer.cornerRadius = 22; sendButton.accessibilityIdentifier = "attachment-send"
    sendButton.addAction(UIAction { [weak self] _ in
      guard let self, !self.retired, !self.busy, !self.captionSending,
            (self.activeCategory == "photos" ? !self.selected.isEmpty : !self.recentSelected.isEmpty), self.captionEnabled,
            self.caption.text.utf16.count <= 4096, !self.caption.text.contains("\0") else { return }
      self.send(self.activeCategory == "photos" ? "send" : "recentSend", ["text": self.caption.text ?? ""])
    }, for: .touchUpInside)
    review.addAction(UIAction { [weak self] _ in
      guard let self, !self.retired, !self.busy, self.activeCategory != "photos", self.fileLanding.searchAvailable else { return }
      self.send("recentSearch", [:])
    }, for: .touchUpInside)
    countLabel.font = .systemFont(ofSize: 11, weight: .semibold); countLabel.textAlignment = .center
    countLabel.layer.cornerRadius = 9; countLabel.clipsToBounds = true
    close.setImage(ZoenTelegramAttachmentGlyph.image(.close), for: .normal)
    close.accessibilityIdentifier = "attachment-keyboard-close"
    close.addAction(UIAction { [weak self] _ in
      guard let self else { return }
      self.emit(self.screen == "grid" ? "close" : "back")
    }, for: .touchUpInside)
    review.setImage(ZoenTelegramAttachmentGlyph.image(.more), for: .normal)
    review.accessibilityIdentifier = "attachment-keyboard-review"
    // Telegram's more control exposes send options, never terminal approval.
    review.showsMenuAsPrimaryAction = true
    albums.titleLabel?.font = .systemFont(ofSize: 17, weight: .semibold)
    albums.addAction(UIAction { [weak self] _ in self?.emit("albums") }, for: .touchUpInside)
    selectedButton.titleLabel?.font = .systemFont(ofSize: 17, weight: .semibold)
    selectedButton.addAction(UIAction { [weak self] _ in self?.emit("selected") }, for: .touchUpInside)
    selectedButton.accessibilityIdentifier = "attachment-keyboard-selected"
    root.addSubview(gallery.view)
    gallery.view.isHidden = true
    gallery.setVisible(false)
    gallery.transitionSourceView = { [weak self] id in
      self?.grid.visibleCells.compactMap { $0 as? PhotoCell }.first(where: { $0.identifier == id })?.transitionView
    }
    root.addSubview(albumMenu)
    root.insertSubview(fileLanding.view, aboveSubview: grid)
    fileLanding.didBeginEditing = { [weak self] in self?.attachmentDidBeginEditing?() }
    gallery.didBeginEditing = { [weak self] in self?.attachmentDidBeginEditing?() }
    manage.titleLabel?.font = .systemFont(ofSize: 14)
    manage.addAction(UIAction { [weak self] _ in self?.emit("manage") }, for: .touchUpInside)
    settings.addAction(UIAction { [weak self] _ in self?.emit("settings") }, for: .touchUpInside)
    retry.titleLabel?.font = .systemFont(ofSize: 13)
    retry.addAction(UIAction { [weak self] _ in self?.emit("retry") }, for: .touchUpInside)
    message.numberOfLines = 0; message.textAlignment = .center
    message.font = .preferredFont(forTextStyle: .body); message.adjustsFontForContentSizeCategory = true
    selectionGesture.sideInset = 44
    selectionGesture.itemAt = { [weak self] point in
      guard let self, let index = self.grid.indexPathForItem(at: point)?.item,
            let item = self.photo(at: index), let id = item["id"] as? String else { return nil }
      return (id, self.selected.contains(id))
    }
    selectionGesture.updateSelection = { [weak self] id, _ in
      guard let self, !self.gestureSent.contains(id) else { return }
      self.gestureSent.insert(id); self.emit("select", id: id)
    }
    selectionGesture.began = { [weak self] in self?.gestureSent.removeAll() }
    selectionGesture.updateIsScrollEnabled = { [weak self] in self?.grid.isScrollEnabled = $0 }
    grid.addGestureRecognizer(selectionGesture)
    grid.panGestureRecognizer.require(toFail: selectionGesture)
  }

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
    let optionSelection = selected
    let options = activeCategory == "photos" && screen == "grid" && revision != nil
      ? model["sendOptions"] as? [[String: Any]] ?? [] : []
    review.menu = UIMenu(children: options.compactMap { option -> UIAction? in
      guard option["id"] as? String == "send-as-file", let revision,
            let title = option["title"] as? String, !title.isEmpty,
            title.utf16.count <= 1024, !title.contains("\0"),
            (1...10).contains(optionSelection.count) else { return nil }
      let enabled = option["enabled"] as? Bool != false
      sendAsFileOptionEnabled = enabled
      // MediaPickerScreen:3174–3181 uses the actual Context Menu/File glyph and
      // an explicit terminal file intent. No quality or MIME substitution.
      return UIAction(title: title, image: ZoenTelegramAttachmentGlyph.image(.sendAsFile),
                      attributes: enabled ? [] : .disabled) { [weak self] _ in
        self?.performSendAsFileOption(revision: revision, selection: optionSelection)
      }
    })
    let hasOptions = review.menu?.children.isEmpty == false
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
      review.menu = nil; review.showsMenuAsPrimaryAction = false
      review.setImage(ZoenTelegramFileVisuals.searchIcon(), for: .normal)
      review.accessibilityLabel = labels["search"]
      review.isEnabled = !busy && fileLanding.searchAvailable
      review.isHidden = !fileLanding.searchAvailable
      headerRightSurface.isHidden = !fileLanding.searchAvailable
    } else {
      review.showsMenuAsPrimaryAction = true
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
  }
  private func updateSelectionPresentation() {
    let active = selecting && captionEnabled
    captionPanel.alpha = active ? 1 : 0
    captionPanel.transform = active ? .identity : CGAffineTransform(scaleX: 0.85, y: 0.85)
    footer.alpha = active ? 0 : 1
    footer.transform = active ? CGAffineTransform(scaleX: 0.85, y: 0.85) : .identity
  }
  private func reflow() {
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
  private func photo(at index: Int) -> [String: Any]? {
    let photoIndex = index - (showCamera ? 1 : 0)
    return items.indices.contains(photoIndex) ? items[photoIndex] : nil
  }
  private func emit(_ action: String, id: String? = nil) {
    guard !retired, !busy else { return }
    send(action, id.map { ["id": $0] } ?? [:])
  }
  func performSendAsFileOption(revision: String, selection: [String]) {
    guard !retired, !busy, !captionSending, captionEnabled, sendAsFileOptionEnabled,
          activeCategory == "photos", screen == "grid", sendOptionsRevision == revision,
          (1...10).contains(selection.count), selection == selected,
          caption.text.utf16.count <= 4096, !caption.text.contains("\0") else { return }
    send("option", ["id": "send-as-file", "ids": selection,
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
  private func updateCameraPreview() {
    let visible = activeCategory == "photos" && !galleryVisible && screen != "albums" && showCamera && granted && !retired && root.window != nil && grid.indexPathsForVisibleItems.contains(IndexPath(item: 0, section: 0))
    cameraPreview.setRunning(visible)
  }
  private func updateCategoryLens(transition: ComponentTransition = .immediate) {
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
  @objc private func selectTab(_ recognizer: TabSelectionRecognizer) {
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
  func retire() {
    guard !retired else { return }
    retired = true; root.isUserInteractionEnabled = false
    attachmentDidBeginEditing = nil
    attachmentPresentationChanged = nil
    gallery.didBeginEditing = nil
    gallery.retire(); albumMenu.retire()
    fileLanding.retire()
    lensGeneration += 1; tabSelection = nil; tabSelectionRecognizer.isEnabled = false
    if let snapshot = grid.snapshotView(afterScreenUpdates: false) {
      snapshot.frame = grid.frame; snapshot.isUserInteractionEnabled = false
      snapshot.accessibilityElementsHidden = true
      root.insertSubview(snapshot, aboveSubview: grid)
      frozenGallery = snapshot; grid.isHidden = true
    }
    caption.resignFirstResponder(); caption.delegate = nil; selectionGesture.isEnabled = false
    grid.setContentOffset(grid.contentOffset, animated: false)
    grid.isScrollEnabled = false
    cameraPreview.setRunning(false)
    for cell in grid.visibleCells { (cell as? PhotoCell)?.cancel() }
    manager.stopCachingImagesForAllAssets()
    // Keep the rendered gallery and caption through the closing animation.
    // Retired guards remove all authority immediately; the owner releases this
    // content after actual removal, without flashing an empty sheet first.
  }
  func retireForDismissal(completion: @escaping @MainActor () -> Void) {
    retire()
    cameraPreview.retire(completion: completion)
  }
  private enum Failure: Error { case invalid }
}

// AttachmentPanel.AttachButtonComponent:326–432. Explicit icon/title geometry
// replaces UIButton.Configuration's unrelated intrinsic image/title spacing.
@MainActor
private final class AttachmentCategoryButton: UIButton {
  private let animation = ZoenTelegramAttachmentAnimationView(frame: .zero)
  override init(frame: CGRect) {
    super.init(frame: frame)
    titleLabel?.font = .systemFont(ofSize: 10, weight: .medium)
    titleLabel?.textAlignment = .center
    titleLabel?.lineBreakMode = .byTruncatingTail
    addSubview(animation)
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
  func update(glyph: ZoenTelegramAttachmentGlyph, tintColor: UIColor, isAnimating: Bool) {
    animation.update(glyph: glyph, tintColor: tintColor, isAnimating: isAnimating)
    setTitleColor(tintColor, for: .normal)
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    let pixel = 1 / max(1, traitCollection.displayScale)
    let top: CGFloat = 9 + pixel
    animation.frame = CGRect(x: floor((bounds.width - 30) / 2), y: top, width: 30, height: 30)
    let titleSize = titleLabel?.sizeThatFits(CGSize(width: 64, height: bounds.height)) ?? .zero
    titleLabel?.frame = CGRect(x: floor((bounds.width - min(64, titleSize.width)) / 2), y: top + 15 + 15 + pixel * 2, width: min(64, titleSize.width), height: titleSize.height)
  }
}

// MediaPickerTitleView:246–280, glass title offset and bundled expand arrow.
@MainActor
private final class AttachmentAlbumTitleButton: UIButton {
  let arrow = UIImageView()
  override init(frame: CGRect) {
    super.init(frame: frame)
    arrow.isUserInteractionEnabled = false
    addSubview(arrow)
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
  override func layoutSubviews() {
    super.layoutSubviews()
    let pixel = 1 / max(1, traitCollection.displayScale)
    let titleSize = titleLabel?.sizeThatFits(CGSize(width: min(210, max(0, bounds.width - 18)), height: 44)) ?? .zero
    let arrowSize = arrow.isHidden ? .zero : arrow.image?.size ?? .zero
    let spacing: CGFloat = arrow.isHidden ? 0 : 1
    let titleWidth = min(max(0, bounds.width - arrowSize.width - spacing), titleSize.width)
    let titleX = floor((bounds.width - titleWidth - arrowSize.width - spacing) / 2 / pixel) * pixel
    titleLabel?.frame = CGRect(x: titleX, y: floor((bounds.height - titleSize.height) / 2 / pixel) * pixel - 1, width: titleWidth, height: titleSize.height)
    arrow.frame = CGRect(x: titleX + titleWidth + spacing, y: floor((bounds.height - titleSize.height) / 2 / pixel) * pixel + titleSize.height / 2 - arrowSize.height / 2 - pixel, width: arrowSize.width, height: arrowSize.height)
  }
}

@MainActor
private final class AttachmentRootView: UIView {
  var onLayout: (() -> Void)?
  override func layoutSubviews() { super.layoutSubviews(); onLayout?() }
  override func didMoveToWindow() { super.didMoveToWindow(); onLayout?() }
}

@MainActor
private final class PhotoLayout: UICollectionViewLayout {
  var columns = 3 { didSet { if oldValue != columns { invalidateLayout() } } }
  var camera = true
  private var values: [UICollectionViewLayoutAttributes] = []
  private var contentHeight: CGFloat = 0
  override func prepare() {
    super.prepare()
    guard let collectionView else { return }
    let width = collectionView.bounds.width, scale = max(1, collectionView.traitCollection.displayScale)
    let size = floor((width - CGFloat(columns - 1)) / CGFloat(columns) * scale) / scale
    guard size > 0 else { values = []; contentHeight = 0; return }
    let count = collectionView.numberOfItems(inSection: 0), firstCount = (columns - 1) * 2
    values = (0..<count).map { index in
      let value = UICollectionViewLayoutAttributes(forCellWith: IndexPath(item: index, section: 0))
      if camera && index == 0 { value.frame = CGRect(x: 0, y: 0, width: size, height: size * 2 + 1) }
      else {
        let photo = index - (camera ? 1 : 0)
        let row = camera && photo < firstCount ? photo / (columns - 1) : (camera ? 2 + (photo - firstCount) / columns : photo / columns)
        let column = camera && photo < firstCount ? 1 + photo % (columns - 1) : (camera ? (photo - firstCount) % columns : photo % columns)
        value.frame = CGRect(x: CGFloat(column) * (size + 1), y: CGFloat(row) * (size + 1), width: size, height: size)
      }
      return value
    }
    contentHeight = values.map(\.frame.maxY).max() ?? 0
  }
  override var collectionViewContentSize: CGSize { CGSize(width: collectionView?.bounds.width ?? 0, height: contentHeight) }
  override func layoutAttributesForElements(in rect: CGRect) -> [UICollectionViewLayoutAttributes]? { values.filter { $0.frame.intersects(rect) } }
  override func layoutAttributesForItem(at indexPath: IndexPath) -> UICollectionViewLayoutAttributes? { values.indices.contains(indexPath.item) ? values[indexPath.item] : nil }
  override func shouldInvalidateLayout(forBoundsChange newBounds: CGRect) -> Bool { newBounds.width != collectionView?.bounds.width }
}

@MainActor
private final class PhotoCell: UICollectionViewCell {
  private let image = AttachmentThumbnailView()
  private let check = UIButton(type: .custom)
  private let badge = UIView()
  private let orderLabel = UILabel()
  private let duration = UILabel()
  private let cameraIcon = UIImageView(image: ZoenTelegramAttachmentGlyph.image(.camera))
  private weak var preview: UIView?
  private var request: PHImageRequestID?
  private weak var manager: PHImageManager?
  private var callback: (() -> Void)?
  private var activation: (() -> Void)?
  private(set) var identifier: String?
  var transitionView: UIView { image }
  override init(frame: CGRect) {
    super.init(frame: frame)
    contentView.clipsToBounds = true
    image.contentMode = .scaleAspectFill; image.clipsToBounds = true
    duration.textColor = .white; duration.font = .systemFont(ofSize: 12, weight: .semibold)
    duration.layer.shadowColor = UIColor.black.cgColor; duration.layer.shadowOpacity = 0.6; duration.layer.shadowRadius = 2
    badge.layer.borderColor = UIColor.white.cgColor; badge.isUserInteractionEnabled = false
    badge.layer.shadowColor = UIColor.black.cgColor; badge.layer.shadowOpacity = 0.3
    badge.layer.shadowRadius = 1; badge.layer.shadowOffset = CGSize(width: 0, height: 1)
    orderLabel.textColor = .white; orderLabel.textAlignment = .center
    badge.addSubview(orderLabel)
    check.addSubview(badge); check.sendSubviewToBack(badge)
    check.addAction(UIAction { [weak self] _ in self?.callback?() }, for: .touchUpInside)
    for child in [image, duration, cameraIcon, check] { contentView.addSubview(child) }
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
  func camera(label: String, appearance: ZoenKeyboardPanelAppearance?, preview: UIView, activate: @escaping () -> Void) {
    cancel(); identifier = nil; image.image = nil; check.isHidden = true; duration.isHidden = true
    self.preview = preview; contentView.insertSubview(preview, at: 0)
    preview.frame = contentView.bounds; preview.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    cameraIcon.isHidden = false; cameraIcon.tintColor = .white; contentView.backgroundColor = appearance?.separator
    isAccessibilityElement = true; accessibilityLabel = label
    accessibilityIdentifier = "attachment-keyboard-camera"; accessibilityTraits = .button
    activation = activate
  }
  func photo(item: [String: Any], order: Int?, manager: PHImageManager, appearance: ZoenKeyboardPanelAppearance?, loadThumbnail: Bool, preview openPreview: @escaping () -> Void, select: @escaping () -> Void) {
    cancel()
    guard let id = item["id"] as? String else { return }
    identifier = id; self.manager = manager; callback = select
    preview?.removeFromSuperview(); preview = nil; cameraIcon.isHidden = true; check.isHidden = false
    contentView.backgroundColor = appearance?.separator; image.image = nil
    isAccessibilityElement = false; image.isAccessibilityElement = true; image.accessibilityTraits = .button
    image.accessibilityLabel = item["name"] as? String; image.accessibilityIdentifier = "attachment-keyboard-photo-\(id)"
    image.activate = openPreview
    check.accessibilityLabel = item["name"] as? String; check.accessibilityIdentifier = "attachment-keyboard-select-\(id)"
    updateSelection(order: order, appearance: appearance)
    let seconds = max(0, item["duration"] as? Double ?? 0)
    duration.text = "\(Int(seconds) / 60):\(String(format: "%02d", Int(seconds) % 60))"; duration.isHidden = item["type"] as? String != "video"
    guard loadThumbnail, let asset = PHAsset.fetchAssets(withLocalIdentifiers: [String(id.dropFirst(5))], options: nil).firstObject else { return }
    let options = PHImageRequestOptions()
    options.deliveryMode = .opportunistic; options.resizeMode = .fast; options.isNetworkAccessAllowed = false
    request = manager.requestImage(for: asset, targetSize: CGSize(width: 360, height: 360), contentMode: .aspectFill, options: options) { [weak self] image, _ in
      Task { @MainActor [weak self] in guard let self, self.identifier == id else { return }; self.image.image = image }
    }
  }
  func updateSelection(order: Int?, appearance: ZoenKeyboardPanelAppearance?) {
    check.accessibilityTraits = order == nil ? .button : [.button, .selected]
    orderLabel.text = order.map(String.init) ?? ""
    let digits = orderLabel.text?.count ?? 0
    let size: CGFloat = digits < 2 ? 16 : (digits == 2 ? 15 : 13)
    let font = UIFont.systemFont(ofSize: size, weight: .medium)
    orderLabel.font = UIFont(descriptor: font.fontDescriptor.withDesign(.rounded) ?? font.fontDescriptor, size: size)
    badge.backgroundColor = order == nil ? UIColor.black.withAlphaComponent(0.25) : appearance?.accent
    badge.layer.borderWidth = order == nil ? 1.5 : 0
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    image.frame = contentView.bounds
    check.frame = CGRect(x: bounds.width - 44, y: 0, width: 44, height: 44)
    let pixel = 1 / max(1, traitCollection.displayScale)
    let inset = 0.75 + 2 - pixel
    badge.frame = CGRect(x: 12 + inset, y: 3 + inset, width: 29 - inset * 2, height: 29 - inset * 2)
    badge.layer.cornerRadius = badge.bounds.width / 2
    orderLabel.frame = badge.bounds
    cameraIcon.frame = CGRect(x: bounds.width - 36, y: 10, width: 26, height: 24)
    duration.frame = CGRect(x: 4, y: bounds.height - 22, width: max(0, bounds.width - 10), height: 18); duration.textAlignment = .right
  }
  func cancel() {
    if let request { manager?.cancelImageRequest(request) }
    request = nil; callback = nil; activation = nil; image.activate = nil; identifier = nil
  }
  override func accessibilityActivate() -> Bool {
    guard let activation else { return false }
    activation(); return true
  }
  override func prepareForReuse() { super.prepareForReuse(); cancel(); image.image = nil; preview?.removeFromSuperview(); preview = nil }
}

@MainActor
private final class AttachmentThumbnailView: UIImageView {
  var activate: (() -> Void)?
  override func accessibilityActivate() -> Bool {
    guard let activate else { return false }
    activate(); return true
  }
}

@MainActor
private final class AttachmentCameraPreview: UIView {
  private let preview = AVCaptureVideoPreviewLayer()
  private let capture = AttachmentPreviewCapture()
  private var retired = false
  private var running = false
  override init(frame: CGRect) {
    super.init(frame: frame)
    preview.videoGravity = .resizeAspectFill; preview.session = capture.session; layer.addSublayer(preview)
  }
  convenience init() { self.init(frame: .zero) }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
  override func layoutSubviews() { super.layoutSubviews(); preview.frame = bounds }
  func setRunning(_ value: Bool) {
    let next = value && !retired && AVCaptureDevice.authorizationStatus(for: .video) == .authorized
    guard next != running else { return }
    running = next; capture.setRunning(next)
  }
  func retire(completion: @escaping () -> Void) { retired = true; running = false; capture.stop(completion: completion) }
  override func didMoveToWindow() { super.didMoveToWindow(); if window == nil { setRunning(false) } }
}

// The capture queue serializes start/stop. Its completed stop, rather than a
// guessed animation duration, precedes dismissal and the external camera handoff.
private final class AttachmentPreviewCapture: @unchecked Sendable {
  let session = AVCaptureSession()
  private let queue = DispatchQueue(label: "xyz.zoen.attachment-camera-preview", qos: .userInitiated)
  private var configured = false
  func setRunning(_ value: Bool) { queue.async { [self] in update(value) } }
  func stop(completion: @escaping @MainActor () -> Void) {
    queue.async { [self] in
      update(false)
      DispatchQueue.main.async { completion() }
    }
  }
  private func update(_ value: Bool) {
    if value, !configured {
      guard AVCaptureDevice.authorizationStatus(for: .video) == .authorized,
            let device = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back),
            let input = try? AVCaptureDeviceInput(device: device), session.canAddInput(input) else { return }
      session.beginConfiguration(); session.sessionPreset = .medium; session.addInput(input); session.commitConfiguration(); configured = true
    }
    if value && configured && !session.isRunning { session.startRunning() }
    else if !value && session.isRunning { session.stopRunning() }
  }
}

// UIKit adaptation of MediaPickerGridSelectionGesture. Per-gesture IDs suppress
// duplicate requests only; PhotoSession remains the sole selection authority.
@MainActor
private final class GridSelectionGesture: UIPanGestureRecognizer {
  var itemAt: (CGPoint) -> (String, Bool)? = { _ in nil }
  var updateSelection: (String, Bool) -> Void = { _, _ in }
  var updateIsScrollEnabled: (Bool) -> Void = { _ in }
  var began: () -> Void = {}
  var sideInset: CGFloat = 0
  private var processing = false
  private var selecting = false
  private var initialLocation: CGPoint?
  init() { super.init(target: nil, action: nil); maximumNumberOfTouches = 1 }
  override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesBegan(touches, with: event)
    guard let touch = touches.first, numberOfTouches == 1 else { return }
    let location = touch.location(in: view)
    if location.x > sideInset { initialLocation = location } else { state = .failed }
  }
  override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesMoved(touches, with: event)
    guard let touch = touches.first, let initialLocation else { state = .failed; return }
    let location = touch.location(in: view), translation = CGPoint(x: location.x - initialLocation.x, y: location.y - initialLocation.y)
    var additionalLocation: CGPoint?
    if !processing {
      if abs(translation.y) > 5 { state = .failed }
      else if abs(translation.x) > 8 {
        processing = true; updateIsScrollEnabled(false); began()
        if let (_, selected) = itemAt(location) { selecting = !selected }
        additionalLocation = initialLocation
      }
    }
    if processing {
      if let additionalLocation, let (item, selected) = itemAt(additionalLocation), selected != selecting { updateSelection(item, selecting) }
      if let (item, selected) = itemAt(location), selected != selecting { updateSelection(item, selecting) }
    }
  }
  override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) { super.touchesEnded(touches, with: event); state = .failed; reset() }
  override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) { super.touchesCancelled(touches, with: event); state = .failed; reset() }
  override func reset() { super.reset(); processing = false; initialLocation = nil; updateIsScrollEnabled(true) }
}
