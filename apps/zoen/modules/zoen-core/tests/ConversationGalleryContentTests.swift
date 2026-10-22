import UIKit
import XCTest

@MainActor final class ConversationGalleryContentTests: XCTestCase {
  private let appearance=ZoenKeyboardPanelAppearance(background:.black,text:.white,secondary:.gray,accent:.white,separator:.darkGray,bottomInset:0)
  private func model(_ selected:String="b",revision:String="one",neighborName:String="c")->[String:Any] {
    ["source":"conversation","screen":"preview","previewId":selected,"revision":revision,"selected":[String](),
      "items":["a","b","c","d"].map{["id":$0,"name":$0=="c" ? neighborName:$0,"mime":"image/png"]},
      "caption":["value":"QA caption \(selected)","editable":true,"sendEnabled":true],
      "canEdit":true,"canAdd":true,"canRemove":true,"position":"2 / 4"]
  }
  private func descendants(_ view:UIView)->[UIView] {[view]+view.subviews.flatMap(descendants)}
  private func find(_ view:UIView,_ id:String)->UIView? {descendants(view).first{$0.accessibilityIdentifier==id}}
  private func visible(_ view:UIView)->Bool {
    var current:UIView?=view
    while let value=current {if value.isHidden || value.alpha<0.01{return false};current=value.superview}
    return true
  }
  func testSelectedChildViewportAndReadOnlyControlsUseBoundedNeighborWindow() throws {
    let gallery=ZoenAttachmentGalleryContent{_,_ in XCTFail("Read-only update emitted an action")}
    gallery.view.frame=CGRect(x:0,y:0,width:390,height:844)
    try gallery.update(model:model(),appearance:appearance)
    gallery.updatePresentation(keyboardHeight:0,safeArea:.zero)
    let pager=try XCTUnwrap(find(gallery.view,"conversation-gallery-pager") as? UIScrollView)
    XCTAssertEqual(pager.bounds.width,430)
    XCTAssertEqual(pager.contentOffset.x,430)
    XCTAssertEqual(descendants(pager).filter{$0.accessibilityIdentifier?.hasPrefix("conversation-gallery-item-")==true}.count,3)
    let send=try XCTUnwrap(find(gallery.view,"attachment-approval-send"))
    XCTAssertFalse(visible(send))
    XCTAssertFalse(visible(try XCTUnwrap(find(gallery.view,"attachment-approval-edit"))))
    let caption=try XCTUnwrap(find(gallery.view,"conversation-gallery-caption") as? UITextView)
    XCTAssertFalse(caption.isEditable)
    XCTAssertEqual(caption.text,"QA caption b")
    gallery.retire()
  }
  func testNeighborReplacementPreservesCurrentPageAndReclaimsOnlyEvictedPage() throws {
    let gallery=ZoenAttachmentGalleryContent{_,_ in}
    gallery.view.frame=CGRect(x:0,y:0,width:390,height:844)
    try gallery.update(model:model(),appearance:appearance)
    let current=try XCTUnwrap(find(gallery.view,"conversation-gallery-item-b"))
    try gallery.update(model:model(revision:"two",neighborName:"arrived neighbor"),appearance:appearance)
    XCTAssertTrue(find(gallery.view,"conversation-gallery-item-b") === current)
    try gallery.update(model:model("c",revision:"three"),appearance:appearance)
    XCTAssertNil(find(gallery.view,"conversation-gallery-item-a"))
    XCTAssertNotNil(find(gallery.view,"conversation-gallery-item-d"))
    gallery.retire()
  }
  func testPagingEmitsExactChildButOldRevisionCannotNavigateSuccessor() throws {
    var events:[(String,[String:Any])]=[]
    let gallery=ZoenAttachmentGalleryContent{events.append(($0,$1))}
    gallery.view.frame=CGRect(x:0,y:0,width:390,height:844)
    try gallery.update(model:model(),appearance:appearance)
    let pager=try XCTUnwrap(find(gallery.view,"conversation-gallery-pager") as? UIScrollView)
    gallery.scrollViewWillBeginDragging(pager)
    pager.contentOffset.x=pager.bounds.width*2
    gallery.scrollViewDidEndDecelerating(pager)
    XCTAssertEqual(events.last?.0,"preview")
    XCTAssertEqual(events.last?.1["id"] as? String,"c")
    XCTAssertEqual(events.last?.1["revision"] as? String,"one")
    events=[]
    gallery.scrollViewWillBeginDragging(pager)
    try gallery.update(model:model("a",revision:"successor"),appearance:appearance)
    pager.contentOffset.x=pager.bounds.width*2
    gallery.scrollViewDidEndDecelerating(pager)
    XCTAssertTrue(events.isEmpty)
    XCTAssertEqual(pager.contentOffset.x,0)
    gallery.retire()
  }
}
