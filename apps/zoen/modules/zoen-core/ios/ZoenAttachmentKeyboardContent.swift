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
  let root = AttachmentRootView()
  var view: UIView { root }
  var attachmentScrollView: UIScrollView {
    if galleryVisible { return gallery.scrollView }
    if screen == "albums" { return albumMenu.scrollView }
    if activeCategory != "photos" { return fileLanding.scrollView }
    return grid
  }
  var attachmentHasKeyboardFocus: Bool { caption.isFirstResponder || (galleryVisible && gallery.hasKeyboardFocus) || (activeCategory != "photos" && fileLanding.hasKeyboardFocus) }
  var attachmentHandlesVerticalGesture: Bool { screen == "preview" || screen == "albums" }
  var attachmentPrefersFullScreen: Bool { ZoenAttachmentGalleryLayout.prefersFullScreen(screen: screen) }
  var attachmentDidBeginEditing: (() -> Void)?
  var attachmentPresentationChanged: (() -> Void)?
  var attachmentAnimatedViews: [UIView] {
    [grid, header, headerLeftSurface, headerRightSurface, footer, categoryLens, categories, unselectedCategories, selectedCategories, captionPanel,
     captionBackground, caption, captionPlaceholder, sendButton, countLabel,
     close, albums, albumArrow, selectedButton, review, manage, message, settings, retry, activity]
      + buttons.map(\.1) + selectedButtons + gallery.animatedViews + albumMenu.animatedViews + fileLanding.animatedViews
  }
  let send: (String, [String: Any]) -> Void
  let layout = PhotoLayout()
  lazy var grid = UICollectionView(frame: .zero, collectionViewLayout: layout)
  let header = UIView()
  let headerLeftSurface = ZoenTelegramAttachmentGlassView()
  let headerRightSurface = ZoenTelegramAttachmentGlassView()
  let footer = ZoenTelegramAttachmentGlassView()
  let categoryLens = LiquidLensView(kind: .noContainer)
  let categories = UIScrollView()
  let unselectedCategories = UIView()
  let selectedCategories = UIView()
  let captionPanel = UIView()
  let captionBackground = ZoenTelegramAttachmentGlassView()
  let caption = UITextView()
  let captionPlaceholder = UILabel()
  let sendButton = UIButton(type: .system)
  let countLabel = UILabel()
  let close = UIButton(type: .system)
  let albums = AttachmentAlbumTitleButton(type: .custom)
  var albumArrow: UIImageView { albums.arrow }
  let selectedButton = UIButton(type: .system)
  let review = UIButton(type: .system)
  let manage = UIButton(type: .system)
  let message = UILabel()
  let settings = UIButton(type: .system)
  let retry = UIButton(type: .system)
  let activity = UIActivityIndicatorView(style: .medium)
  let manager = PHCachingImageManager()
  let selectionGesture = GridSelectionGesture()
  let cameraPreview = AttachmentCameraPreview()
  lazy var gallery = ZoenAttachmentGalleryContent { [weak self] action, payload in
    guard let self, !self.retired, !self.busy else { return }
    self.send(action, payload)
  }
  lazy var albumMenu = ZoenAttachmentAlbumMenu(select: { [weak self] id in
    self?.emit("album", id: id)
  }, dismiss: { [weak self] in self?.emit("back") }, details: { [weak self] id in self?.emit("albumDetails", id: id) })
  lazy var fileLanding = ZoenAttachmentFileContent { [weak self] action, payload in
    guard let self, !self.retired, !self.busy, self.activeCategory != "photos" else { return }
    self.send(action, payload)
  }
  var recentSelected: [String] = []
  var items: [[String: Any]] = []
  var selected: [String] = []
  var labels: [String: String] = [:]
  var buttons: [(String, AttachmentCategoryButton)] = []
  var selectedButtons: [AttachmentCategoryButton] = []
  var categoriesKey = ""
  private var gestureSent = Set<String>()
  var busy = false
  var retired = false
  var granted = false
  var limited = false
  var loading = false
  var hasMore = false
  var hasError = false
  var hasAudio = false
  var showCamera = false
  var captionEditable = false
  var captionEnabled = false
  var captionSending = false
  var selecting = false
  var selectionAnimation = 0
  var selectionAnimating = false
  var tabSelection: (startX: CGFloat, currentX: CGFloat, id: String, isLifted: Bool)?
  var lensIsLifted = false
  var lensGeneration = 0
  private lazy var tabSelectionRecognizer = TabSelectionRecognizer(target: self, action: #selector(selectTab(_:)))
  var expansion: CGFloat = 0
  var keyboardHeight: CGFloat = 0
  var additionalBottomInset: CGFloat = 0
  var safeArea = UIEdgeInsets.zero
  var requestedCount: Int?
  var sendOptionsRevision: String?
  var sendUngroupedOptionEnabled = false
  var sendAsFileOptionEnabled = false
  var sendOptionActions: [ZoenAttachmentReferenceMenu.Action] = []
  var referenceMenu: ZoenAttachmentReferenceMenu?
  var albumId: String?
  var screen = "grid"
  var activeCategory = "photos"
  var galleryVisible: Bool { screen == "preview" || screen == "selected" }
  var frozenGallery: UIView?
  var appearance: ZoenKeyboardPanelAppearance?
  static let categoryGlyphs: [String: ZoenTelegramAttachmentGlyph] = ["photos": .gallery, "document": .file,
    "audio": .audio, "location": .location, "poll": .poll, "contact": .contact, "article": .article]

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
      guard let self, !self.retired, !self.busy else { return }
      if self.activeCategory == "photos" { self.showSendOptions() }
      else if self.fileLanding.searchAvailable { self.send("recentSearch", [:]) }
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
    review.showsMenuAsPrimaryAction = false
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


  func retire() {
    guard !retired else { return }
    retired = true; root.isUserInteractionEnabled = false
    referenceMenu?.retire(); referenceMenu = nil; sendOptionActions.removeAll()
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
  enum Failure: Error { case invalid }
}
