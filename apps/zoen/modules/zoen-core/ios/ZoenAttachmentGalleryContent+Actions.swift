import AVFoundation
import ImageIO
import Photos
import UIKit

@MainActor extension ZoenAttachmentGalleryContent {
  func emit(_ action:String,id:String?=nil,payload:[String:Any]=[:]) {
    guard !retired,visible,!busy else{return}
    if let id, !items.contains(where:{$0.id == id}) {return}
    var value = payload; if let id {value["id"] = id}
    value["revision"] = revision
    send(action,value)
  }
  func back() {
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
  func imageFrame()->CGRect {
    let size=items.first(where:{$0.id==previewId})?.resolvedDimensions ?? CGSize(width:1,height:1)
    let viewport=ZoenAttachmentGalleryLayout.mediaFrame(size:root.bounds.size,keyboardHeight:keyboardHeight)
    return ZoenAttachmentGalleryLayout.aspectFit(dimensions:size,viewport:viewport)
  }
  func animateHero(image:UIImage,from:CGRect,to:CGRect,entering:Bool,ticket:UUID,completion:@escaping()->Void) {
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
  func finishHero() {
    heroTicket=nil;heroView?.layer.removeAllAnimations();heroView?.removeFromSuperview()
    heroView=nil;heroDelegate=nil;pager.alpha=1
    if !retired{root.isUserInteractionEnabled=true}
  }
  func sendPressed() {
    guard canSend,previewIsVisible,!retired,!busy,caption.text.utf16.count<=4096,!caption.text.contains("\0") else{return}
    caption.resignFirstResponder()
    emit("send",id:previewId,payload:["text":caption.text ?? ""])
  }
  func editPressed() {
    guard canEdit,previewIsVisible else{return}
    caption.resignFirstResponder(); pages.values.forEach { $0.stop() }
    emit("edit",id:previewId,payload:["text":caption.text ?? ""])
  }
  func toggleRail() {
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
  func toggleControls() {
    guard !retired,!busy,!hasKeyboardFocus else{return}
    controlsHidden.toggle()
    let footer = source == "conversation" ? (conversationCaption.isHidden ? CGRect.zero:conversationCaption.frame) : captionPanel.frame
    for page in pages.values {
      page.updateVideoPresentation(visible:!controlsHidden,safeArea:safe,
        captionFrame:footer.isEmpty ? .zero:page.convert(footer,from:root))
    }
    UIView.animate(withDuration:UIAccessibility.isReduceMotionEnabled ? 0 : 0.2,delay:0,
      options:[.beginFromCurrentState,.curveLinear],animations:{[weak self] in
        guard let self else{return}; for v in [toolbar,captionPanel,counter,check,recipient,recipientArrow,strip,conversationCaption,position] {v.alpha = controlsHidden ? 0 : 1}
      },completion:{[weak self] _ in self?.updateControls()})
  }
  @objc func highlight(_ sender:UIButton) {sender.alpha = 0.4}
  @objc func unhighlight(_ sender:UIButton) {sender.alpha = 1}
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
      if source == "conversation", !conversationCaption.isHidden,
        conversationCaption.frame.contains(dismissal.location(in:root)) {return false}
      let point=dismissal.location(in:root)
      if let page=pages[previewId],page.ownsControls(at:page.convert(point,from:root)) {return false}
      var hit=root.hitTest(point,with:nil)
      while let view=hit,view !== root {if view is UIControl{return false};hit=view.superview}
      let v=dismissal.velocity(in:root)
      return abs(v.y)>abs(v.x) && pages[previewId]?.isZoomed != true
    }
    return recognizer === reorder && railVisible && canReorder && selected.count>1
  }
  @objc func dismissPan(_ gesture:UIPanGestureRecognizer) {
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
      let dismiss = source == "conversation"
        ? abs(y)>root.bounds.height/4 || abs(gesture.velocity(in:root).y)>1000
        : ZoenAttachmentGalleryMetrics.dismiss(distance:y,velocity:gesture.velocity(in:root).y,height:root.bounds.height)
      if dismiss {back()}
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
  @objc func reorderGesture(_ recognizer:UILongPressGestureRecognizer) {
    let point=recognizer.location(in:strip)
    switch recognizer.state {
    case .began:
      guard !retired,!busy,visible,canReorder,let index=strip.indexPathForItem(at:point),
        selected.indices.contains(index.item) else{return}
      cancelReorder();dragId=selected[index.item];reorderRevision=revision
      if strip.beginInteractiveMovementForItem(at:index) {UIImpactFeedbackGenerator(style:.light).impactOccurred()}
      else {cancelReorder()}
    case .changed:
      guard reorderRevision==revision,dragId != nil else{cancelReorder();return}
      strip.updateInteractiveMovementTargetPosition(point)
    case .ended:
      guard !retired,!busy,visible,reorderRevision==revision else{cancelReorder();return}
      strip.endInteractiveMovement()
    case .cancelled,.failed:cancelReorder()
    default:break
    }
  }
  func cancelReorder() {
    strip.cancelInteractiveMovement();dragId=nil;reorderRevision=nil
  }
}
