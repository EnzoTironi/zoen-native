import AVFoundation
import ImageIO
import Photos
import UIKit

@MainActor extension ZoenAttachmentGalleryContent {
  func update(model: [String:Any], appearance: ZoenKeyboardPanelAppearance) throws {
    guard !retired, let source = model["source"] as? String, ["photos","draft","conversation"].contains(source),
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
    guard text.utf16.count <= (source == "conversation" ? 65536 : 4096), !text.contains("\0"),
      source != "conversation" || (incoming.count <= 10 && selected.isEmpty && model["screen"] as? String == "preview") else { throw GalleryFailure.invalid }
    let newSelectedMode = model["screen"] as? String == "selected"
    let newGrouped = model["grouped"] as? Bool ?? true
    let mediaChanged = self.source != source || self.items != parsed || self.previewId != preview || selectedMode != newSelectedMode || grouped != newGrouped
    let focusedChanged = self.source != source || self.previewId != preview
    if self.revision != revision { cancelReorder() }
    let itemChanged = captionItem != preview
    self.source = source; self.items = parsed; self.selected = selected
    self.previewId = preview; self.revision = revision; self.appearance = appearance
    selectedMode = newSelectedMode; busy = model["busy"] as? Bool ?? false
    grouped = newGrouped
    if busy {pages.values.forEach{$0.stop()};caption.resignFirstResponder()}
    root.backgroundColor = selectedMode ? appearance.background : .black
    root.overrideUserInterfaceStyle = selectedMode ? (appearance.isDark ? .dark : .light) : .dark
    caption.keyboardAppearance = selectedMode ? (appearance.isDark ? .dark : .light) : .dark
    labels = model["labels"] as? [String:String] ?? [:]
    canEdit = source != "conversation" && model["canEdit"] as? Bool == true
    canAdd = source == "draft" && model["canAdd"] as? Bool == true
    canRemove = source == "draft" && model["canRemove"] as? Bool == true
    canReorder = model["canReorder"] as? Bool ?? (source == "photos")
    canSend = source != "conversation" && (captionModel["sendEnabled"] as? Bool ?? (!selected.isEmpty || (source == "photos" && !preview.isEmpty)))
    captionEditable = source != "conversation" && captionModel["editable"] as? Bool == true && !busy
    conversationCaption.text = text
    position.text = model["position"] as? String
    caption.isEditable = captionEditable
    if itemChanged || (!caption.isFirstResponder && caption.text != text) { caption.text = text }
    captionItem = preview
    placeholder.text = captionModel["placeholder"] as? String ?? ""
    placeholder.isHidden = !caption.text.isEmpty
    recipient.text = model["recipient"] as? String ?? ""
    close.accessibilityLabel = labels["back"] ?? labels["close"]
    close.accessibilityIdentifier = source == "conversation" ? "conversation-gallery-close":"attachment-approval-close"
    pager.accessibilityIdentifier = source == "conversation" ? "conversation-gallery-pager":"attachment-gallery-pager"
    root.accessibilityIdentifier = source == "conversation" ? "conversation-gallery-surface":"attachment-gallery-surface"
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
      if source != "conversation" || focusedChanged {
        controlsHidden = false
        for view in [toolbar,captionPanel,counter,check,recipient,recipientArrow,strip,conversationCaption,position] {
          view.layer.removeAnimation(forKey:"opacity");view.alpha = view === recipientArrow ? 0.6 : 1
        }
      }
      if visible {rebuildMedia()} else {needsRebuild=true}
    }
    updateControls(); strip.reloadData(); reflow()
  }

  func updatePresentation(keyboardHeight:CGFloat,safeArea:UIEdgeInsets) {
    guard !retired else { return }
    self.keyboardHeight = max(0,keyboardHeight); safe = safeArea; reflow()
  }
  /// GalleryControllerNode:420–515 simple-animation path when no native
  /// transition image survives (for example, a virtualized Fabric message).
  func animateConversationIn() {
    guard source == "conversation",!retired else{return}
    root.backgroundColor = .clear;pager.alpha=0
    for view in [toolbar,recipient,position,conversationCaption]{view.alpha=0}
    UIView.animate(withDuration:UIAccessibility.isReduceMotionEnabled ? 0:0.15,delay:0,options:[.curveLinear,.beginFromCurrentState]) {
      self.root.backgroundColor = .black
      for view in [self.toolbar,self.recipient,self.position,self.conversationCaption]{view.alpha=1}
    }
    UIView.animate(withDuration:UIAccessibility.isReduceMotionEnabled ? 0:0.3,delay:0,options:[.curveLinear,.beginFromCurrentState]) {self.pager.alpha=1}
  }
  func animateConversationOut(completion:@escaping()->Void) {
    root.isUserInteractionEnabled=false
    UIView.animate(withDuration:UIAccessibility.isReduceMotionEnabled ? 0:0.1,delay:0,options:[.curveLinear,.beginFromCurrentState]) {
      self.root.backgroundColor = .clear
      for view in [self.toolbar,self.recipient,self.position,self.conversationCaption]{view.alpha=0}
    }
    UIView.animate(withDuration:UIAccessibility.isReduceMotionEnabled ? 0:0.25,delay:0,options:[.curveLinear,.beginFromCurrentState],animations:{self.pager.alpha=0}){_ in completion()}
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
  func updateControls() {
    let aligned = previewIsVisible
    pager.isScrollEnabled = !busy && pages[previewId]?.isZoomed != true
    close.isEnabled = !busy; edit.isEnabled = !busy && canEdit && aligned
    add.isEnabled = !busy && canAdd
    sendButton.isEnabled = !busy && canSend && aligned; sendGlass.alpha = sendButton.isEnabled ? 1 : 0.2
    check.isEnabled = !busy && aligned && (source == "photos" || canRemove) && !hasKeyboardFocus
    check.isHidden = source == "conversation" || selectedMode || hasKeyboardFocus || (source == "draft" && !canRemove)
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
    if source == "conversation" {
      sendGlass.isHidden = true; centerGlass.isHidden = true
      captionPanel.isHidden = true; counter.isHidden = true; strip.isHidden = true
      recipientArrow.isHidden = true
    } else { sendGlass.isHidden = false }
    conversationCaption.isHidden = source != "conversation" || controlsHidden || (conversationCaption.text ?? "").isEmpty || conversationCaption.frame.height<=0
    position.isHidden = source != "conversation" || controlsHidden
    strip.isHidden = controlsHidden || !railVisible || hasKeyboardFocus || selectedMode
    captionPanel.isUserInteractionEnabled = !controlsHidden
    for page in pages.values {page.updateVideoPresentation(visible:!controlsHidden,safeArea:safe,captionFrame:conversationCaption.isHidden ? .zero:conversationCaption.frame)}
  }
  var previewIsVisible:Bool {
    if selectedMode{return true}
    guard !pager.isDragging,!pager.isDecelerating,pager.bounds.width>0,
      let index=items.firstIndex(where:{$0.id==previewId}) else{return false}
    return abs(pager.contentOffset.x-CGFloat(index)*pager.bounds.width)<1/max(1,root.traitCollection.displayScale)
  }
}
