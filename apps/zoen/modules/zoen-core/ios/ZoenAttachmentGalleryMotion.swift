import UIKit

// Translated from Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838,
// TGModernGalleryController.m:910–983 and JNWSpringAnimation.m:194–285.
// GPL-2.0-or-later, GPLv3 selected. Uses public keyframe animation APIs only.
@MainActor enum ZoenAttachmentGalleryMotion {
  static func samples(from:[CGFloat],to:[CGFloat],entering:Bool)->[[CGFloat]] {
    guard from.count==to.count,!from.isEmpty,from.count<=4,
      (from+to).allSatisfy(\.isFinite) else{return []}
    let source=from.map{CGFloat(Float($0))},target=to.map{CGFloat(Float($0))}
    let distance=zip(target,source).map(-)
    let damping:CGFloat=entering ? 25:30,mass:CGFloat=0.8,stiffness:CGFloat=300
    let frequency=sqrt(max(0,stiffness/mass-pow(damping,2)/(4*pow(mass,2))))
    var previous=Array(repeating:CGFloat.zero,count:from.count),result:[[CGFloat]]=[]
    // The original steps at 1ms until both movement and final error reach a
    // relative 0.0001 threshold. A finite 10s ceiling fences invalid geometry.
    for step in 0..<10000 {
      let t=CGFloat(step)*0.001
      let proposed=zip(source,distance).map{start,delta in start+delta-delta*exp(-damping/(2*mass)*t)*cos(frequency*t)}
      let complete=source.indices.allSatisfy{index in
        abs(proposed[index]-previous[index])<=0.0001*abs(distance[index]) &&
        abs(previous[index]-target[index])<=0.0001*abs(distance[index])
      }
      if complete{break}
      result.append(proposed);previous=proposed
    }
    return result
  }
  static func animate(view:UIView,from:CGRect,to:CGRect,entering:Bool,completion:@escaping()->Void)->CAAnimationDelegate? {
    guard !UIAccessibility.isReduceMotionEnabled,from.width>0,from.height>0,to.width>0,to.height>0 else{view.frame=to;completion();return nil}
    view.bounds=CGRect(origin:.zero,size:to.size);view.center=CGPoint(x:to.midX,y:to.midY)
    let position=samples(from:[from.midX,from.midY],to:[to.midX,to.midY],entering:entering)
    let scale=samples(from:[from.width/to.width,from.height/to.height],to:[1,1],entering:entering)
    guard !position.isEmpty,!scale.isEmpty else{completion();return nil}
    let factor:Double=entering ? 1.8:2
    let p=CAKeyframeAnimation(keyPath:"position");p.values=position.map{NSValue(cgPoint:CGPoint(x:$0[0],y:$0[1]))};p.duration=Double(position.count)*0.001*factor
    let x=CAKeyframeAnimation(keyPath:"transform.scale.x");x.values=scale.map{NSNumber(value:Double($0[0]))};x.duration=Double(scale.count)*0.001*factor
    let y=CAKeyframeAnimation(keyPath:"transform.scale.y");y.values=scale.map{NSNumber(value:Double($0[1]))};y.duration=x.duration
    let group=CAAnimationGroup();group.animations=[p,x,y];group.duration=max(p.duration,x.duration)
    let delegate=GalleryMotionCompletion(completion:completion);group.delegate=delegate
    view.layer.add(group,forKey:"zoen.telegram.gallery.hero")
    return delegate
  }
}
@MainActor private final class GalleryMotionCompletion:NSObject,@preconcurrency CAAnimationDelegate {
  private var completion:(()->Void)?
  init(completion:@escaping()->Void){self.completion=completion}
  func animationDidStop(_ anim:CAAnimation,finished flag:Bool){let callback=completion;completion=nil;callback?()}
}
