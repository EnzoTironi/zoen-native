import AVFoundation
import ImageIO
import Photos
import UIKit

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
  func collectionView(_ collectionView:UICollectionView,canMoveItemAt indexPath:IndexPath)->Bool {
    !retired && visible && !busy && canReorder && selected.count>1 && selected.indices.contains(indexPath.item)
  }
  func collectionView(_ collectionView:UICollectionView,moveItemAt sourceIndexPath:IndexPath,to destinationIndexPath:IndexPath) {
    guard !retired,visible,!busy,canReorder,reorderRevision==revision,
      selected.indices.contains(sourceIndexPath.item),selected[sourceIndexPath.item]==dragId,
      let ids=ZoenAttachmentGalleryLayout.reordered(selected,from:sourceIndexPath.item,to:destinationIndexPath.item) else {
      cancelReorder();strip.reloadData();return
    }
    dragId=nil;reorderRevision=nil
    emit("reorder",payload:["ids":ids]);UISelectionFeedbackGenerator().selectionChanged()
  }

}
