import AVKit
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
  private let root = GalleryRoot()
  var view: UIView { root }
  var scrollView: UIScrollView { selectedMode ? selection : pager }
  var animatedViews: [UIView] { [root, pager, selection, toolbar, captionPanel, counter, check, recipient] }
  var hasKeyboardFocus: Bool { caption.isFirstResponder }
  var didBeginEditing: (() -> Void)?
  var transitionSourceView: ((String) -> UIView?)?
  private let send: (String,[String:Any]) -> Void
  private let pager = UIScrollView()
  private let selection = UIScrollView()
  private let stripLayout = UICollectionViewFlowLayout()
  private lazy var strip = UICollectionView(frame: .zero, collectionViewLayout: stripLayout)
  private let toolbar = UIView()
  private let centerGlass = ZoenTelegramAttachmentGlassView()
  private let closeGlass = ZoenTelegramAttachmentGlassView()
  private let sendGlass = ZoenTelegramAttachmentGlassView()
  private let captionPanel = ZoenTelegramAttachmentGlassView()
  private let close = UIButton(type: .custom)
  private let edit = UIButton(type: .custom)
  private let add = UIButton(type: .custom)
  private let sendButton = UIButton(type: .custom)
  private let check = GalleryCheckButton()
  private let counter = GalleryCounterButton()
  private let recipient = UILabel()
  private let recipientArrow = UIImageView()
  private let caption = UITextView()
  private let placeholder = UILabel()
  private let errorLabel = UILabel()
  private let retry = UIButton(type: .system)
  private let title = UILabel()
  private let dragHint = UILabel()
  private var pages: [String:GalleryMediaView] = [:]
  private var selectedViews: [String:GallerySelectionTile] = [:]
  private var backgrounds: [UIView] = []
  private var items: [GalleryItem] = []
  private var selected: [String] = []
  private var previewId = ""
  private var revision = ""
  private var source = "photos"
  private var labels: [String:String] = [:]
  private var appearance: ZoenKeyboardPanelAppearance?
  private var selectedMode = false
  private var retired = false
  private var visible = true
  private var needsRebuild = false
  private var heroView:UIImageView?
  private var heroDelegate:CAAnimationDelegate?
  private var heroTicket:UUID?
  private var sourceFrame:CGRect?
  private var sourceId:String?
  private var busy = false
  private var canEdit = false
  private var canAdd = false
  private var canRemove = false
  private var canSend = false
  private var canReorder = false
  private var captionEditable = false
  private var captionItem = ""
  private var safe = UIEdgeInsets.zero
  private var keyboardHeight: CGFloat = 0
  private var railVisible = false
  private var controlsHidden = false
  private var lastSize = CGSize.zero
  private var pagingRevision: String?
  private var reorderRevision: String?
  private var dragId: String?
  private var dragSnapshot: UIView?
  private var dragOrigin = CGPoint.zero
  private lazy var dismissal = UIPanGestureRecognizer(target: self, action: #selector(dismissPan(_:)))
  private lazy var reorder = UILongPressGestureRecognizer(target: self, action: #selector(reorderGesture(_:)))

  init(send: @escaping (String,[String:Any]) -> Void) {
    self.send = send
    super.init()
    root.backgroundColor = .black
    root.accessibilityIdentifier = "attachment-gallery-surface"
    root.onLayout = { [weak self] in self?.reflow() }
    root.accessibilityEscape = { [weak self] in self?.emit("back") }
    pager.isPagingEnabled = true; pager.alwaysBounceHorizontal = true
    pager.showsHorizontalScrollIndicator = false; pager.delegate = self
    pager.contentInsetAdjustmentBehavior = .never
    pager.accessibilityIdentifier = "attachment-gallery-pager"
    selection.alwaysBounceVertical = true; selection.contentInsetAdjustmentBehavior = .never
    selection.keyboardDismissMode = .onDrag
    selection.accessibilityIdentifier = "attachment-gallery-selected-list"
    for child in [pager,selection,toolbar,captionPanel,counter,check,recipient,recipientArrow,errorLabel,retry] { root.addSubview(child) }
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
    title.font = .systemFont(ofSize:13); title.textColor = .white; title.textAlignment = .center
    dragHint.font = .systemFont(ofSize:13); dragHint.textColor = .white; dragHint.textAlignment = .center
    selection.addSubview(title); selection.addSubview(dragHint)
    strip.backgroundColor = .clear; strip.dataSource = self; strip.delegate = self
    strip.register(GalleryStripCell.self,forCellWithReuseIdentifier:"strip")
    strip.showsHorizontalScrollIndicator = false; strip.showsVerticalScrollIndicator = false
    strip.contentInsetAdjustmentBehavior = .never
    strip.accessibilityIdentifier = "attachment-approval-selection-rail"
    stripLayout.minimumLineSpacing = 4; stripLayout.minimumInteritemSpacing = 4
    root.addSubview(strip)
    dismissal.delegate = self; pager.addGestureRecognizer(dismissal)
    reorder.minimumPressDuration = 0.3; reorder.delegate = self; selection.addGestureRecognizer(reorder)
  }

  func update(model: [String:Any], appearance: ZoenKeyboardPanelAppearance) throws {
    guard !retired, let source = model["source"] as? String, ["photos","draft"].contains(source),
      let incoming = model["items"] as? [[String:Any]], incoming.count <= 20000,
      let selected = model["selected"] as? [String], selected.count <= 10,
      Set(selected).count == selected.count,
      let revision = model["revision"] as? String, !revision.isEmpty, revision.utf8.count <= 128 else { throw GalleryFailure.invalid }
    var union = incoming
    let existing = Set(incoming.compactMap { $0["id"] as? String })
    union += (model["selectedItems"] as? [[String:Any]] ?? []).filter { !existing.contains($0["id"] as? String ?? "") }
    let parsed = try union.map { try GalleryItem(value:$0,source:source) }
    guard parsed.count <= 20010, Set(parsed.map(\.id)).count == parsed.count,
      selected.allSatisfy({ id in parsed.contains { $0.id == id } }) else { throw GalleryFailure.invalid }
    let preview = model["previewId"] as? String ?? selected.first ?? parsed.first?.id ?? ""
    guard preview.isEmpty || parsed.contains(where: { $0.id == preview }) else { throw GalleryFailure.invalid }
    let captionModel = model["caption"] as? [String:Any] ?? [:]
    let text = captionModel["value"] as? String ?? ""
    guard text.utf16.count <= 4096, !text.contains("\0") else { throw GalleryFailure.invalid }
    let newSelectedMode = model["screen"] as? String == "selected"
    let mediaChanged = self.source != source || self.items != parsed || self.previewId != preview || selectedMode != newSelectedMode
    if self.revision != revision { cancelReorder() }
    let itemChanged = captionItem != preview
    self.source = source; self.items = parsed; self.selected = selected
    self.previewId = preview; self.revision = revision; self.appearance = appearance
    selectedMode = newSelectedMode; busy = model["busy"] as? Bool ?? false
    if busy {pages.values.forEach{$0.stop()};caption.resignFirstResponder()}
    root.backgroundColor = selectedMode ? appearance.background : .black
    root.overrideUserInterfaceStyle = selectedMode ? (appearance.isDark ? .dark : .light) : .dark
    caption.keyboardAppearance = selectedMode ? (appearance.isDark ? .dark : .light) : .dark
    labels = model["labels"] as? [String:String] ?? [:]
    canEdit = model["canEdit"] as? Bool == true
    canAdd = source == "draft" && model["canAdd"] as? Bool == true
    canRemove = source == "draft" && model["canRemove"] as? Bool == true
    canReorder = model["canReorder"] as? Bool ?? (source == "photos")
    canSend = captionModel["sendEnabled"] as? Bool ?? (!selected.isEmpty || (source == "photos" && !preview.isEmpty))
    captionEditable = captionModel["editable"] as? Bool == true && !busy
    caption.isEditable = captionEditable
    if itemChanged || (!caption.isFirstResponder && caption.text != text) { caption.text = text }
    captionItem = preview
    placeholder.text = captionModel["placeholder"] as? String ?? ""
    placeholder.isHidden = !caption.text.isEmpty
    recipient.text = model["recipient"] as? String ?? ""
    close.accessibilityLabel = labels["back"] ?? labels["close"]
    sendButton.accessibilityLabel = captionModel["sendLabel"] as? String
    edit.accessibilityLabel = labels["edit"]; add.accessibilityLabel = labels["add"]
    check.accessibilityLabel = labels["select"]
    counter.accessibilityLabel = labels["review"]
    counter.menu=UIMenu(children:[UIAction(title:labels["messagePreview"] ?? ""){[weak self] _ in self?.emit("selected")}])
    add.menu=UIMenu(children:[("photo","photos"),("camera","camera"),("document","files")].map{kind,label in
      UIAction(title:labels[label] ?? ""){[weak self] _ in self?.emit("add",payload:["kind":kind])}
    })
    title.text = labels["messagePreview"] ?? ""; dragHint.text = labels["dragToReorder"] ?? ""
    errorLabel.text = model["errorText"] as? String
    retry.setTitle(labels["retry"],for:.normal)
    if let label = model["errorText"] as? String, !label.isEmpty { errorLabel.isHidden = false; retry.isHidden = false }
    else { errorLabel.isHidden = true; retry.isHidden = true }
    sendGlass.contentView.backgroundColor = appearance.accent
    sendGlass.contentView.layer.cornerRadius = 22
    root.isUserInteractionEnabled = true
    if mediaChanged {
      controlsHidden = false
      for view in [toolbar,captionPanel,counter,check,recipient,recipientArrow,strip] {
        view.layer.removeAnimation(forKey:"opacity");view.alpha = view === recipientArrow ? 0.6 : 1
      }
      if visible {rebuildMedia()} else {needsRebuild=true}
    }
    updateControls(); strip.reloadData(); reflow()
  }

  func updatePresentation(keyboardHeight:CGFloat,safeArea:UIEdgeInsets) {
    guard !retired else { return }
    self.keyboardHeight = max(0,keyboardHeight); safe = safeArea; reflow()
  }
  func setVisible(_ visible:Bool) {
    guard !retired,self.visible != visible else{return}
    self.visible=visible;root.isHidden = !visible
    if visible {
      if needsRebuild {rebuildMedia();needsRebuild=false};reflow()
      if !selectedMode,source == "photos",let tile=transitionSourceView?(previewId),let window=tile.window,
        let image=(tile as? UIImageView)?.image {
        sourceFrame=tile.convert(tile.bounds,to:window);sourceId=previewId
        let ticket=UUID();heroTicket=ticket
        Task{@MainActor [weak self] in
          await Task.yield()
          guard let self,heroTicket==ticket,!retired,self.visible,!selectedMode,
            let frame=sourceFrame,let currentWindow=root.window else{return}
          animateHero(image:image,from:root.convert(frame,from:currentWindow),to:imageFrame(),entering:true,ticket:ticket){[weak self] in self?.finishHero()}
        }
      }
    }
    else {
      caption.resignFirstResponder();cancelReorder();finishHero()
      pages.values.forEach{$0.retire()};selectedViews.values.forEach{$0.retire()}
      strip.visibleCells.forEach{($0 as? GalleryStripCell)?.retire()};needsRebuild=true
    }
  }
  private func updateControls() {
    let aligned = previewIsVisible
    pager.isScrollEnabled = !busy && pages[previewId]?.isZoomed != true
    close.isEnabled = !busy; edit.isEnabled = !busy && canEdit && aligned
    add.isEnabled = !busy && canAdd
    sendButton.isEnabled = !busy && canSend && aligned; sendGlass.alpha = sendButton.isEnabled ? 1 : 0.2
    check.isEnabled = !busy && aligned && (source == "photos" || canRemove) && !hasKeyboardFocus
    check.isHidden = selectedMode || hasKeyboardFocus || (source == "draft" && !canRemove)
    check.accessibilityIdentifier = source == "draft" ? "attachment-approval-remove":"attachment-approval-select"
    check.accessibilityLabel = labels[source == "draft" ? "remove":"select"]
    let order = selected.firstIndex(of:previewId).map { $0 + 1 }
    check.update(order:order,accent:appearance?.accent ?? .systemBlue)
    counter.update(count:selected.count,expanded:railVisible)
    counter.isHidden = selectedMode || selected.isEmpty || hasKeyboardFocus
    recipient.isHidden = hasKeyboardFocus || (recipient.text ?? "").isEmpty
    recipientArrow.isHidden = recipient.isHidden
    edit.isHidden = !canEdit || hasKeyboardFocus || selectedMode
    add.isHidden = !canAdd || hasKeyboardFocus || selectedMode
    toolbar.isHidden = controlsHidden
    captionPanel.isHidden = controlsHidden
    strip.isHidden = controlsHidden || !railVisible || hasKeyboardFocus || selectedMode
    captionPanel.isUserInteractionEnabled = !controlsHidden
  }
  private var previewIsVisible:Bool {
    if selectedMode{return true}
    guard !pager.isDragging,!pager.isDecelerating,pager.bounds.width>0,
      let index=items.firstIndex(where:{$0.id==previewId}) else{return false}
    return abs(pager.contentOffset.x-CGFloat(index)*pager.bounds.width)<1/max(1,root.traitCollection.displayScale)
  }
  private func rebuildMedia() {
    for page in pages.values { page.retire(); page.removeFromSuperview() }; pages.removeAll()
    for tile in selectedViews.values { tile.retire(); tile.removeFromSuperview() }; selectedViews.removeAll()
    for background in backgrounds { background.removeFromSuperview() }; backgrounds.removeAll()
    pager.isHidden = selectedMode; selection.isHidden = !selectedMode
    if selectedMode {
      for item in orderedItems {
        let tile = GallerySelectionTile(item:item)
        tile.open = { [weak self] in self?.emit("preview",id:item.id) }
        selectedViews[item.id] = tile; selection.addSubview(tile)
      }
    } else {
      guard let index = items.firstIndex(where:{$0.id == previewId}) else{return}
      for position in max(0,index-1)...min(items.count-1,index+1) {
        let item = items[position]
        let page = GalleryMediaView(item:item,allowNetwork:position == index)
        page.active = position == index
        page.toggle = { [weak self] in self?.toggleControls() }
        page.zoomChanged = { [weak self] _ in self?.updateControls() }
        pages[item.id] = page; pager.addSubview(page)
      }
    }
  }
  private var orderedItems: [GalleryItem] { selected.compactMap { id in items.first { $0.id == id } } }

  private func reflow() {
    let size = root.bounds.size
    guard size.width > 0,size.height > 0 else{return}
    updateControls()
    let landscape = size.width > size.height
    let scale = max(1,root.traitCollection.displayScale)
    let toolbarWidth: CGFloat = landscape ? 49 : size.width
    let toolbarHeight: CGFloat = landscape ? size.height : 49
    let leftSide = safe.left >= safe.right
    let bottom = keyboardHeight > 0 ? keyboardHeight : safe.bottom
    let toolbarY = max(0,size.height-bottom-49)
    toolbar.frame = landscape && !hasKeyboardFocus
      ? CGRect(x:leftSide ? safe.left : size.width-safe.right-49,y:0,width:49,height:size.height)
      : CGRect(x:0,y:toolbarY,width:size.width,height:49)
    let vertical = landscape && !hasKeyboardFocus
    let closeFrame = vertical ? CGRect(x:0,y:toolbarHeight-44,width:44,height:44) : CGRect(x:26,y:0,width:44,height:44)
    let sendFrame = vertical ? CGRect(x:0,y:0,width:44,height:44) : CGRect(x:toolbarWidth-46-26,y:0,width:46,height:44)
    closeGlass.frame = closeFrame; sendGlass.frame = sendFrame
    close.frame = closeGlass.bounds; sendButton.frame = sendGlass.bounds
    for glass in [closeGlass,sendGlass] { glass.update(isDark:selectedMode ? appearance?.isDark ?? true : true,cornerRadius:22,isInteractive:true) }
    let tools = [edit].filter { !$0.isHidden }
    let length = CGFloat(tools.count)*44 + CGFloat(max(0,tools.count-1))*10
    centerGlass.isHidden = tools.isEmpty
    centerGlass.frame = vertical ? CGRect(x:0,y:floor((toolbarHeight-length)/2),width:44,height:length)
      : CGRect(x:floor((toolbarWidth-length)/2),y:0,width:length,height:44)
    centerGlass.update(isDark:selectedMode ? appearance?.isDark ?? true : true,cornerRadius:22,isInteractive:true)
    for (index,button) in tools.enumerated() { button.frame = CGRect(x:vertical ? 0 : CGFloat(index)*54,y:vertical ? CGFloat(index)*54 : 0,width:44,height:44) }
    let captionWidth = max(0,size.width-safe.left-safe.right-(vertical ? 65 : 0)-16)
    let textSize = caption.sizeThatFits(CGSize(width:max(1,captionWidth),height:.greatestFiniteMagnitude))
    let captionHeight = min(110,max(40,textSize.height))
    let captionY = max(safe.top+8,size.height-bottom-49-10-captionHeight)
    captionPanel.frame = CGRect(x:safe.left+8+(vertical && leftSide ? 57 : 0),y:captionY,width:captionWidth,height:captionHeight)
    captionPanel.update(isDark:selectedMode ? appearance?.isDark ?? true : true,cornerRadius:20,isInteractive:true)
    caption.frame = captionPanel.bounds; caption.isScrollEnabled = textSize.height>110
    placeholder.frame = CGRect(x:15,y:0,width:max(0,captionWidth-30),height:40)
    let thumbnail = ZoenAttachmentGalleryMetrics.thumbnail(size:size,scale:scale)
    stripLayout.itemSize = CGSize(width:thumbnail,height:thumbnail)
    stripLayout.scrollDirection = landscape ? .vertical : .horizontal
    strip.contentInset = UIEdgeInsets(top:4,left:4,bottom:4,right:4)
    if landscape {
      counter.frame = CGRect(x:leftSide ? safe.left+50 : size.width-safe.right-113,y:6,width:64,height:38)
      strip.frame = CGRect(x:leftSide ? safe.left+115 : size.width-safe.right-115-thumbnail-8,y:4,width:thumbnail+8,height:size.height-8)
    } else {
      counter.frame = CGRect(x:size.width-safe.right-64,y:captionY-50,width:64,height:38)
      strip.frame = CGRect(x:4,y:captionY-64-thumbnail-8,width:size.width-8-safe.right,height:thumbnail+8)
    }
    recipientArrow.frame = CGRect(x:safe.left+14,y:safe.top+16,width:recipientArrow.image?.size.width ?? 9,height:recipientArrow.image?.size.height ?? 9)
    recipient.frame = CGRect(x:recipientArrow.frame.maxX+6,y:recipientArrow.frame.minY-2,width:max(0,min(size.width,size.height)-150),height:17)
    check.frame = CGRect(x:size.width-safe.right-44,y:safe.top+5,width:40,height:40)
    add.frame = CGRect(x:size.width-safe.right-96,y:captionY-49,width:44,height:44)
    if add.superview !== root {root.addSubview(add)}
    pager.frame = root.bounds; pager.contentSize = CGSize(width:size.width*CGFloat(items.count),height:size.height)
    let changedSize = lastSize != size; lastSize = size
    for (position,item) in items.enumerated() { pages[item.id]?.frame = CGRect(x:CGFloat(position)*size.width,y:0,width:size.width,height:size.height) }
    if let index = items.firstIndex(where:{$0.id == previewId}), (!pager.isDragging && !pager.isDecelerating) || changedSize {
      pager.setContentOffset(CGPoint(x:CGFloat(index)*size.width,y:0),animated:false)
    }
    selection.frame = root.bounds
    layoutSelected(size:CGSize(width:size.width,height:captionY-10))
    errorLabel.frame = CGRect(x:16,y:captionY-76,width:max(0,size.width-32),height:50)
    retry.frame = CGRect(x:size.width-100,y:captionY-26,width:84,height:24)
    updateControls()
  }

  private func layoutSelected(size:CGSize) {
    guard selectedMode else{return}
    let width = max(1,min(320,size.width-safe.left-safe.right-68))
    var y = safe.top+60
    title.frame = CGRect(x:0,y:safe.top+28,width:size.width,height:20)
    let selected = orderedItems
    for (index,start) in stride(from:0,to:selected.count,by:10).enumerated() {
      let group = Array(selected[start..<min(start+10,selected.count)])
      let sizes = group.map { $0.resolvedDimensions }
      let layout: ([(CGRect,ZoenAttachmentMosaicPosition)],CGSize)
      if group.count>1 { layout = zoenAttachmentMosaicLayout(maxSize:CGSize(width:width,height:width),itemSizes:sizes,spacing:1,fillWidth:true,screenScale:max(1,root.traitCollection.displayScale)) }
      else if let first = sizes.first {
        let height = first.width>first.height ? floor(width*first.height/max(1,first.width)) : width
        let s = CGSize(width:width,height:max(1,height)); layout = ([(CGRect(origin:.zero,size:s),[.top,.bottom,.left,.right])],s)
      } else {continue}
      let origin = CGPoint(x:safe.left+floor((size.width-safe.left-safe.right-layout.1.width)/2),y:y)
      let backing: UIView
      if backgrounds.indices.contains(index) { backing = backgrounds[index] }
      else { backing = UIView(); backgrounds.append(backing); selection.insertSubview(backing,belowSubview:title) }
      backing.frame = CGRect(origin:origin,size:layout.1).insetBy(dx:-5,dy:-2).offsetBy(dx:3,dy:0)
      backing.backgroundColor = appearance?.accent.withAlphaComponent(0.25)
      backing.layer.cornerRadius = 16
      for (item,entry) in zip(group,layout.0) {
        guard let tile = selectedViews[item.id] else{continue}
        tile.frame = entry.0.offsetBy(dx:origin.x,dy:origin.y)
        var corners: CACornerMask = []
        if entry.1.contains([.top,.left]) {corners.insert(.layerMinXMinYCorner)}
        if entry.1.contains([.top,.right]) {corners.insert(.layerMaxXMinYCorner)}
        if entry.1.contains([.bottom,.left]) {corners.insert(.layerMinXMaxYCorner)}
        if entry.1.contains([.bottom,.right]) {corners.insert(.layerMaxXMaxYCorner)}
        tile.layer.cornerRadius = 16; tile.layer.maskedCorners = corners
      }
      y += layout.1.height+6
    }
    dragHint.frame = CGRect(x:16,y:y+9,width:size.width-32,height:20)
    dragHint.isHidden = selected.count<2
    selection.contentSize = CGSize(width:size.width,height:y+60)
    selection.contentInset.bottom = max(0,root.bounds.height-size.height)
  }

  private func emit(_ action:String,id:String?=nil,payload:[String:Any]=[:]) {
    guard !retired,visible,!busy else{return}
    if let id, !items.contains(where:{$0.id == id}) {return}
    var value = payload; if let id {value["id"] = id}
    value["revision"] = revision
    send(action,value)
  }
  private func back() {
    guard !retired,!busy else{return}
    if caption.isFirstResponder {caption.resignFirstResponder();return}
    guard heroTicket==nil else{return}
    if source=="photos",sourceId==previewId,let sourceFrame,let window=root.window,
      let image=pages[previewId]?.image.image {
      let ticket=UUID();heroTicket=ticket
      animateHero(image:image,from:imageFrame().offsetBy(dx:0,dy:pager.transform.ty),to:root.convert(sourceFrame,from:window),entering:false,ticket:ticket){[weak self] in
        guard let self,heroTicket==ticket,!retired,visible else{return}
        finishHero();pager.alpha=0;root.isUserInteractionEnabled=false;emit("back")
      }
    }else{emit("back")}
  }
  private func imageFrame()->CGRect {
    let size=items.first(where:{$0.id==previewId})?.resolvedDimensions ?? CGSize(width:1,height:1)
    let scale=min(root.bounds.width/max(1,size.width),root.bounds.height/max(1,size.height))
    let fit=CGSize(width:size.width*scale,height:size.height*scale)
    return CGRect(x:root.bounds.midX-fit.width/2,y:root.bounds.midY-fit.height/2,width:fit.width,height:fit.height)
  }
  private func animateHero(image:UIImage,from:CGRect,to:CGRect,entering:Bool,ticket:UUID,completion:@escaping()->Void) {
    guard from.width>0,from.height>0,to.width>0,to.height>0 else{completion();return}
    let imageView=UIImageView(image:image);imageView.contentMode = .scaleAspectFill;imageView.clipsToBounds=true
    imageView.frame=to;root.addSubview(imageView);heroView=imageView
    pager.alpha=0;root.isUserInteractionEnabled=false
    heroDelegate=ZoenAttachmentGalleryMotion.animate(view:imageView,from:from,to:to,entering:entering){[weak self] in
      guard let self,heroTicket==ticket else{return};completion()
    }
    // The invoked Legacy gallery crossfades the decoded destination at0.07s
    // and the interface/backdrop at0.15s, independently of its physical spring.
    UIView.animate(withDuration:UIAccessibility.isReduceMotionEnabled ? 0:0.15){[weak self] in
      self?.toolbar.alpha=entering ? 1:0;self?.captionPanel.alpha=entering ? 1:0
      self?.root.backgroundColor=entering ? .black:.clear
    }
  }
  private func finishHero() {
    heroTicket=nil;heroView?.layer.removeAllAnimations();heroView?.removeFromSuperview()
    heroView=nil;heroDelegate=nil;pager.alpha=1
    if !retired{root.isUserInteractionEnabled=true}
  }
  private func sendPressed() {
    guard canSend,previewIsVisible,!retired,!busy,caption.text.utf16.count<=4096,!caption.text.contains("\0") else{return}
    caption.resignFirstResponder()
    emit("send",id:previewId,payload:["text":caption.text ?? ""])
  }
  private func editPressed() {
    guard canEdit,previewIsVisible else{return}
    caption.resignFirstResponder(); pages.values.forEach { $0.stop() }
    emit("edit",id:previewId,payload:["text":caption.text ?? ""])
  }
  private func toggleRail() {
    guard !busy,!retired,!hasKeyboardFocus else{return}
    railVisible.toggle(); updateControls()
    let value = railVisible
    strip.alpha = value ? 0 : 1
    let offset = strip.bounds.height/3
    strip.transform = value ? CGAffineTransform(translationX:0,y:offset) : .identity
    UIView.animate(withDuration:UIAccessibility.isReduceMotionEnabled ? 0 : 0.3,delay:0,
      options:[.beginFromCurrentState,.curveEaseOut],animations:{[weak self] in
        self?.strip.alpha = value ? 1 : 0; self?.strip.transform = value ? .identity : CGAffineTransform(translationX:0,y:offset)
      },completion:{[weak self] _ in self?.updateControls()})
  }
  private func toggleControls() {
    guard !retired,!busy,!hasKeyboardFocus else{return}
    controlsHidden.toggle()
    UIView.animate(withDuration:UIAccessibility.isReduceMotionEnabled ? 0 : 0.2,delay:0,
      options:[.beginFromCurrentState,.curveLinear],animations:{[weak self] in
        guard let self else{return}; for v in [toolbar,captionPanel,counter,check,recipient,recipientArrow,strip] {v.alpha = controlsHidden ? 0 : 1}
      },completion:{[weak self] _ in self?.updateControls()})
  }
  @objc private func highlight(_ sender:UIButton) {sender.alpha = 0.4}
  @objc private func unhighlight(_ sender:UIButton) {sender.alpha = 1}
  func textView(_ textView:UITextView,shouldChangeTextIn range:NSRange,replacementText text:String)->Bool {
    guard !retired,!busy,captionEditable,!text.contains("\0"),let r=Range(range,in:caption.text) else{return false}
    return caption.text.replacingCharacters(in:r,with:text).utf16.count<=4096
  }
  func textViewDidChange(_ textView:UITextView) {
    guard captionEditable,caption.text.utf16.count<=4096,!caption.text.contains("\0") else{return}
    placeholder.isHidden = !caption.text.isEmpty
    emit("caption",id:captionItem,payload:["text":caption.text ?? ""]);reflow()
  }
  func textViewDidBeginEditing(_ textView:UITextView) {
    guard !retired,captionEditable else{textView.resignFirstResponder();return}
    pages.values.forEach {$0.stop()}; railVisible=false; didBeginEditing?();reflow()
  }
  func textViewDidEndEditing(_ textView:UITextView) {reflow()}
  func scrollViewWillBeginDragging(_ scrollView:UIScrollView) {
    caption.resignFirstResponder();pages.values.forEach{$0.stop()}
    if scrollView === pager {pagingRevision=revision;updateControls()}
  }
  func scrollViewDidEndDecelerating(_ scrollView:UIScrollView) {settlePage(scrollView)}
  func scrollViewDidEndDragging(_ scrollView:UIScrollView,willDecelerate decelerate:Bool) {if !decelerate {settlePage(scrollView)}}
  private func settlePage(_ scroll:UIScrollView) {
    guard scroll === pager,!retired,pager.bounds.width>0 else{return}
    let current = !busy && pagingRevision==revision
    pagingRevision=nil
    guard current else {
      if let index=items.firstIndex(where:{$0.id==previewId}) {
        pager.setContentOffset(CGPoint(x:CGFloat(index)*pager.bounds.width,y:0),animated:false)
      }
      updateControls();return
    }
    let index=Int(round(pager.contentOffset.x/pager.bounds.width))
    if items.indices.contains(index),items[index].id != previewId {emit("preview",id:items[index].id)}
    updateControls()
  }
  func gestureRecognizerShouldBegin(_ recognizer:UIGestureRecognizer)->Bool {
    guard !retired,!busy,!hasKeyboardFocus else{return false}
    if recognizer === dismissal {
      let v=dismissal.velocity(in:root)
      return !selectedMode && abs(v.y)>abs(v.x) && pages[previewId]?.isZoomed != true
    }
    return selectedMode && canReorder && selected.count>1
  }
  @objc private func dismissPan(_ gesture:UIPanGestureRecognizer) {
    guard !retired,!busy else{return}
    let y=gesture.translation(in:root).y
    switch gesture.state {
    case .began: caption.resignFirstResponder();pages.values.forEach{$0.stop()}
    case .changed:
      pager.transform=CGAffineTransform(translationX:0,y:y)
      let progress=min(1,abs(y)/150)
      root.backgroundColor=UIColor.black.withAlphaComponent(1-min(1,progress*4))
      toolbar.alpha=1-min(1,progress*2);captionPanel.alpha=toolbar.alpha
    case .ended:
      if ZoenAttachmentGalleryMetrics.dismiss(distance:y,velocity:gesture.velocity(in:root).y,height:root.bounds.height) {back()}
      else {restoreDrag()}
    case .cancelled,.failed:restoreDrag()
    default:break
    }
  }
  private func restoreDrag() {
    UIView.animate(withDuration:UIAccessibility.isReduceMotionEnabled ? 0 : 0.3,delay:0,
      options:[.beginFromCurrentState,.curveEaseOut],animations:{[weak self] in
        self?.pager.transform = .identity;self?.root.backgroundColor = .black
        self?.toolbar.alpha=1;self?.captionPanel.alpha=1
      })
  }
  @objc private func reorderGesture(_ recognizer:UILongPressGestureRecognizer) {
    let point=recognizer.location(in:selection)
    switch recognizer.state {
    case .began:
      guard !retired,!busy,let (id,tile)=selectedViews.first(where:{$0.value.frame.contains(point)}),let snapshot=tile.snapshotView(afterScreenUpdates:false) else{return}
      dragId=id;reorderRevision=revision;dragOrigin=point;snapshot.frame=tile.frame
      selection.addSubview(snapshot);dragSnapshot=snapshot;tile.isHidden=true
      UIImpactFeedbackGenerator(style:.light).impactOccurred()
      UIView.animate(withDuration:0.2){snapshot.transform=CGAffineTransform(scaleX:1.05,y:1.05)}
    case .changed:
      guard let snapshot=dragSnapshot else{return}
      let delta=CGPoint(x:point.x-dragOrigin.x,y:point.y-dragOrigin.y)
      snapshot.transform=CGAffineTransform(translationX:delta.x,y:delta.y).scaledBy(x:1.05,y:1.05)
    case .ended:
      guard let id=dragId,reorderRevision==revision,let target=selectedViews.first(where:{$0.value.frame.contains(point)})?.key,
        let from=selected.firstIndex(of:id),let to=selected.firstIndex(of:target) else{cancelReorder();return}
      var order=selected;order.remove(at:from);order.insert(id,at:to)
      cancelReorder();emit("reorder",payload:["ids":order]);UISelectionFeedbackGenerator().selectionChanged()
    case .cancelled,.failed:cancelReorder()
    default:break
    }
  }
  private func cancelReorder() {
    if let id=dragId {selectedViews[id]?.isHidden=false}
    dragSnapshot?.removeFromSuperview();dragSnapshot=nil;dragId=nil;reorderRevision=nil
  }
  func conceal() {root.isHidden=true;retire()}
  func retire() {
    guard !retired else{return}
    retired=true;finishHero();root.isUserInteractionEnabled=false;caption.resignFirstResponder();caption.delegate=nil
    didBeginEditing=nil;cancelReorder();pages.values.forEach{$0.retire()};selectedViews.values.forEach{$0.retire()}
    strip.visibleCells.forEach{($0 as? GalleryStripCell)?.retire()}
  }
}

@MainActor extension ZoenAttachmentGalleryContent: UICollectionViewDataSource,UICollectionViewDelegate {
  func collectionView(_ collectionView:UICollectionView,numberOfItemsInSection section:Int)->Int {selected.count}
  func collectionView(_ collectionView:UICollectionView,cellForItemAt indexPath:IndexPath)->UICollectionViewCell {
    guard let cell=collectionView.dequeueReusableCell(withReuseIdentifier:"strip",for:indexPath) as? GalleryStripCell,
      selected.indices.contains(indexPath.item),let item=items.first(where:{$0.id==selected[indexPath.item]}) else{return UICollectionViewCell()}
    cell.update(item:item,focused:item.id==previewId);return cell
  }
  func collectionView(_ collectionView:UICollectionView,didSelectItemAt indexPath:IndexPath) {
    guard selected.indices.contains(indexPath.item) else{return};emit("preview",id:selected[indexPath.item])
  }
  func collectionView(_ collectionView:UICollectionView,didEndDisplaying cell:UICollectionViewCell,forItemAt indexPath:IndexPath) {(cell as? GalleryStripCell)?.retire()}
}

private enum GalleryFailure:Error {case invalid}
private struct GalleryItem:Equatable {
  let id:String;let name:String;let video:Bool;let uri:URL?;let photoId:String?;let dimensions:CGSize
  init(value:[String:Any],source:String)throws {
    guard let id=value["id"] as? String,!id.isEmpty,id.utf8.count<=2048 else{throw GalleryFailure.invalid}
    self.id=id;name=String((value["name"] as? String ?? "").prefix(255))
    video=(value["mime"] as? String ?? "").hasPrefix("video/") || value["type"] as? String == "video"
    let w=(value["width"] as? Double).flatMap{$0.isFinite && $0>0 && $0<=100000 ? $0:nil} ?? 1
    let h=(value["height"] as? Double).flatMap{$0.isFinite && $0>0 && $0<=100000 ? $0:nil} ?? 1
    if source=="photos" {guard id.hasPrefix("ph://"),id.count>5 else{throw GalleryFailure.invalid};photoId=String(id.dropFirst(5));uri=nil}
    else {
      photoId=nil
      let raw=value["uri"] as? String ?? ""
      if raw.isEmpty {uri=nil;dimensions=CGSize(width:w,height:h);return}
      guard let url=URL(string:raw),url.isFileURL,url.query==nil,url.fragment==nil else{throw GalleryFailure.invalid}
      let resolved=url.resolvingSymlinksInPath().standardizedFileURL
      let roots=[URL.cachesDirectory,URL.temporaryDirectory].map{$0.resolvingSymlinksInPath().standardizedFileURL.path+"/"}
      guard roots.contains(where:{resolved.path.hasPrefix($0)}),
        let attributes=try? FileManager.default.attributesOfItem(atPath:url.path),attributes[.type] as? FileAttributeType == .typeRegular,
        let size=attributes[.size] as? NSNumber,size.uint64Value>0,size.uint64Value<=ZoenMediaFiles.maximumBytes else{throw GalleryFailure.invalid}
      uri=resolved
    }
    dimensions=CGSize(width:w,height:h)
  }
  var resolvedDimensions:CGSize {
    if dimensions.width==1,dimensions.height==1,let photoId,
      let asset=PHAsset.fetchAssets(withLocalIdentifiers:[photoId],options:nil).firstObject {
      return CGSize(width:max(1,asset.pixelWidth),height:max(1,asset.pixelHeight))
    }
    return dimensions
  }
}

@MainActor private final class GalleryRoot:UIView {
  var onLayout:(()->Void)?;var accessibilityEscape:(()->Void)?
  override func layoutSubviews(){super.layoutSubviews();onLayout?()}
  override func accessibilityPerformEscape()->Bool{accessibilityEscape?();return true}
}

/// Each surface owns only cancelable display requests. No asset is copied or
/// saved; replacement and retirement fence every asynchronous result.
@MainActor private class GalleryAssetView:UIView {
  let image=UIImageView()
  let item:GalleryItem
  private let manager=PHImageManager()
  private let targetPixels:CGFloat
  private let allowNetwork:Bool
  private let progress=UIProgressView(progressViewStyle:.bar)
  private let retry=UIButton(type:.system)
  private let fallback=UILabel()
  private var request:PHImageRequestID?
  private var generation=UUID()
  private(set)var retired=false
  var loaded:((UIImage)->Void)?
  init(item:GalleryItem,targetPixels:CGFloat=640,allowNetwork:Bool=false){
    self.item=item;self.targetPixels=targetPixels;self.allowNetwork=allowNetwork
    super.init(frame:.zero);image.contentMode = .scaleAspectFill;image.clipsToBounds=true;addSubview(image)
    progress.progressTintColor = .white;progress.trackTintColor=UIColor.white.withAlphaComponent(0.2);progress.isHidden=true
    retry.setTitle(NSLocalizedString("Try Again",comment:"Retry requested media download"),for:.normal);retry.tintColor = .white;retry.isHidden=true
    retry.addAction(UIAction{[weak self] _ in self?.load()},for:.touchUpInside)
    fallback.text=item.name;fallback.textColor = .white;fallback.textAlignment = .center;fallback.numberOfLines=2
    addSubview(fallback);addSubview(progress);addSubview(retry);load()
  }
  required init?(coder:NSCoder){return nil}
  override func layoutSubviews(){super.layoutSubviews();if image.superview===self{image.frame=bounds};progress.frame=CGRect(x:bounds.midX-50,y:bounds.midY+40,width:100,height:2);retry.frame=CGRect(x:bounds.midX-60,y:bounds.midY-22,width:120,height:44);fallback.frame=CGRect(x:24,y:bounds.midY+65,width:max(0,bounds.width-48),height:44)}
  private func load(){
    guard !retired else{return}
    if let request{manager.cancelImageRequest(request)}
    generation=UUID();let ticket=generation;retry.isHidden=true
    if let id=item.photoId,let asset=PHAsset.fetchAssets(withLocalIdentifiers:[id],options:nil).firstObject {
      let options=PHImageRequestOptions();options.isNetworkAccessAllowed=allowNetwork;options.deliveryMode = .opportunistic
      options.progressHandler={[weak self] value,_,_,_ in Task{@MainActor [weak self] in guard let self,!retired,generation==ticket else{return};progress.isHidden=false;progress.progress=Float(min(1,max(0,value)))}}
      request=manager.requestImage(for:asset,targetSize:CGSize(width:targetPixels,height:targetPixels),contentMode:.aspectFit,options:options){[weak self] value,info in
        Task{@MainActor [weak self] in
          guard let self,!retired,generation==ticket else{return}
          if let value{image.image=value;fallback.isHidden=true;loaded?(value)}
          if info?[PHImageResultIsDegradedKey] as? Bool != true{progress.isHidden=true;retry.isHidden=value != nil || !allowNetwork}
        }
      }
    } else if let url=item.uri {
      let options:[CFString:Any]=[kCGImageSourceShouldCache:false]
      if let source=CGImageSourceCreateWithURL(url as CFURL,options as CFDictionary),let cg=CGImageSourceCreateThumbnailAtIndex(source,0,[kCGImageSourceCreateThumbnailFromImageAlways:true,kCGImageSourceCreateThumbnailWithTransform:true,kCGImageSourceThumbnailMaxPixelSize:targetPixels] as CFDictionary){
        image.image=UIImage(cgImage:cg);fallback.isHidden=true
      }
    }
  }
  func retire(){guard !retired else{return};retired=true;generation=UUID();if let request{manager.cancelImageRequest(request)};request=nil;loaded=nil}
}

@MainActor private final class GalleryMediaView:GalleryAssetView,UIScrollViewDelegate {
  private let zoom=UIScrollView()
  private var player:AVPlayer?
  private var playerLayer:AVPlayerLayer?
  private var videoRequest:PHImageRequestID?
  private let videoManager=PHImageManager()
  private let play=UIButton(type:.system)
  var toggle:(()->Void)?;var zoomChanged:((Bool)->Void)?
  var active=false {didSet{if !active{stop()}}}
  var isZoomed:Bool{abs(zoom.zoomScale-1)>0.01}
  init(item:GalleryItem,allowNetwork:Bool){
    super.init(item:item,targetPixels:2048,allowNetwork:allowNetwork)
    image.removeFromSuperview();zoom.addSubview(image);addSubview(zoom)
    zoom.delegate=self;zoom.minimumZoomScale=1;zoom.maximumZoomScale=3;zoom.bouncesZoom=true
    zoom.showsHorizontalScrollIndicator=false;zoom.showsVerticalScrollIndicator=false
    image.contentMode = .scaleAspectFit;image.isUserInteractionEnabled=false
    let tap=UITapGestureRecognizer(target:self,action:#selector(singleTap));let double=UITapGestureRecognizer(target:self,action:#selector(doubleTap(_:)));double.numberOfTapsRequired=2;tap.require(toFail:double)
    zoom.addGestureRecognizer(tap);zoom.addGestureRecognizer(double)
    play.setImage(ZoenTelegramAttachmentGlyph.image(.play),for:.normal);play.backgroundColor=UIColor.black.withAlphaComponent(0.3);play.layer.cornerRadius=30;play.tintColor = .white
    play.accessibilityLabel=item.name;play.addAction(UIAction{[weak self] _ in self?.playVideo()},for:.touchUpInside);play.isHidden = !item.video;addSubview(play)
  }
  required init?(coder:NSCoder){return nil}
  override func layoutSubviews(){super.layoutSubviews();zoom.frame=bounds;if !isZoomed{image.frame=zoom.bounds};playerLayer?.frame=bounds;play.frame=CGRect(x:bounds.midX-30,y:bounds.midY-30,width:60,height:60)}
  func viewForZooming(in scrollView:UIScrollView)->UIView?{image}
  func scrollViewDidZoom(_ scrollView:UIScrollView){zoomChanged?(isZoomed)}
  @objc private func singleTap(){if item.video,let player,player.rate>0{player.pause();play.isHidden=false}else{toggle?()}}
  @objc private func doubleTap(_ tap:UITapGestureRecognizer){
    guard !item.video,!retired,active else{return}
    if isZoomed{zoom.setZoomScale(1,animated:!UIAccessibility.isReduceMotionEnabled)}
    else{let point=tap.location(in:image);let size=CGSize(width:zoom.bounds.width/3,height:zoom.bounds.height/3);zoom.zoom(to:CGRect(x:point.x-size.width/2,y:point.y-size.height/2,width:size.width,height:size.height),animated:!UIAccessibility.isReduceMotionEnabled)}
  }
  private func playVideo(){
    guard item.video,active,!retired else{return}
    if let player{player.rate==0 ? player.play() : player.pause();return}
    if let url=item.uri{install(AVPlayerItem(url:url))}
    else if let id=item.photoId,let asset=PHAsset.fetchAssets(withLocalIdentifiers:[id],options:nil).firstObject{
      let options=PHVideoRequestOptions();options.isNetworkAccessAllowed=true
      videoRequest=videoManager.requestPlayerItem(forVideo:asset,options:options){[weak self] item,_ in
        Task{@MainActor [weak self] in guard let self,!retired,active,let item else{return};install(item)}
      }
    }
  }
  private func install(_ item:AVPlayerItem){
    let player=AVPlayer(playerItem:item);player.allowsExternalPlayback=false
    let layer=AVPlayerLayer(player:player);layer.videoGravity = .resizeAspect;self.layer.insertSublayer(layer,above:zoom.layer)
    self.player=player;playerLayer=layer;layer.frame=bounds;play.isHidden=true;player.play()
  }
  func stop(){player?.pause()}
  override func retire(){super.retire();stop();if let videoRequest{videoManager.cancelImageRequest(videoRequest)};videoRequest=nil;player?.replaceCurrentItem(with:nil);player=nil;playerLayer?.removeFromSuperlayer();playerLayer=nil;toggle=nil;zoomChanged=nil}
}

@MainActor private final class GallerySelectionTile:GalleryAssetView {
  var open:(()->Void)?
  init(item:GalleryItem){super.init(item:item);clipsToBounds=true;addGestureRecognizer(UITapGestureRecognizer(target:self,action:#selector(tap)));isAccessibilityElement=true;accessibilityLabel=item.name;accessibilityTraits = .button}
  required init?(coder:NSCoder){return nil}
  @objc private func tap(){if !retired{open?()}}
  override func retire(){super.retire();open=nil}
}

@MainActor private final class GalleryStripCell:UICollectionViewCell {
  private var asset:GalleryAssetView?
  override func prepareForReuse(){super.prepareForReuse();retire();asset?.removeFromSuperview();asset=nil}
  func update(item:GalleryItem,focused:Bool){
    retire();asset?.removeFromSuperview();let view=GalleryAssetView(item:item);asset=view;contentView.addSubview(view);view.frame=contentView.bounds
    contentView.layer.borderWidth=focused ? 2 : 0;contentView.layer.borderColor=UIColor.white.cgColor
    isAccessibilityElement=true;accessibilityLabel=item.name;accessibilityTraits=focused ? [.button,.selected] : .button
  }
  override func layoutSubviews(){super.layoutSubviews();asset?.frame=contentView.bounds}
  func retire(){asset?.retire()}
}

@MainActor private final class GalleryCheckButton:UIButton {
  private let badge=UIView()
  private let number=UILabel()
  override init(frame:CGRect){super.init(frame:frame);number.textAlignment = .center;number.textColor = .white;number.font = .systemFont(ofSize:15,weight:.semibold);badge.isUserInteractionEnabled=false;badge.addSubview(number);addSubview(badge)}
  required init?(coder:NSCoder){return nil}
  func update(order:Int?,accent:UIColor){number.text=order.map(String.init);accessibilityValue=number.text;badge.layer.borderColor=UIColor.white.cgColor;badge.layer.borderWidth=2;badge.backgroundColor=order==nil ? UIColor.black.withAlphaComponent(0.3) : accent;badge.layer.cornerRadius=14.5;isSelected=order != nil}
  override func layoutSubviews(){super.layoutSubviews();badge.frame=CGRect(x:(bounds.width-29)/2,y:(bounds.height-29)/2,width:29,height:29);number.frame=badge.bounds}
}

@MainActor private final class GalleryCounterButton:UIButton {
  private let ring=UIView();private let number=UILabel();private let cross=UIImageView()
  override init(frame:CGRect){super.init(frame:frame);ring.isUserInteractionEnabled=false;ring.layer.cornerRadius=15.5;ring.layer.borderColor=UIColor.white.cgColor;ring.backgroundColor=UIColor.black.withAlphaComponent(0.5);number.font = .systemFont(ofSize:18,weight:.regular);number.textColor = .white;number.textAlignment = .center;cross.image=ZoenTelegramAttachmentGlyph.image(.close);cross.contentMode = .center;for child in [ring,number,cross]{addSubview(child)}}
  required init?(coder:NSCoder){return nil}
  func update(count:Int,expanded:Bool){number.text=String(count);number.isHidden=expanded;cross.isHidden = !expanded;accessibilityValue=String(count)}
  override func layoutSubviews(){super.layoutSubviews();ring.frame=CGRect(x:16.5,y:1,width:31,height:31);ring.layer.borderWidth=traitCollection.displayScale==3 ? 5/3 : 1.5;number.frame=CGRect(x:13,y:5+1/max(1,traitCollection.displayScale),width:38,height:22);cross.frame=CGRect(x:13,y:0,width:38,height:33)}
}
