import ExpoModulesCore
import UIKit

/// Zero-size Expo adapter for the shared native gallery. The mounted draft
/// owner supplies an exact immutable context/revision and admitted cache leases.
/// This adapter never copies, deletes, edits or sends a media file itself.
@MainActor final class ZoenAttachmentGalleryHost: ExpoView {
  let onAction = EventDispatcher()
  let onClosed = EventDispatcher()
  var binding = ""
  var model = "{}"
  private var owner: Owner?
  private var observers: [NSObjectProtocol] = []
  private var terminal = Set<String>()
  private var admissionRetired = false
  private static let hosts = NSHashTable<ZoenAttachmentGalleryHost>.weakObjects()
  private static var enabled = true
  private static var epoch = UUID()
  @MainActor private final class Owner {
    let token: String
    let context: String
    let gallery: ZoenAttachmentGalleryContent
    let cover = UIView()
    var accessibility: ZoenAttachmentAccessibilityLease?
    weak var root:UIView?
    var revision: String
    var retiring = false
    var suspended = false
    var propSuspended = false
    var childLeases = Set<UUID>()
    init(token:String,context:String,revision:String,gallery:ZoenAttachmentGalleryContent,root:UIView) {
      self.token=token;self.context=context;self.revision=revision;self.gallery=gallery
      self.root=root;accessibility=ZoenAttachmentAccessibilityLease(root:root)
    }
  }
  required init(appContext:AppContext?=nil) {
    super.init(appContext:appContext)
    isUserInteractionEnabled=false
    Self.hosts.add(self)
    for name in [UIApplication.willResignActiveNotification,UIApplication.didEnterBackgroundNotification] {
      observers.append(NotificationCenter.default.addObserver(forName:name,object:nil,queue:.main){[weak self] _ in
        MainActor.assumeIsolated{self?.close(reason:"inactive",sensitive:true)}
      })
    }
    for name in [UIResponder.keyboardWillChangeFrameNotification,UIResponder.keyboardDidChangeFrameNotification] {
      observers.append(NotificationCenter.default.addObserver(forName:name,object:nil,queue:.main){[weak self] note in
        MainActor.assumeIsolated{self?.keyboard(note)}
      })
    }
  }
  deinit {for observer in observers{NotificationCenter.default.removeObserver(observer)}}
  override func didMoveToWindow(){super.didMoveToWindow();if window==nil{close(reason:"unmounted",sensitive:true)}else{applyProps()}}
  override func layoutSubviews(){super.layoutSubviews();layoutOwner()}
  func applyProps() {
    guard Self.enabled,!admissionRetired else{close(reason:"retired",sensitive:true);return}
    guard let request=Self.json(binding,limit:8192),let active=request["active"] as? Bool,
      let token=request["token"] as? String,!token.isEmpty,token.utf8.count<=128,
      let context=request["context"] as? String,!context.isEmpty,context.utf8.count<=2048 else{close(reason:"invalid",sensitive:true);return}
    if let owner {
      guard owner.token==token,owner.context==context,active else{close(reason:active ? "replaced":"closed",sensitive:false);if active{applyProps()};return}
      guard !owner.retiring else{return}
      owner.propSuspended=request["suspended"] as? Bool == true
      refreshSuspension(owner)
      do {
        guard let value=Self.json(model,limit:262144),value["source"] as? String == "draft",
          let revision=value["revision"] as? String else{throw Failure.invalid}
        try owner.gallery.update(model:value,appearance:appearance(request))
        owner.revision=revision
        layoutOwner()
      }catch{close(reason:"invalid",sensitive:true)}
      return
    }
    guard active else{if request["closing"] as? Bool == true{reject(token:token,context:context,reason:"closed")};return}
    // Prop delivery may precede Fabric's window attachment. That state is
    // pending; it must not permanently reject an otherwise valid owner.
    guard let window else{return}
    guard request["suspended"] as? Bool != true else{return}
    guard !terminal.contains(token),window.windowScene?.activationState == .foregroundActive,
      let root=window.rootViewController?.view,root.window===window,
      let value=Self.json(model,limit:262144),value["source"] as? String == "draft",
      let revision=value["revision"] as? String else{reject(token:token,context:context,reason:"unavailable");return}
    for other in Self.hosts.allObjects where other !== self {other.close(reason:"replaced",sensitive:true)}
    let gallery=ZoenAttachmentGalleryContent{[weak self] action,payload in self?.send(action,payload:payload,token:token,context:context)}
    do{try gallery.update(model:value,appearance:appearance(request))}
    catch{gallery.retire();reject(token:token,context:context,reason:"invalid");return}
    let owner=Owner(token:token,context:context,revision:revision,gallery:gallery,root:root)
    self.owner=owner
    owner.cover.backgroundColor = .clear
    owner.cover.accessibilityViewIsModal=true
    owner.cover.addSubview(gallery.view)
    window.addSubview(owner.cover)
    layoutOwner()
    UIAccessibility.post(notification:.screenChanged,argument:gallery.view)
  }
  private func layoutOwner() {
    guard let owner,let window,owner.cover.superview===window else{return}
    owner.cover.frame=window.bounds;owner.gallery.view.frame=owner.cover.bounds
    owner.gallery.updatePresentation(keyboardHeight:keyboardHeight,safeArea:window.safeAreaInsets)
  }
  private func suspend(_ owner:Owner,requested:Bool) {
    guard owner.suspended != requested else{return}
    owner.suspended=requested;owner.cover.isHidden=requested
    owner.gallery.setVisible(!requested)
    if requested {owner.accessibility?.release();owner.accessibility=nil}
    else if let root=owner.root {owner.accessibility=ZoenAttachmentAccessibilityLease(root:root)}
  }
  private func refreshSuspension(_ owner:Owner) {
    guard self.owner===owner,!owner.retiring else{return}
    let request=Self.json(binding,limit:8192)
    let admitted=request?["active"] as? Bool == true && request?["token"] as? String == owner.token &&
      request?["context"] as? String == owner.context && window?.windowScene?.activationState == .foregroundActive
    suspend(owner,requested:owner.propSuspended || !owner.childLeases.isEmpty || !admitted)
  }
  /// Yield synchronously before a native editor/picker is admitted. Releasing
  /// this display lease cannot resurrect a replaced/retired owner or override
  /// a still-suspended live React binding.
  static func yieldForChildPresentation() -> @MainActor @Sendable () -> Void {
    var leases:[ChildLease]=[]
    for host in hosts.allObjects {
      guard let owner=host.owner,!owner.retiring else{continue}
      let id=UUID();owner.childLeases.insert(id);host.refreshSuspension(owner)
      leases.append(ChildLease(host:host,owner:owner,id:id))
    }
    let captured=leases
    return {
      for lease in captured {
        guard let host=lease.host,let owner=lease.owner,host.owner===owner,!owner.retiring else{continue}
        owner.childLeases.remove(lease.id);host.refreshSuspension(owner)
      }
    }
  }
  private final class ChildLease {
    weak var host:ZoenAttachmentGalleryHost?
    weak var owner:Owner?
    let id:UUID
    init(host:ZoenAttachmentGalleryHost,owner:Owner,id:UUID){self.host=host;self.owner=owner;self.id=id}
  }
  private var keyboardHeight:CGFloat=0
  private func keyboard(_ note:Notification) {
    guard let owner,!owner.retiring,let window else{return}
    let frame=(note.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? NSValue)?.cgRectValue ?? .zero
    let intersection=window.bounds.intersection(window.convert(frame,from:nil))
    let height=intersection.isNull ? 0 : max(0,intersection.height)
    guard height==0 || owner.gallery.hasKeyboardFocus else{return}
    keyboardHeight=height
    let duration=note.userInfo?[UIResponder.keyboardAnimationDurationUserInfoKey] as? Double ?? 0.25
    let curve=note.userInfo?[UIResponder.keyboardAnimationCurveUserInfoKey] as? UInt ?? 0
    UIView.animate(withDuration:UIAccessibility.isReduceMotionEnabled ? 0 : duration,delay:0,
      options:[UIView.AnimationOptions(rawValue:curve<<16),.beginFromCurrentState],animations:{[weak self] in self?.layoutOwner()})
  }
  private func send(_ action:String,payload:[String:Any],token:String,context:String) {
    guard let owner,!owner.retiring,!owner.suspended,owner.token==token,owner.context==context,
      let window,owner.cover.superview===window,window.windowScene?.activationState == .foregroundActive,
      let request=Self.json(binding,limit:8192),request["active"] as? Bool == true,
      request["token"] as? String==token,request["context"] as? String==context,
      payload["revision"] as? String==owner.revision else{return}
    let allowed=Set(["preview","select","selected","back","close","caption","send","edit","remove","add","retry","reorder"])
    guard allowed.contains(action) else{return}
    var event:[String:Any]=["token":token,"context":context,"revision":owner.revision,"action":action]
    if let id=payload["id"] as? String,id.utf8.count<=2048{event["id"]=id}
    if action=="caption" || action=="send" || action=="edit" {
      guard let text=payload["text"] as? String,text.utf16.count<=4096,!text.contains("\0") else{return}
      event["text"]=text
    }
    if action=="reorder",let ids=payload["ids"] as? [String],ids.count<=10,Set(ids).count==ids.count{event["ids"]=ids}
    if action=="add",let kind=payload["kind"] as? String,["photo","camera","document"].contains(kind){event["kind"]=kind}
    // The JS owner performs close-before-cache-release, durable CAS and Send.
    // In particular this is not a native auto-dismiss on an accepted tap.
    onAction(event)
  }
  private func close(reason:String,sensitive:Bool) {
    guard let owner,!owner.retiring else{return}
    owner.retiring=true;terminal.insert(owner.token)
    if sensitive{owner.gallery.conceal()}else{owner.gallery.retire()}
    owner.cover.removeFromSuperview();owner.gallery.view.removeFromSuperview()
    owner.accessibility?.release();owner.accessibility=nil;self.owner=nil;keyboardHeight=0
    onClosed(["token":owner.token,"context":owner.context,"reason":reason])
  }
  private func reject(token:String,context:String,reason:String){guard !terminal.contains(token) else{return};terminal.insert(token);onClosed(["token":token,"context":context,"reason":reason])}
  @discardableResult static func retireAll() async -> UUID {
    let ticket=UUID();epoch=ticket;enabled=false
    for host in hosts.allObjects{host.admissionRetired=true;host.close(reason:"retired",sensitive:true)}
    return ticket
  }
  @discardableResult static func resumeAdmissions(expected:UUID)->Bool{guard epoch==expected else{return false};enabled=true;return true}
  private static func json(_ text:String,limit:Int)->[String:Any]?{guard let data=text.data(using:.utf8),data.count<=limit else{return nil};return (try? JSONSerialization.jsonObject(with:data)) as? [String:Any]}
  private func appearance(_ request:[String:Any])->ZoenKeyboardPanelAppearance {
    let values=request["appearance"] as? [String:String] ?? [:]
    func color(_ name:String,_ fallback:UIColor)->UIColor {
      guard let value=values[name],value.hasPrefix("#"),[7,9].contains(value.count),let raw=UInt64(value.dropFirst(),radix:16) else{return fallback}
      let rgb=value.count==9 ? raw>>8:raw
      return UIColor(red:CGFloat((rgb>>16)&255)/255,green:CGFloat((rgb>>8)&255)/255,blue:CGFloat(rgb&255)/255,alpha:value.count==9 ? CGFloat(raw&255)/255:1)
    }
    return ZoenKeyboardPanelAppearance(background:color("background",.black),text:color("text",.white),secondary:color("secondary",.secondaryLabel),accent:color("accent",.systemBlue),separator:color("separator",.separator),bottomInset:window?.safeAreaInsets.bottom ?? 0)
  }
  private enum Failure:Error{case invalid}
}
