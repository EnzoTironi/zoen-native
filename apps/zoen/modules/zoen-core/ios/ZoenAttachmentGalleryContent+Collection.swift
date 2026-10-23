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
}
