import AVFoundation
import ImageIO
import Photos
import UIKit

enum GalleryFailure:Error {case invalid}
struct GalleryItem:Equatable {
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

@MainActor final class GalleryRoot:UIView {
  var onLayout:(()->Void)?;var accessibilityEscape:(()->Void)?
  override func layoutSubviews(){super.layoutSubviews();onLayout?()}
  override func accessibilityPerformEscape()->Bool{accessibilityEscape?();return true}
}

/// Each surface owns only cancelable display requests. No asset is copied or
/// saved; replacement and retirement fence every asynchronous result.
@MainActor class GalleryAssetView:UIView {
  let image=UIImageView()
  let item:GalleryItem
  private let manager=PHImageManager()
  private let targetPixels:CGFloat
  private let allowNetwork:Bool
  private let progress=UIProgressView(progressViewStyle:.bar)
  private let retry=UIButton(type:.system)
  private let fallback=UILabel()
  private let videoThumbnail=ZoenAttachmentVideoThumbnail()
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
  override func didMoveToWindow(){
    super.didMoveToWindow()
    guard item.video,item.uri != nil,!retired else{return}
    if window == nil{generation=UUID();videoThumbnail.cancel();progress.isHidden=true}
    else if image.image == nil{load()}
  }
  override func layoutSubviews(){super.layoutSubviews();if image.superview===self{image.frame=bounds};progress.frame=CGRect(x:bounds.midX-50,y:bounds.midY+40,width:100,height:2);retry.frame=CGRect(x:bounds.midX-60,y:bounds.midY-22,width:120,height:44);fallback.frame=CGRect(x:24,y:bounds.midY+65,width:max(0,bounds.width-48),height:44)}
  private func load(){
    guard !retired else{return}
    if let request{manager.cancelImageRequest(request)}
    videoThumbnail.cancel()
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
      if item.video {
        guard window != nil else{return}
        progress.isHidden=false
        videoThumbnail.load(url:url,maximumPixels:targetPixels){[weak self] result in
          guard let self,!retired,generation==ticket,window != nil else{return}
          progress.isHidden=true
          switch result {
          case let .success(cg):
            let value=UIImage(cgImage:cg);image.image=value;fallback.isHidden=true;loaded?(value)
          case .failure:retry.isHidden=false
          }
        }
        return
      }
      let options:[CFString:Any]=[kCGImageSourceShouldCache:false]
      if let source=CGImageSourceCreateWithURL(url as CFURL,options as CFDictionary),let cg=CGImageSourceCreateThumbnailAtIndex(source,0,[kCGImageSourceCreateThumbnailFromImageAlways:true,kCGImageSourceCreateThumbnailWithTransform:true,kCGImageSourceThumbnailMaxPixelSize:targetPixels] as CFDictionary){
        image.image=UIImage(cgImage:cg);fallback.isHidden=true
      } else {retry.isHidden=false}
    }
  }
  func promoteStatus(){for view in [fallback,progress,retry]{bringSubviewToFront(view)}}
  func retire(){guard !retired else{return};retired=true;generation=UUID();videoThumbnail.cancel();if let request{manager.cancelImageRequest(request)};request=nil;loaded=nil}
}

@MainActor final class GalleryMediaView:GalleryAssetView,UIScrollViewDelegate {
  private let zoom=UIScrollView()
  private var player:AVPlayer?
  private var playerLayer:AVPlayerLayer?
  private let conversation:Bool
  private var playback:ZoenConversationVideoPlayback?
  private var videoControls:ZoenConversationVideoControls?
  private var chromeVisible=true
  private var chromeSafe=UIEdgeInsets.zero
  private var chromeCaption=CGRect.zero
  private var playerStatus:NSKeyValueObservation?
  private var videoRequest:PHImageRequestID?
  private let videoManager=PHImageManager()
  private let play=UIButton(type:.system)
  var toggle:(()->Void)?;var zoomChanged:((Bool)->Void)?
  var active=false {didSet{
    guard !retired else{return}
    if active,conversation,item.video,playback == nil,let uri=item.uri {install(AVPlayerItem(url:uri),autoplay:false)}
    playback?.setActive(active);if !active{stop()};refreshVideoPresentation()
  }}
  var isZoomed:Bool{abs(zoom.zoomScale-1)>0.01}
  init(item:GalleryItem,allowNetwork:Bool,conversation:Bool=false){
    self.conversation=conversation
    super.init(item:item,targetPixels:2048,allowNetwork:allowNetwork)
    image.removeFromSuperview();zoom.addSubview(image);addSubview(zoom)
    promoteStatus()
    zoom.delegate=self;zoom.minimumZoomScale=1;zoom.maximumZoomScale=3;zoom.bouncesZoom=true
    zoom.showsHorizontalScrollIndicator=false;zoom.showsVerticalScrollIndicator=false
    image.contentMode = .scaleAspectFit;image.isUserInteractionEnabled=false
    let tap=UITapGestureRecognizer(target:self,action:#selector(singleTap));let double=UITapGestureRecognizer(target:self,action:#selector(doubleTap(_:)));double.numberOfTapsRequired=2;tap.require(toFail:double)
    zoom.addGestureRecognizer(tap);zoom.addGestureRecognizer(double)
    play.setImage(ZoenTelegramAttachmentGlyph.image(.play),for:.normal);play.backgroundColor=UIColor.black.withAlphaComponent(0.3);play.layer.cornerRadius=30;play.tintColor = .white
    play.accessibilityLabel=item.name;play.addAction(UIAction{[weak self] _ in self?.playVideo()},for:.touchUpInside);play.isHidden = !item.video;addSubview(play)
  }
  required init?(coder:NSCoder){return nil}
  override func layoutSubviews(){super.layoutSubviews();zoom.frame=bounds;if !isZoomed{image.frame=zoom.bounds};playerLayer?.frame=bounds;videoControls?.frame=bounds;play.frame=CGRect(x:bounds.midX-30,y:bounds.midY-30,width:60,height:60)}
  func viewForZooming(in scrollView:UIScrollView)->UIView?{image}
  func scrollViewDidZoom(_ scrollView:UIScrollView){zoomChanged?(isZoomed)}
  @objc private func singleTap(){if conversation{toggle?()}else if item.video,let player,player.rate>0{player.pause();play.isHidden=false}else{toggle?()}}
  @objc private func doubleTap(_ tap:UITapGestureRecognizer){
    guard !item.video,!retired,active else{return}
    if isZoomed{zoom.setZoomScale(1,animated:!UIAccessibility.isReduceMotionEnabled)}
    else{let point=tap.location(in:image);let size=CGSize(width:zoom.bounds.width/3,height:zoom.bounds.height/3);zoom.zoom(to:CGRect(x:point.x-size.width/2,y:point.y-size.height/2,width:size.width,height:size.height),animated:!UIAccessibility.isReduceMotionEnabled)}
  }
  private func playVideo(){
    guard item.video,active,!retired else{return}
    if let player {
      if player.currentItem?.status == .failed,let uri=item.uri{install(AVPlayerItem(url:uri))}
      else if let playback {playback.toggle()}
      else {player.rate==0 ? player.play() : player.pause()};return
    }
    if let url=item.uri{install(AVPlayerItem(url:url))}
    else if let id=item.photoId,let asset=PHAsset.fetchAssets(withLocalIdentifiers:[id],options:nil).firstObject{
      let options=PHVideoRequestOptions();options.isNetworkAccessAllowed=true
      videoRequest=videoManager.requestPlayerItem(forVideo:asset,options:options){[weak self] item,_ in
        Task{@MainActor [weak self] in guard let self,!retired,active,let item else{return};install(item)}
      }
    }
  }
  private func install(_ item:AVPlayerItem,autoplay:Bool=true){
    playerStatus=nil;player?.pause();player?.replaceCurrentItem(with:nil)
    let player=AVPlayer(playerItem:item);player.allowsExternalPlayback=false
    self.player=player
    let layer=AVPlayerLayer(player:player);layer.videoGravity = .resizeAspect
    self.layer.insertSublayer(layer,above:zoom.layer)
    playerLayer?.removeFromSuperlayer();playerLayer=layer;layer.frame=bounds
    if conversation {
      playback?.retire();videoControls?.retire();videoControls?.removeFromSuperview()
      let transport=ZoenConversationVideoPlayback(player:player)
      let controls=ZoenConversationVideoControls(playback:transport)
      playback=transport;videoControls=controls;addSubview(controls);controls.frame=bounds;bringSubviewToFront(play)
      transport.setActive(active);refreshVideoPresentation()
    }
    playerStatus=item.observe(\.status,options:[.initial,.new]){[weak self,weak player] item,_ in
      Task{@MainActor [weak self,weak player] in
        guard let self,!retired,self.player === player,player?.currentItem === item else{return}
        if item.status == .failed {play.isHidden=false;play.accessibilityLabel=NSLocalizedString("Try again",comment:"Retry conversation video")}
      }
    }
    play.isHidden=true
    if autoplay {if let playback {playback.play()}else{player.play()}}
  }
  func updateVideoPresentation(visible:Bool,safeArea:UIEdgeInsets,captionFrame:CGRect) {
    chromeVisible=visible;chromeSafe=safeArea;chromeCaption=captionFrame;refreshVideoPresentation()
  }
  private func refreshVideoPresentation() {
    videoControls?.updatePresentation(active:active,chromeVisible:chromeVisible,safeArea:chromeSafe,captionFrame:chromeCaption)
  }
  func stop(){if let playback {playback.pause()}else{player?.pause()}}

  override func retire(){
    guard !retired else{return}
    // Preserve the last rendered conversation frame through a user dismissal;
    // authority/player work retires now, and the parent removes this surface.
    if conversation,player != nil,let snapshot=snapshotView(afterScreenUpdates:false){snapshot.frame=bounds;addSubview(snapshot)}
    playback?.retire();videoControls?.retire()
    super.retire();stop();if let videoRequest{videoManager.cancelImageRequest(videoRequest)};videoRequest=nil;playerStatus=nil;player?.replaceCurrentItem(with:nil);player=nil;playerLayer?.removeFromSuperlayer();playerLayer=nil;playback=nil;videoControls=nil;toggle=nil;zoomChanged=nil
  }
}

@MainActor final class GallerySelectionTile:GalleryAssetView {
  var open:(()->Void)?
  init(item:GalleryItem){super.init(item:item);clipsToBounds=true;addGestureRecognizer(UITapGestureRecognizer(target:self,action:#selector(tap)));isAccessibilityElement=true;accessibilityLabel=item.name;accessibilityTraits = .button}
  required init?(coder:NSCoder){return nil}
  @objc private func tap(){if !retired{open?()}}
  override func retire(){super.retire();open=nil}
}

@MainActor final class GalleryStripCell:UICollectionViewCell {
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

@MainActor final class GalleryCheckButton:UIButton {
  private let badge=UIView()
  private let number=UILabel()
  override init(frame:CGRect){super.init(frame:frame);number.textAlignment = .center;number.textColor = .white;number.font = .systemFont(ofSize:15,weight:.semibold);badge.isUserInteractionEnabled=false;badge.addSubview(number);addSubview(badge)}
  required init?(coder:NSCoder){return nil}
  func update(order:Int?,accent:UIColor){number.text=order.map(String.init);accessibilityValue=number.text;badge.layer.borderColor=UIColor.white.cgColor;badge.layer.borderWidth=2;badge.backgroundColor=order==nil ? UIColor.black.withAlphaComponent(0.3) : accent;badge.layer.cornerRadius=14.5;isSelected=order != nil}
  override func layoutSubviews(){super.layoutSubviews();badge.frame=CGRect(x:(bounds.width-29)/2,y:(bounds.height-29)/2,width:29,height:29);number.frame=badge.bounds}
}

@MainActor final class GalleryCounterButton:UIButton {
  private let ring=UIView();private let number=UILabel();private let cross=UIImageView()
  override init(frame:CGRect){super.init(frame:frame);ring.isUserInteractionEnabled=false;ring.layer.cornerRadius=15.5;ring.layer.borderColor=UIColor.white.cgColor;ring.backgroundColor=UIColor.black.withAlphaComponent(0.5);number.font = .systemFont(ofSize:18,weight:.regular);number.textColor = .white;number.textAlignment = .center;cross.image=ZoenTelegramAttachmentGlyph.image(.close);cross.contentMode = .center;for child in [ring,number,cross]{addSubview(child)}}
  required init?(coder:NSCoder){return nil}
  func update(count:Int,expanded:Bool){number.text=String(count);number.isHidden=expanded;cross.isHidden = !expanded;accessibilityValue=String(count)}
  override func layoutSubviews(){super.layoutSubviews();ring.frame=CGRect(x:16.5,y:1,width:31,height:31);ring.layer.borderWidth=traitCollection.displayScale==3 ? 5/3 : 1.5;number.frame=CGRect(x:13,y:5+1/max(1,traitCollection.displayScale),width:38,height:22);cross.frame=CGRect(x:13,y:0,width:38,height:33)}
}
