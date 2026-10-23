import AVFoundation
import ImageIO
import Photos
import UIKit

@MainActor extension ZoenAttachmentGalleryContent {
  func rebuildMedia() {
    if source == "conversation",let index=items.firstIndex(where:{$0.id==previewId}) {
      let wanted=Array(items[max(0,index-1)...min(items.count-1,index+1)])
      for id in Array(pages.keys) {
        guard let page=pages[id] else{continue}
        if !wanted.contains(where:{$0 == page.item}) {page.retire();page.removeFromSuperview();pages.removeValue(forKey:id)}
      }
      for item in wanted {
        if pages[item.id] == nil {
          let page=GalleryMediaView(item:item,allowNetwork:false,conversation:true)
          page.accessibilityIdentifier="conversation-gallery-item-\(item.id)"
          page.toggle = { [weak self] in self?.toggleControls() }
          page.zoomChanged = { [weak self] _ in self?.updateControls() }
          pages[item.id]=page;pager.addSubview(page)
        }
        pages[item.id]?.active = item.id == previewId
      }
      pager.isHidden=false;selection.isHidden=true
      return
    }
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
        let page = GalleryMediaView(item:item,allowNetwork:position == index,conversation:source == "conversation")
        page.active = position == index
        page.toggle = { [weak self] in self?.toggleControls() }
        page.zoomChanged = { [weak self] _ in self?.updateControls() }
        pages[item.id] = page; pager.addSubview(page)
      }
    }
  }
  private var orderedItems: [GalleryItem] { selected.compactMap { id in items.first { $0.id == id } } }

  func reflow() {
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
    // GalleryPagerNode:420–424 uses a20pt gap at either side of each page.
    // Picker geometry remains the separately ported MediaPicker gallery.
    let gap:CGFloat = source == "conversation" ? 20:0
    let stride = size.width+gap*2
    pager.frame = CGRect(x:-gap,y:0,width:stride,height:size.height)
    pager.contentSize = CGSize(width:stride*CGFloat(items.count),height:size.height)
    let changedSize = lastSize != size; lastSize = size
    for (position,item) in items.enumerated() { pages[item.id]?.frame = CGRect(x:CGFloat(position)*stride+gap,y:0,width:size.width,height:size.height) }
    if let index = items.firstIndex(where:{$0.id == previewId}), (!pager.isDragging && !pager.isDecelerating) || changedSize {
      pager.setContentOffset(CGPoint(x:CGFloat(index)*stride,y:0),animated:false)
    }
    selection.frame = root.bounds
    layoutSelected(size:CGSize(width:size.width,height:captionY-10))
    errorLabel.frame = CGRect(x:16,y:captionY-76,width:max(0,size.width-32),height:50)
    retry.frame = CGRect(x:size.width-100,y:captionY-26,width:84,height:24)
    if source == "conversation" {
      toolbar.frame = CGRect(x:0,y:safe.top,width:size.width,height:44)
      closeGlass.frame = CGRect(x:safe.left+8,y:0,width:44,height:44); close.frame = closeGlass.bounds
      recipient.frame = CGRect(x:safe.left+64,y:safe.top+3,width:max(0,size.width-safe.left-safe.right-128),height:20)
      recipient.textAlignment = .center
      position.frame = CGRect(x:safe.left+64,y:safe.top+23,width:max(0,size.width-safe.left-safe.right-128),height:20)
      position.font = .systemFont(ofSize:13)
      let available = max(0,size.height-safe.top-safe.bottom-110)
      let text = conversationCaption.sizeThatFits(CGSize(width:max(1,size.width-safe.left-safe.right-32),height:available))
      // ChatItemGalleryFooterContentNode1189–1193,1218–1230: compact
      // landscape hides caption; text over100pt uses an80pt scroll viewport.
      let displaysCaption = !landscape || root.traitCollection.horizontalSizeClass == .regular
      let bottom = size.height-safe.bottom-16
      let sourceHeight:CGFloat = text.height>100 ? 80:text.height
      let transportLimit = items.first(where:{$0.id==previewId})?.video == true
        ? max(0,bottom-ZoenConversationVideoControls.minimumCaptionY(height:size.height)):available
      let captionHeight = displaysCaption ? min(available,sourceHeight,transportLimit):0
      conversationCaption.frame = CGRect(x:safe.left+16,y:bottom-captionHeight,width:max(0,size.width-safe.left-safe.right-32),height:captionHeight)
      conversationCaption.isScrollEnabled = captionHeight>0 && text.height>captionHeight
    } else { recipient.textAlignment = .left }
    updateControls()
  }

  private func layoutSelected(size:CGSize) {
    guard selectedMode else{return}
    let width = max(1,min(320,size.width-safe.left-safe.right-68))
    var y = safe.top+60
    title.frame = CGRect(x:0,y:safe.top+28,width:size.width,height:20)
    let selected = orderedItems
    let groupSize = grouped ? 10 : 1
    for (index,start) in stride(from:0,to:selected.count,by:groupSize).enumerated() {
      let group = Array(selected[start..<min(start+groupSize,selected.count)])
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

}
