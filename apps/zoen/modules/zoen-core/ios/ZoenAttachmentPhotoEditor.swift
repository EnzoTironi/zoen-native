import ImageIO
import PencilKit
import UIKit

// The toolbar and crop affordances are adapted from Telegram-iOS
// f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838, PhotoToolbar:428–855 and
// TGPhotoCropAreaView.m:11–12,129–230 (GPL-2.0-or-later, GPLv3 selected).
// Raster composition and ink are public UIKit/ImageIO/PencilKit adaptations.
// This bounded image editor is not the entire upstream GPU/filter/paint engine.
// Only functional crop, quarter-turn rotation, ink, undo and Save are exposed.

@MainActor final class ZoenAttachmentPhotoEditor:NSObject,ZoenFilePreviewPresentation {
  private weak var presenter:UIViewController?
  private let editing:ZoenFilePreview.Editing
  private var editor:Editor?
  private var ended:(@MainActor (Error?)->Void)?
  private var presenting=false
  private var dismissing=false
  private var waiting=false
  private var callbacks:[@MainActor ()->Void]=[]
  private var recovery:UIAlertController?
  private var captureError:Error?
  init(presenter:UIViewController?,editing:ZoenFilePreview.Editing){self.presenter=presenter;self.editing=editing}
  func show(file:URL,title:String,ended:@escaping @MainActor (Error?)->Void)throws {
    guard editing.allowsEditing,editor==nil,let presenter,presenter.viewIfLoaded?.window != nil,
      !presenter.isBeingPresented,!presenter.isBeingDismissed,presenter.presentedViewController==nil else{throw ZoenFilePreview.Failure.unavailable}
    let controller=try Editor(file:file,title:title)
    self.editor=controller;self.ended=ended
    controller.cancelled={[weak self] in self?.userClose()}
    controller.saved={[weak self] png in
      guard let self else{return}
      captureError=nil;editing.receiveImage(png)
      if captureError==nil{userClose()}
    }
    presenting=true
    presenter.present(controller,animated:!UIAccessibility.isReduceMotionEnabled){[weak self] in
      guard let self else{return};presenting=false;drainDismissal()
    }
  }
  private func userClose(){dismiss{[weak self] in guard let self else{return};let callback=ended;ended=nil;callback?(nil)}}
  func dismiss(completion:@escaping @MainActor ()->Void){callbacks.append(completion);editor?.retire();drainDismissal()}
  private func drainDismissal(){
    guard !presenting,!dismissing,!waiting,!callbacks.isEmpty else{return}
    let active:UIViewController?=recovery?.presentingViewController != nil ? recovery:editor
    if let transition=active?.transitionCoordinator ?? editor?.presentedViewController?.transitionCoordinator {
      waiting=true
      if transition.animate(alongsideTransition:nil,completion:{[weak self] _ in Task{@MainActor [weak self] in self?.waiting=false;self?.drainDismissal()}}){return}
      waiting=false
    }
    dismissing=true
    if let parent=editor?.presentingViewController ?? recovery?.presentingViewController {
      parent.dismiss(animated:false){[weak self] in self?.finishDismissal()}
    }else{finishDismissal()}
  }
  private func finishDismissal(){
    editor=nil;recovery=nil;dismissing=false
    let values=callbacks;callbacks=[];for callback in values{callback()}
  }
  func captureFailed(error:Error){captureError=error;editor?.show(error:error)}
  func recover(error:Error,changesSaved:Bool,retry:(@MainActor ()->Void)?,discard:@escaping @MainActor ()->Void){
    recover(error:error,changesSaved:changesSaved,retry:retry,keep:nil,discard:discard)
  }
  func recover(error:Error,changesSaved:Bool,retry:(@MainActor ()->Void)?,keep:(@MainActor ()->Void)?,discard:@escaping @MainActor ()->Void){
    guard recovery==nil,let presenter,presenter.viewIfLoaded?.window != nil else{return}
    let alert=UIAlertController(title:NSLocalizedString("Your changes need attention",comment:"Media edit recovery"),message:error.localizedDescription,preferredStyle:.alert)
    if let retry {alert.addAction(UIAlertAction(title:NSLocalizedString("Try Again",comment:"Retry media save"),style:.default){[weak self] _ in self?.recovery=nil;retry()})}
    if let keep {alert.addAction(UIAlertAction(title:NSLocalizedString("Keep for Later",comment:"Retain private media edit"),style:.default){[weak self] _ in self?.recovery=nil;keep()})}
    alert.addAction(UIAlertAction(title:changesSaved ? NSLocalizedString("Close",comment:"Close saved media") : NSLocalizedString("Discard Changes",comment:"Discard unsaved media edit"),style:.destructive){[weak self] _ in self?.recovery=nil;discard()})
    recovery=alert;presenter.present(alert,animated:!UIAccessibility.isReduceMotionEnabled)
  }

  @MainActor private final class Editor:UIViewController,PKCanvasViewDelegate {
    private var bitmap:UIImage
    private let image=UIImageView()
    private let ink=PKCanvasView()
    private let crop=CropArea()
    private let toolbar=UIView()
    private let tools=ZoenTelegramAttachmentGlassView()
    private let cancelGlass=ZoenTelegramAttachmentGlassView()
    private let saveGlass=ZoenTelegramAttachmentGlassView()
    private let cancel=UIButton(type:.custom)
    private let save=UIButton(type:.custom)
    private let cropButton=UIButton(type:.custom)
    private let paintButton=UIButton(type:.custom)
    private let rotateButton=UIButton(type:.custom)
    private let undo=UIButton(type:.system)
    private let palette=UIStackView()
    private var cropping=false
    private var painting=false
    private var dirty=false
    private var retired=false
    private var previousSize=CGSize.zero
    var cancelled:(()->Void)?
    var saved:((Data)->Void)?
    init(file:URL,title:String)throws {
      guard file.isFileURL,
        let source=CGImageSourceCreateWithURL(file as CFURL,[kCGImageSourceShouldCache:false] as CFDictionary),
        let cg=CGImageSourceCreateThumbnailAtIndex(source,0,[kCGImageSourceCreateThumbnailFromImageAlways:true,kCGImageSourceCreateThumbnailWithTransform:true,kCGImageSourceThumbnailMaxPixelSize:4096,kCGImageSourceShouldCacheImmediately:false] as CFDictionary) else{throw ZoenFilePreview.Failure.editedMedia}
      bitmap=UIImage(cgImage:cg)
      super.init(nibName:nil,bundle:nil)
      self.title=title;modalPresentationStyle = .fullScreen;isModalInPresentation=true
    }
    required init?(coder:NSCoder){return nil}
    override var prefersStatusBarHidden:Bool{true}
    override var supportedInterfaceOrientations:UIInterfaceOrientationMask{.allButUpsideDown}
    override func viewDidLoad(){
      super.viewDidLoad();view.backgroundColor = .black;overrideUserInterfaceStyle = .dark
      image.image=bitmap;image.contentMode = .scaleAspectFit
      ink.backgroundColor = .clear;ink.isOpaque=false;ink.drawingPolicy = .anyInput
      ink.isScrollEnabled=false;ink.delegate=self;ink.tool=PKInkingTool(.pen,color:.white,width:4)
      ink.isUserInteractionEnabled=false;crop.isHidden=true
      crop.changed={[weak self] in self?.dirty=true}
      for child in [image,ink,crop,toolbar,undo,palette]{view.addSubview(child)}
      for glass in [cancelGlass,tools,saveGlass]{toolbar.addSubview(glass)}
      cancelGlass.contentView.addSubview(cancel);saveGlass.contentView.addSubview(save)
      for button in [cropButton,paintButton,rotateButton]{tools.contentView.addSubview(button)}
      configure(cancel,glyph:.close,label:NSLocalizedString("Cancel",comment:"Cancel media edit")){[weak self] in self?.cancelPressed()}
      configure(save,glyph:.done,label:NSLocalizedString("Save",comment:"Save media edit")){[weak self] in self?.savePressed()}
      configure(cropButton,glyph:.crop,label:NSLocalizedString("Crop",comment:"Crop image")){[weak self] in self?.toggleCrop()}
      configure(paintButton,glyph:.pencil,label:NSLocalizedString("Draw",comment:"Draw on image")){[weak self] in self?.togglePaint()}
      configure(rotateButton,glyph:.rotate,label:NSLocalizedString("Rotate",comment:"Rotate image")){[weak self] in self?.rotate()}
      undo.setTitle(NSLocalizedString("Undo",comment:"Undo ink stroke"),for:.normal);undo.tintColor = .white
      undo.addAction(UIAction{[weak self] _ in self?.ink.undoManager?.undo()},for:.touchUpInside)
      palette.axis = .horizontal;palette.spacing=12;palette.distribution = .fillEqually
      for color in [UIColor.white,.black,.systemRed,.systemYellow,.systemGreen,.systemBlue] {
        let button=UIButton(type:.custom);button.backgroundColor=color;button.layer.cornerRadius=12;button.layer.borderWidth=1;button.layer.borderColor=UIColor.white.withAlphaComponent(0.6).cgColor
        button.addAction(UIAction{[weak self] _ in self?.ink.tool=PKInkingTool(.pen,color:color,width:4)},for:.touchUpInside)
        button.accessibilityLabel=color.accessibilityName;palette.addArrangedSubview(button)
      }
      palette.isHidden=true;undo.isHidden=true
    }
    private func configure(_ button:UIButton,glyph:ZoenTelegramAttachmentGlyph,label:String,action:@escaping()->Void){
      button.setImage(ZoenTelegramAttachmentGlyph.image(glyph,pointSize:CGSize(width:28,height:28)),for:.normal)
      button.tintColor = .white;button.accessibilityLabel=label
      button.addAction(UIAction{_ in action()},for:.touchUpInside)
    }
    override func viewDidLayoutSubviews(){
      super.viewDidLayoutSubviews()
      let safe=view.safeAreaInsets,size=view.bounds.size
      let region=CGRect(x:safe.left+16,y:safe.top+12,width:max(1,size.width-safe.left-safe.right-32),height:max(1,size.height-safe.top-safe.bottom-130))
      let scale=min(region.width/max(1,bitmap.size.width),region.height/max(1,bitmap.size.height))
      let fit=CGSize(width:bitmap.size.width*scale,height:bitmap.size.height*scale)
      let frame=CGRect(x:region.midX-fit.width/2,y:region.midY-fit.height/2,width:fit.width,height:fit.height)
      if previousSize != size,!ink.drawing.bounds.isEmpty,previousSize.width>0 {
        let old=ink.bounds.size
        if old.width>0,old.height>0 {ink.drawing=ink.drawing.transformed(using:CGAffineTransform(scaleX:frame.width/old.width,y:frame.height/old.height))}
      }
      previousSize=size;image.frame=frame;ink.frame=frame;crop.frame=frame
      toolbar.frame=CGRect(x:0,y:size.height-safe.bottom-49,width:size.width,height:49)
      cancelGlass.frame=CGRect(x:26,y:0,width:44,height:44);saveGlass.frame=CGRect(x:size.width-70,y:0,width:44,height:44)
      tools.frame=CGRect(x:floor((size.width-152)/2),y:0,width:152,height:44)
      for glass in [cancelGlass,saveGlass,tools]{glass.update(isDark:true,cornerRadius:22,isInteractive:true)}
      cancel.frame=cancelGlass.bounds;save.frame=saveGlass.bounds
      for (index,button) in [cropButton,paintButton,rotateButton].enumerated(){button.frame=CGRect(x:CGFloat(index)*54,y:0,width:44,height:44)}
      palette.frame=CGRect(x:floor((size.width-204)/2),y:toolbar.frame.minY-40,width:204,height:24)
      undo.frame=CGRect(x:safe.left+16,y:safe.top+8,width:60,height:44)
    }
    func canvasViewDrawingDidChange(_ canvasView:PKCanvasView){dirty=true;undo.isEnabled=ink.undoManager?.canUndo==true}
    private func toggleCrop(){
      guard !retired else{return}
      if cropping {applyCrop()} else {commitInk();cropping=true}
      painting=false
      crop.isHidden = !cropping;ink.isUserInteractionEnabled=false;palette.isHidden=true;undo.isHidden=true
      cropButton.alpha=cropping ? 0.55:1;paintButton.alpha=1
    }
    private func togglePaint(){
      guard !retired else{return};applyCrop();painting.toggle();cropping=false;crop.isHidden=true
      ink.isUserInteractionEnabled=painting;palette.isHidden = !painting;undo.isHidden = !painting
      paintButton.alpha=painting ? 0.55:1;cropButton.alpha=1
    }
    private func commitInk(){
      guard !ink.drawing.bounds.isEmpty,ink.bounds.width>0 else{return}
      let format=UIGraphicsImageRendererFormat();format.scale=1;format.opaque=false
      let overlay=ink.drawing.image(from:ink.bounds,scale:bitmap.size.width/ink.bounds.width)
      bitmap=UIGraphicsImageRenderer(size:bitmap.size,format:format).image{_ in bitmap.draw(in:CGRect(origin:.zero,size:bitmap.size));overlay.draw(in:CGRect(origin:.zero,size:bitmap.size))}
      ink.drawing=PKDrawing();image.image=bitmap;dirty=true
    }
    private func applyCrop(){
      guard cropping else{return};commitInk()
      let normalized=crop.normalized
      let rect=CGRect(x:normalized.minX*bitmap.size.width,y:normalized.minY*bitmap.size.height,width:normalized.width*bitmap.size.width,height:normalized.height*bitmap.size.height).integral
      if rect.width>0,rect.height>0,let cg=bitmap.cgImage?.cropping(to:rect){bitmap=UIImage(cgImage:cg);image.image=bitmap;dirty=true}
      crop.reset();cropping=false;crop.isHidden=true;view.setNeedsLayout();view.layoutIfNeeded()
    }
    private func rotate(){
      guard !retired else{return};applyCrop();commitInk()
      let old=bitmap,newSize=CGSize(width:old.size.height,height:old.size.width)
      let format=UIGraphicsImageRendererFormat();format.scale=1
      bitmap=UIGraphicsImageRenderer(size:newSize,format:format).image{context in
        context.cgContext.translateBy(x:newSize.width/2,y:newSize.height/2);context.cgContext.rotate(by:.pi/2)
        old.draw(in:CGRect(x:-old.size.width/2,y:-old.size.height/2,width:old.size.width,height:old.size.height))
      }
      image.image=bitmap;dirty=true;view.setNeedsLayout();view.layoutIfNeeded()
    }
    private func savePressed(){
      guard !retired else{return}
      applyCrop();commitInk()
      guard let bytes=bitmap.pngData(),!bytes.isEmpty,UInt64(bytes.count)<=ZoenMediaFiles.maximumBytes else{show(error:ZoenFilePreview.Failure.editedMedia);return}
      // Existing native capture creates the one owned file and performs the
      // same validation/CAS/recovery path. The editor creates no temp URL.
      saved?(bytes)
    }
    private func cancelPressed(){
      guard !retired else{return}
      guard dirty else{cancelled?();return}
      let alert=UIAlertController(title:NSLocalizedString("Discard changes?",comment:"Cancel edited media"),message:nil,preferredStyle:.alert)
      alert.addAction(UIAlertAction(title:NSLocalizedString("Keep Editing",comment:"Keep editing image"),style:.cancel))
      alert.addAction(UIAlertAction(title:NSLocalizedString("Discard",comment:"Discard image changes"),style:.destructive){[weak self] _ in self?.cancelled?()})
      present(alert,animated:!UIAccessibility.isReduceMotionEnabled)
    }
    func show(error:Error){
      guard !retired,presentedViewController==nil else{return}
      let alert=UIAlertController(title:NSLocalizedString("Unable to save changes",comment:"Media edit failure"),message:error.localizedDescription,preferredStyle:.alert)
      alert.addAction(UIAlertAction(title:NSLocalizedString("OK",comment:"Acknowledge media edit error"),style:.default));present(alert,animated:!UIAccessibility.isReduceMotionEnabled)
    }
    func retire(){
      guard !retired else{return};retired=true;view.isUserInteractionEnabled=false;view.isHidden=true
      ink.delegate=nil;ink.drawing=PKDrawing();cancelled=nil;saved=nil
    }
  }
}

/// Crop corners/edges use the original 44pt touch regions, two-point outline
/// and bounded normalized rectangle. Pixel operations happen only on Save.
@MainActor private final class CropArea:UIView {
  var changed:(()->Void)?
  private let outline=CAShapeLayer();private let grid=CAShapeLayer()
  private let corners=UIImageView()
  private var rectangle=CGRect(x:0,y:0,width:1,height:1)
  private var initial=CGRect.zero;private var handle=0
  var normalized:CGRect{rectangle}
  override init(frame:CGRect){
    super.init(frame:frame);backgroundColor = .clear
    outline.fillColor=UIColor.clear.cgColor;outline.strokeColor=UIColor.white.cgColor;outline.lineWidth=2
    grid.fillColor=UIColor.clear.cgColor;grid.strokeColor=UIColor.white.withAlphaComponent(0.6).cgColor;grid.lineWidth=1
    layer.addSublayer(grid);layer.addSublayer(outline);addGestureRecognizer(UIPanGestureRecognizer(target:self,action:#selector(pan(_:))))
    corners.image=ZoenTelegramAttachmentGlyph.image(.cropCorners)?.resizableImage(withCapInsets:UIEdgeInsets(top:20,left:20,bottom:20,right:20))
    corners.isUserInteractionEnabled=false;addSubview(corners)
    accessibilityLabel=NSLocalizedString("Crop area",comment:"Image crop rectangle")
  }
  required init?(coder:NSCoder){return nil}
  func reset(){rectangle=CGRect(x:0,y:0,width:1,height:1);setNeedsLayout()}
  override func layoutSubviews(){
    super.layoutSubviews();let rect=CGRect(x:rectangle.minX*bounds.width,y:rectangle.minY*bounds.height,width:rectangle.width*bounds.width,height:rectangle.height*bounds.height)
    outline.frame=bounds;outline.path=UIBezierPath(rect:rect).cgPath
    corners.frame=rect.insetBy(dx:-2,dy:-2)
    let lines=UIBezierPath()
    for n in 1...2{let f=CGFloat(n)/3;lines.move(to:CGPoint(x:rect.minX+rect.width*f,y:rect.minY));lines.addLine(to:CGPoint(x:rect.minX+rect.width*f,y:rect.maxY));lines.move(to:CGPoint(x:rect.minX,y:rect.minY+rect.height*f));lines.addLine(to:CGPoint(x:rect.maxX,y:rect.minY+rect.height*f))}
    grid.frame=bounds;grid.path=lines.cgPath
  }
  @objc private func pan(_ gesture:UIPanGestureRecognizer){
    guard bounds.width>0,bounds.height>0 else{return}
    let point=gesture.location(in:self),delta=gesture.translation(in:self)
    if gesture.state == .began{
      initial=rectangle;let left=rectangle.minX*bounds.width,right=rectangle.maxX*bounds.width,top=rectangle.minY*bounds.height,bottom=rectangle.maxY*bounds.height
      handle=(abs(point.x-left)<22 ? 1:abs(point.x-right)<22 ? 2:0) | (abs(point.y-top)<22 ? 4:abs(point.y-bottom)<22 ? 8:0)
    }
    guard gesture.state == .changed else{return}
    let dx=delta.x/bounds.width,dy=delta.y/bounds.height
    let minW=min(1,44/bounds.width),minH=min(1,44/bounds.height)
    var x=initial.minX,y=initial.minY,r=initial.maxX,b=initial.maxY
    if handle==0{x=min(max(0,x+dx),1-initial.width);y=min(max(0,y+dy),1-initial.height);r=x+initial.width;b=y+initial.height}
    else{if handle&1 != 0{x=min(max(0,x+dx),r-minW)};if handle&2 != 0{r=max(min(1,r+dx),x+minW)};if handle&4 != 0{y=min(max(0,y+dy),b-minH)};if handle&8 != 0{b=max(min(1,b+dy),y+minH)}}
    rectangle=CGRect(x:x,y:y,width:r-x,height:b-y);changed?();setNeedsLayout()
  }
}

private extension UIColor {
  var accessibilityName:String {
    if self == .white{return NSLocalizedString("White",comment:"Ink color")}
    if self == .black{return NSLocalizedString("Black",comment:"Ink color")}
    if self == .systemRed{return NSLocalizedString("Red",comment:"Ink color")}
    if self == .systemYellow{return NSLocalizedString("Yellow",comment:"Ink color")}
    if self == .systemGreen{return NSLocalizedString("Green",comment:"Ink color")}
    return NSLocalizedString("Blue",comment:"Ink color")
  }
}
