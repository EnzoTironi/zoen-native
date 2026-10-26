import AVFoundation
import ImageIO
import Photos
import UIKit

// TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838,
// GPL-2.0-or-later, GPLv3 selected. PhotoToolbar:428–855, GalleryInterface:173–456,
// 1357–1480, 1940–2203; PhotoStrip:78–102; SelectedList:948–1240,1397–1513.
// UIKit replaces ComponentFlow/ASDisplayNode. The caller retains the exact
// ordered selection, per-item captions, edit CAS, file leases and explicit Send.
// See THIRD_PARTY_NOTICES.md and docs/product/telegram-ios-gallery-sources.json.

@MainActor enum ZoenAttachmentGalleryMetrics {
  static func thumbnail(size: CGSize, scale: CGFloat) -> CGFloat {
    guard scale >= 2 else { return 106 }
    let side = max(size.width, size.height)
    let sizes: [(CGFloat, CGFloat, CGFloat)] = [(956,145,1),(926,141,1),(896,137,-1),
      (874,133,-1),(844,129,-1),(812,124,-1),(736,137,-1),(667,124,-1),(0,106,-1)]
    let value = sizes.first { side >= $0.0 } ?? (0,106,-1)
    return value.1 + value.2 / scale
  }
  static func dismiss(distance: CGFloat, velocity: CGFloat, height: CGFloat) -> Bool {
    distance.isFinite && velocity.isFinite && height > 0 &&
      (abs(distance) >= 128 || abs(velocity) >= 700)
  }
}

/// Shared native gallery: the Photos picker and the durable draft use this same
/// view. It emits intent only; no selection, edit or file copy is committed here.
@MainActor final class ZoenAttachmentGalleryContent: NSObject, UITextViewDelegate,
  UIScrollViewDelegate, UIGestureRecognizerDelegate {
  let root = GalleryRoot()
  var view: UIView { root }
  var scrollView: UIScrollView { pager }
  var animatedViews: [UIView] { [root, pager, strip, toolbar, captionPanel, counter, check, recipient] }
  var hasKeyboardFocus: Bool { caption.isFirstResponder }
  var didBeginEditing: (() -> Void)?
  var transitionSourceView: ((String) -> UIView?)?
  let send: (String,[String:Any]) -> Void
  let pager = UIScrollView()
  let stripLayout = UICollectionViewFlowLayout()
  lazy var strip = UICollectionView(frame: .zero, collectionViewLayout: stripLayout)
  let toolbar = UIView()
  let centerGlass = ZoenTelegramAttachmentGlassView()
  let closeGlass = ZoenTelegramAttachmentGlassView()
  let sendGlass = ZoenTelegramAttachmentGlassView()
  let captionPanel = ZoenTelegramAttachmentGlassView()
  let close = UIButton(type: .custom)
  let edit = UIButton(type: .custom)
  let add = UIButton(type: .custom)
  let sendButton = UIButton(type: .custom)
  let check = GalleryCheckButton()
  let counter = GalleryCounterButton()
  let recipient = UILabel()
  let recipientArrow = UIImageView()
  let caption = UITextView()
  let placeholder = UILabel()
  let errorLabel = UILabel()
  let retry = UIButton(type: .system)
  let conversationCaption = UITextView()
  let position = UILabel()
  var pages: [String:GalleryMediaView] = [:]
  var items: [GalleryItem] = []
  var selected: [String] = []
  var previewId = ""
  var revision = ""
  var source = "photos"
  var labels: [String:String] = [:]
  var appearance: ZoenKeyboardPanelAppearance?
  var reviewingSelection = false
  var retired = false
  var visible = true
  var needsRebuild = false
  var heroView:UIImageView?
  var heroDelegate:CAAnimationDelegate?
  var heroTicket:UUID?
  var sourceFrame:CGRect?
  var sourceId:String?
  var busy = false
  var canEdit = false
  var canAdd = false
  var canRemove = false
  var canSend = false
  var canReorder = false
  var captionEditable = false
  var captionItem = ""
  var safe = UIEdgeInsets.zero
  var keyboardHeight: CGFloat = 0
  var railVisible = false
  var controlsHidden = false
  var lastSize = CGSize.zero
  var pagingRevision: String?
  var reorderRevision: String?
  var dragId: String?
  lazy var dismissal = UIPanGestureRecognizer(target: self, action: #selector(dismissPan(_:)))
  lazy var reorder = UILongPressGestureRecognizer(target: self, action: #selector(reorderGesture(_:)))

  init(send: @escaping (String,[String:Any]) -> Void) {
    self.send = send
    super.init()
    root.backgroundColor = .black
    root.accessibilityIdentifier = "attachment-gallery-surface"
    root.onLayout = { [weak self] in self?.reflow() }
    root.accessibilityEscape = { [weak self] in self?.emit("back") }
    pager.isPagingEnabled = true; pager.alwaysBounceHorizontal = true
    // UIScrollView's delay can let its pan win before the scrubber receives
    // touchesBegan. Deliver immediately; UIKit already preserves UIControl
    // tracking through touchesShouldCancelInContentView's default contract.
    pager.delaysContentTouches = false
    pager.showsHorizontalScrollIndicator = false; pager.delegate = self
    pager.contentInsetAdjustmentBehavior = .never
    pager.accessibilityIdentifier = "attachment-gallery-pager"
    for child in [pager,toolbar,captionPanel,counter,check,recipient,recipientArrow,errorLabel,retry] { root.addSubview(child) }
    conversationCaption.textColor = .white; conversationCaption.font = .systemFont(ofSize:17)
    conversationCaption.backgroundColor = .clear; conversationCaption.isEditable = false
    conversationCaption.textContainerInset = .zero; conversationCaption.isScrollEnabled = true
    position.textColor = .white; position.font = .systemFont(ofSize:17)
    root.addSubview(conversationCaption); root.addSubview(position)
    conversationCaption.accessibilityIdentifier = "conversation-gallery-caption"
    position.textAlignment = .center
    position.accessibilityIdentifier = "conversation-gallery-position"
    toolbar.addSubview(centerGlass); toolbar.addSubview(closeGlass); toolbar.addSubview(sendGlass)
    closeGlass.contentView.addSubview(close); sendGlass.contentView.addSubview(sendButton)
    centerGlass.contentView.addSubview(edit); root.addSubview(add)
    for child in [caption,placeholder] { captionPanel.contentView.addSubview(child) }
    close.setImage(ZoenTelegramAttachmentGlyph.image(.back), for: .normal)
    sendButton.setImage(ZoenTelegramAttachmentGlyph.image(.send), for: .normal)
    edit.setImage(ZoenTelegramAttachmentGlyph.image(.pencil, pointSize: CGSize(width:28,height:28)), for: .normal)
    // The source selection circle also removes draft media. Keep a single
    // accessible control for that action, rather than an invisible toolbar copy.
    add.setImage(ZoenTelegramAttachmentGlyph.image(.camera),for:.normal)
    for button in [close,sendButton,edit,add] {
      button.tintColor = .white; button.setTitleColor(.white,for:.normal)
      button.addTarget(self,action:#selector(highlight(_:)),for:.touchDown)
      button.addTarget(self,action:#selector(unhighlight(_:)),for:[.touchUpInside,.touchUpOutside,.touchCancel])
    }
    close.accessibilityIdentifier = "attachment-approval-close"
    sendButton.accessibilityIdentifier = "attachment-approval-send"
    edit.accessibilityIdentifier = "attachment-approval-edit"
    add.accessibilityIdentifier = "attachment-approval-add"
    close.addAction(UIAction { [weak self] _ in self?.back() },for:.touchUpInside)
    sendButton.addAction(UIAction { [weak self] _ in self?.sendPressed() },for:.touchUpInside)
    edit.addAction(UIAction { [weak self] _ in self?.editPressed() },for:.touchUpInside)
    add.showsMenuAsPrimaryAction=true
    check.addAction(UIAction { [weak self] _ in guard let self else{return}; self.emit(self.source == "draft" ? "remove":"select",id:self.previewId) },for:.touchUpInside)
    counter.accessibilityIdentifier = "attachment-approval-selection-count"
    counter.addAction(UIAction { [weak self] _ in self?.toggleRail() },for:.touchUpInside)
    caption.font = .systemFont(ofSize:17); caption.textColor = .white
    caption.backgroundColor = .clear; caption.delegate = self
    caption.textContainerInset = UIEdgeInsets(top:10,left:10,bottom:10,right:10)
    caption.accessibilityIdentifier = "attachment-approval-caption"
    placeholder.font = .systemFont(ofSize:17); placeholder.textColor = UIColor.white.withAlphaComponent(0.5)
    placeholder.isUserInteractionEnabled = false
    recipient.font = .boldSystemFont(ofSize:13); recipient.textColor = UIColor.white.withAlphaComponent(0.6)
    recipient.lineBreakMode = .byTruncatingTail
    recipientArrow.image = ZoenTelegramAttachmentGlyph.image(.recipientArrow)
    recipientArrow.alpha = 0.6
    errorLabel.font = .systemFont(ofSize:13); errorLabel.textColor = .white
    errorLabel.numberOfLines = 3; errorLabel.accessibilityTraits = .updatesFrequently
    retry.addAction(UIAction { [weak self] _ in self?.emit("retry") },for:.touchUpInside)
    strip.backgroundColor = .clear; strip.dataSource = self; strip.delegate = self
    strip.register(GalleryStripCell.self,forCellWithReuseIdentifier:"strip")
    strip.showsHorizontalScrollIndicator = false; strip.showsVerticalScrollIndicator = false
    strip.contentInsetAdjustmentBehavior = .never
    strip.accessibilityIdentifier = "attachment-approval-selection-rail"
    stripLayout.minimumLineSpacing = 4; stripLayout.minimumInteritemSpacing = 4
    root.addSubview(strip)
    dismissal.delegate = self; pager.addGestureRecognizer(dismissal)
    reorder.minimumPressDuration = 0.3; reorder.delegate = self; strip.addGestureRecognizer(reorder)
  }


  func conceal() {root.isHidden=true;retire()}
  func retire() {
    guard !retired else{return}
    retired=true;finishHero();root.isUserInteractionEnabled=false;caption.resignFirstResponder();caption.delegate=nil
    didBeginEditing=nil;cancelReorder();pages.values.forEach{$0.retire()}
    strip.visibleCells.forEach{($0 as? GalleryStripCell)?.retire()}
  }
}
